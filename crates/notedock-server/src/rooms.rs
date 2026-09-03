//! Live Yjs rooms: one per note that somebody currently has open.
//!
//! A room owns the authoritative document, fans updates out to every other
//! connection, and appends each update to `note_updates` so the document survives
//! a restart. It also re-derives the note's title and preview, but on a timer
//! rather than per keystroke — a change-log entry for every character typed would
//! make the metadata feed useless.

use crate::{db::now_rfc3339, ydoc};
use notedock_api::text;
use sqlx::SqlitePool;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::sync::{broadcast, Mutex};
use yrs::{
    updates::{decoder::Decode, encoder::Encode},
    Doc, ReadTxn, StateVector, Transact, Update,
};

/// How often changed rooms have their metadata re-derived.
const FLUSH_INTERVAL: Duration = Duration::from_secs(2);

/// Once a note's update log passes this, it is merged into a single row. Keeps
/// cold-start replay bounded no matter how long a note has been edited.
const COMPACT_AFTER: i64 = 300;

/// Slow readers are dropped rather than allowed to stall the room; a dropped
/// receiver resyncs from its state vector on the next reconnect.
const BROADCAST_CAPACITY: usize = 128;

/// An update to relay, tagged with the connection it came from so that
/// connection does not receive its own echo.
#[derive(Clone)]
pub struct Relay {
    pub from: usize,
    pub update: Arc<Vec<u8>>,
}

pub struct Room {
    pub note_id: String,
    doc: Doc,
    tx: broadcast::Sender<Relay>,
    /// Set by every applied update, cleared once the flusher has materialized.
    dirty: AtomicBool,
    /// Live connections. At zero the room is flushed and evicted.
    members: AtomicUsize,
}

impl Room {
    pub fn subscribe(&self) -> broadcast::Receiver<Relay> {
        self.tx.subscribe()
    }

    /// This room's state vector, for the peer to diff against.
    pub fn state_vector(&self) -> Vec<u8> {
        self.doc.transact().state_vector().encode_v1()
    }

    /// Everything this room has that the peer's state vector does not.
    pub fn diff(&self, peer_state_vector: &[u8]) -> anyhow::Result<Vec<u8>> {
        let peer = StateVector::decode_v1(peer_state_vector)?;
        Ok(self.doc.transact().encode_diff_v1(&peer))
    }

    /// Applies an update, persists it, and relays it to the other connections.
    ///
    /// An update that changes nothing is dropped on the floor. Clients legitimately
    /// send those — the handshake diff from a client with an empty document is two
    /// bytes of nothing — and relaying them would wake every other connection for
    /// no reason and append dead rows to the log.
    pub async fn apply(&self, from: usize, bytes: Vec<u8>, pool: &SqlitePool) -> anyhow::Result<()> {
        let changed = {
            let before = self.doc.transact().state_vector();
            let update = Update::decode_v1(&bytes)?;
            let mut txn = self.doc.transact_mut();
            txn.apply_update(update)?;
            txn.state_vector() != before
        };
        if !changed {
            return Ok(());
        }

        sqlx::query("INSERT INTO note_updates (note_id, data, at) VALUES (?1, ?2, ?3)")
            .bind(&self.note_id)
            .bind(&bytes)
            .bind(now_rfc3339())
            .execute(pool)
            .await?;

        self.dirty.store(true, Ordering::Relaxed);
        // Err just means nobody else is listening.
        let _ = self.tx.send(Relay {
            from,
            update: Arc::new(bytes),
        });

        Ok(())
    }

    /// Re-derives title and preview from the document and writes them to `notes`,
    /// appending a change-log entry so other clients' lists refresh.
    async fn materialize(&self, pool: &SqlitePool) -> anyhow::Result<()> {
        let body = ydoc::plain_text(&self.doc);
        let collaborative_title = ydoc::title(&self.doc);
        let title = if collaborative_title.is_empty() {
            text::derive_title(&body)
        } else {
            collaborative_title
        };
        let now = now_rfc3339();

        let mut tx = pool.begin().await?;

        // `title` is only overwritten once the body has something in it, so a
        // note named from the palette keeps that name until it is typed into.
        let updated = sqlx::query(
            "UPDATE notes SET content_text = ?1, \
                 title = CASE WHEN ?2 = '' THEN title ELSE ?2 END, \
                 rev = rev + 1, updated_at = ?3 \
             WHERE id = ?4 AND deleted_at IS NULL",
        )
        .bind(&body)
        .bind(&title)
        .bind(&now)
        .bind(&self.note_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();

        if updated == 0 {
            // Deleted while being edited. Nothing to announce.
            return Ok(());
        }

        let rev: i64 = sqlx::query_scalar("SELECT rev FROM notes WHERE id = ?1")
            .bind(&self.note_id)
            .fetch_one(&mut *tx)
            .await?;

        sqlx::query("INSERT INTO note_changes (note_id, rev, at) VALUES (?1, ?2, ?3)")
            .bind(&self.note_id)
            .bind(rev)
            .bind(&now)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        Ok(())
    }

    /// Replaces a long update log with a single equivalent row.
    async fn compact(&self, pool: &SqlitePool) -> anyhow::Result<()> {
        let rows: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM note_updates WHERE note_id = ?1")
                .bind(&self.note_id)
                .fetch_one(pool)
                .await?;
        if rows <= COMPACT_AFTER {
            return Ok(());
        }

        let snapshot = self.doc.transact().encode_diff_v1(&StateVector::default());
        let mut tx = pool.begin().await?;
        sqlx::query("DELETE FROM note_updates WHERE note_id = ?1")
            .bind(&self.note_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO note_updates (note_id, data, at) VALUES (?1, ?2, ?3)")
            .bind(&self.note_id)
            .bind(&snapshot)
            .bind(now_rfc3339())
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;

        tracing::debug!(note = %self.note_id, rows, "compacted update log");
        Ok(())
    }
}

/// Registry of live rooms.
pub struct Rooms {
    pool: SqlitePool,
    rooms: Mutex<HashMap<String, Arc<Room>>>,
    next_connection: AtomicUsize,
}

impl Rooms {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            rooms: Mutex::new(HashMap::new()),
            next_connection: AtomicUsize::new(1),
        }
    }

