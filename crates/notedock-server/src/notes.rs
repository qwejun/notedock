//! Note metadata: list, create, delete, and the incremental sync feed.
//!
//! Bodies are not here. They live in Yjs documents synced over
//! [`crate::ws`], which is why there is no update endpoint and no revision
//! conflict to resolve — two clients editing the same note merge instead of one
//! of them losing.
//!
//! Every write bumps `notes.rev` and appends to `note_changes` in one
//! transaction. Doing one without the other would silently lose a change for
//! every client that syncs incrementally.

use crate::{
    db::{note_from_row, now_rfc3339, SELECT_NOTE_BY_ID},
    error::{AppError, AppResult},
    AppState,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use notedock_api::{CreateNoteRequest, NoteSummary, Seq, SyncResponse};
use serde::Deserialize;
use sqlx::{Row, Sqlite, Transaction};
use uuid::Uuid;

/// Cap on rows returned by the list and sync endpoints. A client that hits it
/// simply comes back for the next page with an advanced cursor.
const PAGE_LIMIT: i64 = 500;

async fn fetch_note<'e, E>(executor: E, id: &str) -> AppResult<NoteSummary>
where
    E: sqlx::Executor<'e, Database = Sqlite>,
{
    let row = sqlx::query(SELECT_NOTE_BY_ID)
        .bind(id)
        .fetch_optional(executor)
        .await?
        .ok_or(AppError::NotFound)?;
    note_from_row(&row)
}

async fn record_change(
    tx: &mut Transaction<'_, Sqlite>,
    note_id: &str,
    rev: i64,
    at: &str,
) -> AppResult<()> {
    sqlx::query("INSERT INTO note_changes (note_id, rev, at) VALUES (?1, ?2, ?3)")
        .bind(note_id)
        .bind(rev)
        .bind(at)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub async fn list(State(state): State<AppState>) -> AppResult<Json<Vec<NoteSummary>>> {
    let rows = sqlx::query(
        "SELECT id, title, content_text, rev, updated_at, deleted_at FROM notes \
         WHERE deleted_at IS NULL ORDER BY updated_at DESC LIMIT ?1",
    )
    .bind(PAGE_LIMIT)
    .fetch_all(&state.pool)
    .await?;

    rows.iter()
        .map(note_from_row)
        .collect::<AppResult<Vec<_>>>()
        .map(Json)
}

/// Reading a tombstone gives a 404. Clients learn about deletions from `/sync`,
/// which is the only place tombstones are meant to surface.
pub async fn get_one(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<NoteSummary>> {
    let note = fetch_note(&state.pool, &id).await?;
    if note.deleted {
        return Err(AppError::NotFound);
    }
    Ok(Json(note))
}

/// Creates the metadata row only. The body starts empty and the creating client
/// writes into it over the WebSocket, which is also what produces the real title.
///
/// Idempotent when the client supplies an `id`: creating the same note twice
/// returns the existing one with 200 instead of failing. That is what lets the
/// desktop app retry after a dropped connection without risking a duplicate, and
/// what lets a note created offline keep its identity.
pub async fn create(
    State(state): State<AppState>,
    Json(req): Json<CreateNoteRequest>,
) -> AppResult<(StatusCode, Json<NoteSummary>)> {
    let id = match req.id.as_deref() {
        Some(given) => {
            // Only accept UUIDs. Ids reach SQL as bind parameters so this is not
            // an injection guard; it keeps clients from inventing id formats the
            // rest of the system does not expect.
            Uuid::parse_str(given.trim())
                .map_err(|_| AppError::BadRequest("id 必须是 UUID".to_owned()))?
                .to_string()
        }
        None => Uuid::now_v7().to_string(),
    };

    let now = now_rfc3339();
    let title = req.title.trim().to_owned();

    let mut tx = state.pool.begin().await?;

    let inserted = sqlx::query(
        "INSERT INTO notes (id, title, content_text, rev, created_at, updated_at) \
         VALUES (?1, ?2, '', 1, ?3, ?3) ON CONFLICT (id) DO NOTHING",
    )
    .bind(&id)
    .bind(&title)
    .bind(&now)
    .execute(&mut *tx)
    .await?
    .rows_affected();

    if inserted == 0 {
        let existing = fetch_note(&mut *tx, &id).await?;
        return Ok((StatusCode::OK, Json(existing)));
    }

    record_change(&mut tx, &id, 1, &now).await?;

    let note = fetch_note(&mut *tx, &id).await?;
    tx.commit().await?;

    Ok((StatusCode::CREATED, Json(note)))
}

pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    let now = now_rfc3339();
    let mut tx = state.pool.begin().await?;

    let deleted = sqlx::query(
        "UPDATE notes SET deleted_at = ?1, updated_at = ?1, rev = rev + 1 \
         WHERE id = ?2 AND deleted_at IS NULL",
    )
    .bind(&now)
    .bind(&id)
    .execute(&mut *tx)
    .await?
    .rows_affected();

    if deleted == 0 {
        // Idempotent: deleting a tombstone is a no-op rather than another
        // revision, or every retry would give clients a change to pull.
        return match sqlx::query_scalar::<_, i64>("SELECT 1 FROM notes WHERE id = ?1")
            .bind(&id)
            .fetch_optional(&mut *tx)
            .await?
        {
            Some(_) => Ok(StatusCode::NO_CONTENT),
            None => Err(AppError::NotFound),
        };
    }

    let rev: i64 = sqlx::query_scalar("SELECT rev FROM notes WHERE id = ?1")
        .bind(&id)
        .fetch_one(&mut *tx)
        .await?;

    record_change(&mut tx, &id, rev, &now).await?;
    tx.commit().await?;

    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct SyncQuery {
    /// Highest change-log position the client already has. `0` means "send me
    /// everything", which is also what a fresh client sends.
    #[serde(default)]
    pub since: Seq,
}

pub async fn sync(
    State(state): State<AppState>,
    Query(query): Query<SyncQuery>,
) -> AppResult<Json<SyncResponse>> {
    // Group by note so a note edited fifty times since the last sync is sent
    // once, at its latest revision, and ordered by that latest position.
    let rows = sqlx::query(
        "SELECT n.id, n.title, n.content_text, n.rev, n.updated_at, n.deleted_at, \
                c.seq AS seq \
         FROM notes n \
         JOIN (SELECT note_id, MAX(seq) AS seq FROM note_changes \
               WHERE seq > ?1 GROUP BY note_id) c ON c.note_id = n.id \
         ORDER BY c.seq ASC LIMIT ?2",
    )
    .bind(query.since)
    .bind(PAGE_LIMIT)
    .fetch_all(&state.pool)
    .await?;

    let cursor = match rows.last() {
        Some(row) => row.try_get("seq")?,
        // Nothing new. Report the global high-water mark, which also repairs a
        // client that somehow sent a cursor from the future.
        None => sqlx::query_scalar::<_, Seq>("SELECT COALESCE(MAX(seq), 0) FROM note_changes")
            .fetch_one(&state.pool)
            .await?,
    };

    let changes = rows
        .iter()
        .map(note_from_row)
        .collect::<AppResult<Vec<_>>>()?;

    Ok(Json(SyncResponse { changes, cursor }))
}
