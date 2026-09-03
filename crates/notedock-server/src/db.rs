//! Pool setup and the one place a database row becomes a [`NoteSummary`].

use crate::error::AppResult;
use notedock_api::{text, NoteSummary};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteRow},
    Row, SqlitePool,
};
use std::{path::Path, time::Duration};

/// Characters of body text kept for the list preview.
pub const PREVIEW_CHARS: usize = 120;

/// sqlx 0.9 only accepts `&'static str` as SQL, so the column list lives inside
/// the finished statement rather than being spliced in with `format!`.
/// [`note_from_row`] expects exactly these columns.
pub const SELECT_NOTE_BY_ID: &str = "SELECT id, title, content_text, rev, \
     updated_at, deleted_at FROM notes WHERE id = ?1";

pub async fn connect(path: &Path) -> anyhow::Result<SqlitePool> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }

    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        // WAL lets the sync poller read while an editor writes.
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_secs(5))
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(options)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;

    Ok(pool)
}

pub fn note_from_row(row: &SqliteRow) -> AppResult<NoteSummary> {
    let content_text: String = row.try_get("content_text")?;
    let deleted_at: Option<String> = row.try_get("deleted_at")?;

    Ok(NoteSummary {
        id: row.try_get("id")?,
        title: row.try_get("title")?,
        preview: text::preview(&content_text, PREVIEW_CHARS),
        rev: row.try_get("rev")?,
        updated_at: row.try_get("updated_at")?,
        deleted: deleted_at.is_some(),
    })
}

pub fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