    /// Identifier used to keep a connection from receiving its own updates back.
    pub fn next_connection_id(&self) -> usize {
        self.next_connection.fetch_add(1, Ordering::Relaxed)
    }

    /// Joins — loading and replaying the note's update log if this is the first
    /// connection. Callers must pair this with [`Self::leave`].
    pub async fn join(&self, note_id: &str) -> anyhow::Result<Arc<Room>> {
        let mut rooms = self.rooms.lock().await;

        if let Some(room) = rooms.get(note_id) {
            room.members.fetch_add(1, Ordering::Relaxed);
            return Ok(Arc::clone(room));
        }

        let updates: Vec<Vec<u8>> =
            sqlx::query_scalar("SELECT data FROM note_updates WHERE note_id = ?1 ORDER BY id")
                .bind(note_id)
                .fetch_all(&self.pool)
                .await?;

        let doc = if updates.is_empty() {
            // Notes written before the Yjs migration have no update log. Their
            // plain-text snapshot is still available, so bootstrap it once on
            // first open and let all current clients converge normally after.
            let legacy: Option<(String, String)> = sqlx::query_as(
                "SELECT content_text, title FROM notes WHERE id = ?1 AND deleted_at IS NULL",
            )
            .bind(note_id)
            .fetch_optional(&self.pool)
            .await?;
            let (body, title) = legacy.unwrap_or_default();
            let doc = ydoc::from_legacy_text(&body, &title);
            if !body.trim().is_empty() || !title.trim().is_empty() {
                let snapshot = doc.transact().encode_diff_v1(&StateVector::default());
                sqlx::query("INSERT INTO note_updates (note_id, data, at) VALUES (?1, ?2, ?3)")
                    .bind(note_id)
                    .bind(snapshot)
                    .bind(now_rfc3339())
                    .execute(&self.pool)
                    .await?;
            }
            doc
        } else {
            ydoc::replay(&updates)?
        };
        let (tx, _) = broadcast::channel(BROADCAST_CAPACITY);
        let room = Arc::new(Room {
            note_id: note_id.to_owned(),
            doc,
            tx,
            dirty: AtomicBool::new(false),
            members: AtomicUsize::new(1),
        });

        tracing::debug!(note = %note_id, updates = updates.len(), "room opened");
        rooms.insert(note_id.to_owned(), Arc::clone(&room));
        Ok(room)
    }

    /// Drops a connection. The last one out flushes and evicts the room, so an
    /// idle server holds no documents in memory.
    pub async fn leave(&self, room: &Arc<Room>) {
        if room.members.fetch_sub(1, Ordering::Relaxed) > 1 {
            return;
        }

        let mut rooms = self.rooms.lock().await;
        // Re-check under the lock: someone may have joined in the meantime.
        if room.members.load(Ordering::Relaxed) > 0 {
            return;
        }
        rooms.remove(&room.note_id);
        drop(rooms);

        if room.dirty.swap(false, Ordering::Relaxed) {
            if let Err(err) = room.materialize(&self.pool).await {
                tracing::error!(note = %room.note_id, %err, "failed to materialize on close");
            }
        }
        if let Err(err) = room.compact(&self.pool).await {
            tracing::error!(note = %room.note_id, %err, "failed to compact update log");
        }
        tracing::debug!(note = %room.note_id, "room closed");
    }

    async fn flush_dirty(&self) {
        let snapshot: Vec<Arc<Room>> = {
            let rooms = self.rooms.lock().await;
            rooms.values().cloned().collect()
        };

        for room in snapshot {
            if !room.dirty.swap(false, Ordering::Relaxed) {
                continue;
            }
            if let Err(err) = room.materialize(&self.pool).await {
                // Put the flag back so the next tick tries again.
                room.dirty.store(true, Ordering::Relaxed);
                tracing::error!(note = %room.note_id, %err, "failed to materialize");
            }
        }
    }
}

/// Starts the metadata flusher. One task for the whole server rather than one per
/// room, so room lifetime stays a plain reference count.
pub fn spawn_flusher(rooms: Arc<Rooms>) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(FLUSH_INTERVAL);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;
            rooms.flush_dirty().await;
        }
    });
}
