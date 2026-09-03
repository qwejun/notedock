//! Wire types shared by every NoteDock component.
//!
//! This crate is the single source of truth for the HTTP contract. The server
//! and the desktop client depend on it directly; `cargo test -p notedock-api`
//! regenerates the TypeScript mirror that the Svelte packages import. It has
//! no I/O dependencies on purpose — nothing here should need a runtime.
//!
//! Note *bodies* are deliberately absent. They live in a Yjs document synced
//! over the WebSocket endpoint, which is what makes editing converge without
//! anyone being asked to resolve a conflict. Everything here is metadata.

pub mod text;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value as Json};
use ts_rs::TS;

/// Timestamps cross the wire as RFC 3339 UTC strings, so Rust, TypeScript and
/// SQLite `TEXT` columns all agree without pulling in a date library.
pub type Timestamp = String;

/// Server-assigned metadata revision, bumped whenever a note's title, preview or
/// tombstone state changes. Clients use it to notice that a list row is stale.
/// It is *not* a body version — the Yjs document has no linear revisions.
///
/// Every field of this type carries `#[ts(type = "number")]`: ts-rs maps `i64`
/// to `bigint`, but `JSON.parse` hands back a plain `number`, so the generated
/// TypeScript would be a lie. Revisions never approach 2^53.
pub type Rev = i64;

/// Position in the server's global change log. Clients keep the last cursor
/// they saw and ask for everything after it. See [`Rev`] on the `number` cast.
pub type Seq = i64;

pub const API_PREFIX: &str = "/api/v1";

/// An empty TipTap document.
///
/// Nothing on the wire carries a body any more — this exists so
/// [`text::plain_text`] has something to be tested against, and as the shape a
/// client can fall back to when a document fails to load.
pub fn empty_doc() -> Json {
    json!({ "type": "doc", "content": [] })
}

/// Everything the clients know about a note without opening it.
///
/// One type for both the list and the sync feed: a sync response is just the
/// same rows filtered by change-log position, and a second near-identical struct
/// would only invite the two to drift.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "api.ts")]
pub struct NoteSummary {
    pub id: String,
    /// Derived from the first line of the body on every materialization.
    pub title: String,
    /// Leading plain text, for the list and the command palette.
    pub preview: String,
    #[ts(type = "number")]
    pub rev: Rev,
    pub updated_at: Timestamp,
    /// Tombstone flag. Deleted notes keep flowing through `/sync` so clients can
    /// drop their local copies; the list endpoint omits them.
    pub deleted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "api.ts")]
pub struct CreateNoteRequest {
    /// Client-chosen UUID. Set by the desktop app so a note written while
    /// offline keeps one identity from the moment it is created, and so
    /// retrying the upload cannot produce a duplicate. Omit to let the server
    /// allocate one.
    #[serde(default)]
    pub id: Option<String>,
    /// Provisional name, shown in the list until the first body edit arrives and
    /// the real title is derived. There is no body here: the client writes that
    /// into the note's Yjs document.
    #[serde(default)]
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "api.ts")]
pub struct SyncResponse {
    /// Notes (including tombstones) whose metadata changed after the cursor.
    pub changes: Vec<NoteSummary>,
    /// Cursor to send next time.
    #[ts(type = "number")]
    pub cursor: Seq,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "api.ts")]
pub struct LoginRequest {
    pub password: String,
    /// Human-readable device name, shown when auditing active sessions.
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "api.ts")]
pub struct LoginResponse {
    pub token: String,
    pub expires_at: Timestamp,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "api.ts")]
pub struct AuthStatusResponse {
    pub initialized: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "api.ts")]
pub struct SetupRequest {
    pub password: String,
    #[serde(default)]
    pub label: Option<String>,
}

/// Short-lived credential for opening the note WebSocket.
///
/// The browser's `WebSocket` constructor cannot set an `Authorization` header, so
/// something has to travel in the URL. A single-use ticket that expires in
/// seconds is what travels, never the month-long bearer token.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "api.ts")]
pub struct TicketResponse {
    pub ticket: String,
    pub expires_at: Timestamp,
    /// Absolute `ws://` or `wss://` URL the client should open, already carrying
    /// the ticket. Built by the server so clients never assemble it themselves.
    pub url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "api.ts")]
pub enum ErrorCode {
    BadRequest,
    Unauthorized,
    NotFound,
    TooManyRequests,
    Internal,
}

/// Every non-2xx response uses this shape.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "api.ts")]
pub struct ApiErrorBody {
    pub code: ErrorCode,
    pub message: String,
}
