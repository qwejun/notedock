//! The local note list.
//!
//! Note *bodies* are not here. Each one is a Yjs document that the webview holds
//! and caches in IndexedDB, synced over its own WebSocket. This table is the
//! offline note list: enough to open the app, search, and pick a note with no
//! network at all.
//!
//! `dirty` therefore means only "this note's creation or deletion has not reached
//! the server yet" — never "the body has unsent edits", which is a state that no
//! longer exists.

use anyhow::Context;
use notedock_api::NoteSummary;
use serde::Serialize;
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteRow},
    Row, SqlitePool,
};
use std::{collections::HashSet, path::Path, time::Duration};
use ts_rs::TS;

const CURSOR_KEY: &str = "sync_cursor";

/// Name of the database inside the app-data directory. Shared with the settings
/// panel, which shows the full path so the cache is findable.
pub const DB_FILE: &str = "notes.db";

const SELECT_NOTE: &str = "SELECT id, title, preview, rev, updated_at, dirty \
     FROM notes WHERE id = ?1 AND deleted = 0";

/// A note as the floating window sees it: metadata only, plus `dirty` so the sync
/// dot can be honest about a creation that has not been uploaded.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "desktop.ts")]
pub struct LocalNote {
    pub id: String,
    pub title: String,
    pub preview: String,
    #[ts(type = "number")]
    pub rev: i64,
    pub updated_at: String,
    pub dirty: bool,
}

fn local_note(row: &SqliteRow) -> anyhow::Result<LocalNote> {
    Ok(LocalNote {
        id: row.try_get("id")?,
        title: row.try_get("title")?,
        preview: row.try_get("preview")?,
        rev: row.try_get("rev")?,
        updated_at: row.try_get("updated_at")?,
        dirty: row.try_get::<i64, _>("dirty")? != 0,
    })
}

pub fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

#[derive(Clone)]
pub struct Store {
    pool: SqlitePool,
}

impl Store {
    pub async fn open(path: &Path) -> anyhow::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }

        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .busy_timeout(Duration::from_secs(5));

        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(options)
            .await?;

        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Self { pool })
    }

    pub async fn list(&self) -> anyhow::Result<Vec<NoteSummary>> {
        let rows = sqlx::query(
            "SELECT id, title, preview, rev, updated_at FROM notes \
             WHERE deleted = 0 ORDER BY updated_at DESC",
        )
        .fetch_all(&self.pool)
        .await?;

        rows.iter()
            .map(|row| {
                Ok(NoteSummary {
                    id: row.try_get("id")?,
                    title: row.try_get("title")?,
                    preview: row.try_get("preview")?,
                    rev: row.try_get("rev")?,
                    updated_at: row.try_get("updated_at")?,
                    deleted: false,
                })
            })
            .collect()
    }

    pub async fn get(&self, id: &str) -> anyhow::Result<Option<LocalNote>> {
        let row = sqlx::query(SELECT_NOTE)
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        row.as_ref().map(local_note).transpose()
    }

    /// Creates a note locally with a client-allocated id, so it can be opened and
    /// typed into before the server has ever heard of it. `rev = 0` marks it as
    /// never uploaded; the sync loop turns that into a POST.
    pub async fn create(&self, title: &str) -> anyhow::Result<LocalNote> {
        let id = uuid::Uuid::now_v7().to_string();
        let now = now_rfc3339();
        let title = title.trim();

        sqlx::query(
            "INSERT INTO notes (id, title, preview, rev, updated_at, dirty) \
             VALUES (?1, ?2, '', 0, ?3, 1)",
        )
        .bind(&id)
        .bind(title)
        .bind(&now)
        .execute(&self.pool)
        .await?;

        self.get(&id)
            .await?
            .context("note vanished immediately after insert")
    }

    /// Marks a note deleted locally and dirty, so the tombstone is pushed too.
    pub async fn delete_local(&self, id: &str) -> anyhow::Result<()> {
        sqlx::query("UPDATE notes SET deleted = 1, dirty = 1, updated_at = ?1 WHERE id = ?2")
            .bind(now_rfc3339())
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Creations and deletions still to reach the server, oldest first.
    pub async fn pending(&self) -> anyhow::Result<Vec<Pending>> {
        let rows = sqlx::query(
            "SELECT id, title, rev, deleted FROM notes WHERE dirty = 1 ORDER BY updated_at ASC",
        )
        .fetch_all(&self.pool)
        .await?;

        rows.iter()
            .map(|row| {
                Ok(Pending {
                    id: row.try_get("id")?,
                    title: row.try_get("title")?,
                    rev: row.try_get("rev")?,
                    deleted: row.try_get::<i64, _>("deleted")? != 0,
                })
            })
            .collect()
    }

    pub async fn pending_count(&self) -> anyhow::Result<i64> {
        Ok(
            sqlx::query_scalar("SELECT COUNT(*) FROM notes WHERE dirty = 1")
                .fetch_one(&self.pool)
                .await?,
        )
    }

    /// The server accepted a create, so this note now exists remotely.
    pub async fn mark_pushed(&self, note: &NoteSummary) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE notes SET title = ?1, preview = ?2, rev = ?3, updated_at = ?4, \
             deleted = ?5, dirty = 0 WHERE id = ?6",
        )
        .bind(&note.title)
        .bind(&note.preview)
        .bind(note.rev)
        .bind(&note.updated_at)
        .bind(i64::from(note.deleted))
        .bind(&note.id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// A note the server says is gone. Nothing left to push.
    pub async fn mark_pushed_missing(&self, id: &str) -> anyhow::Result<()> {
        sqlx::query("UPDATE notes SET deleted = 1, dirty = 0 WHERE id = ?1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Writes metadata pulled from the server.
    ///
    /// A locally dirty row is left alone: its own push decides its fate, and
    /// overwriting it here would resurrect a note the user just deleted offline.
    /// Note that there is no conflict to resolve — the *body* converges through
    /// Yjs, and titles are derived from it server-side.
    pub async fn apply_remote(&self, note: &NoteSummary) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO notes (id, title, preview, rev, updated_at, deleted, dirty) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0) \
             ON CONFLICT (id) DO UPDATE SET \
               title = excluded.title, preview = excluded.preview, rev = excluded.rev, \
               updated_at = excluded.updated_at, deleted = excluded.deleted \
             WHERE notes.dirty = 0",
        )
        .bind(&note.id)
        .bind(&note.title)
        .bind(&note.preview)
        .bind(note.rev)
        .bind(&note.updated_at)
        .bind(i64::from(note.deleted))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn cursor(&self) -> anyhow::Result<i64> {
        let raw: Option<String> = sqlx::query_scalar("SELECT v FROM state WHERE k = ?1")
            .bind(CURSOR_KEY)
            .fetch_optional(&self.pool)
            .await?;
        Ok(raw.and_then(|v| v.parse().ok()).unwrap_or(0))
    }

    pub async fn set_cursor(&self, cursor: i64) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO state (k, v) VALUES (?1, ?2) \
             ON CONFLICT (k) DO UPDATE SET v = excluded.v",
        )
        .bind(CURSOR_KEY)
        .bind(cursor.to_string())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Called after logging in to a different server, whose ids and change-log
    /// positions have nothing to do with the ones cached here.
    pub async fn clear(&self) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM notes").execute(&mut *tx).await?;
        sqlx::query("DELETE FROM state").execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Reconciles clean local rows against an authoritative server list. Dirty
    /// rows are preserved because they may be offline creations waiting to be
    /// uploaded; every clean row absent from the server is stale data from a
    /// previous server/session and can be removed safely.
    pub async fn reconcile(&self, notes: &[NoteSummary]) -> anyhow::Result<()> {
        let ids: HashSet<&str> = notes.iter().map(|note| note.id.as_str()).collect();
        let local: Vec<String> = sqlx::query_scalar("SELECT id FROM notes WHERE dirty = 0")
            .fetch_all(&self.pool)
            .await?;
        let mut tx = self.pool.begin().await?;
        for id in local {
            if !ids.contains(id.as_str()) {
                sqlx::query("DELETE FROM notes WHERE id = ?1 AND dirty = 0")
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
            }
        }
        tx.commit().await?;

        for note in notes {
            self.apply_remote(note).await?;
        }
        Ok(())
    }
}

/// A local creation or deletion still to be sent. `rev == 0` means the note has
/// never reached the server, so it needs a create rather than a tombstone.
#[derive(Debug, Clone)]
pub struct Pending {
    pub id: String,
    pub title: String,
    pub rev: i64,
    pub deleted: bool,
}
