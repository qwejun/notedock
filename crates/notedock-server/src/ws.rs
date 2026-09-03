//! The note WebSocket, and the short-lived ticket that opens it.
//!
//! Protocol: binary frames, first byte a tag. Deliberately not the y-websocket
//! wire format — both ends of this one are ours, and three message types are
//! easier to get provably right than matching someone else's varint encoding.
//!
//!   1  state vector  — "here is what I have"; sent by both sides on connect
//!   2  diff          — "here is what you are missing", answering a state vector
//!   3  update        — an incremental change
//!
//! The handshake is symmetric: each side sends its state vector, each replies
//! with a diff, and after that everything is an update.

use crate::{
    error::{AppError, AppResult},
    rooms::Room,
    AppState,
};
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Query, State,
    },
    http::HeaderMap,
    response::Response,
    Json,
};
use futures_util::{SinkExt, StreamExt};
use notedock_api::TicketResponse;
use serde::Deserialize;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use uuid::Uuid;

/// Long enough to survive a slow page load, short enough that a ticket left in a
/// proxy log is worthless by the time anyone reads it.
const TICKET_TTL: Duration = Duration::from_secs(30);

const MSG_STATE_VECTOR: u8 = 1;
const MSG_DIFF: u8 = 2;
const MSG_UPDATE: u8 = 3;

/// Issued tickets, held only in memory: they expire in seconds, and unlike the
/// session table there is no persisted copy for a database leak to expose.
pub type Tickets = tokio::sync::Mutex<std::collections::HashMap<String, Instant>>;

/// Exchanges a bearer token for a single-use WebSocket ticket.
///
/// This endpoint sits behind the normal `Authorization` check; the WebSocket
/// itself cannot, because the browser's `WebSocket` constructor has no way to set
/// a header. The ticket is what crosses that gap.
pub async fn issue_ticket(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<Json<TicketResponse>> {
    let ticket = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let now = Instant::now();

    {
        let mut tickets = state.tickets.lock().await;
        tickets.retain(|_, issued| now.duration_since(*issued) < TICKET_TTL);
        tickets.insert(ticket.clone(), now);
    }

    Ok(Json(TicketResponse {
        url: websocket_url(&headers, &ticket),
        expires_at: (chrono::Utc::now() + chrono::Duration::from_std(TICKET_TTL).unwrap())
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        ticket,
    }))
}

/// Builds the URL the client should open.
///
/// Derived from the request rather than configured: behind a TLS-terminating
/// proxy the client must be told `wss://`, and only the proxy's forwarded headers
/// know that. Assembling it here also means clients never guess at the path.
fn websocket_url(headers: &HeaderMap, ticket: &str) -> String {
    let header = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());

    let secure = header("x-forwarded-proto")
        .map(|proto| proto.split(',').next().unwrap_or("").trim() == "https")
        .unwrap_or(false);
    let scheme = if secure { "wss" } else { "ws" };
    let host = header("x-forwarded-host")
        .or_else(|| header("host"))
        .unwrap_or("localhost");

    format!(
        "{scheme}://{host}{}/ws?ticket={ticket}",
        notedock_api::API_PREFIX
    )
}

#[derive(Debug, Deserialize)]
pub struct Connect {
    ticket: String,
    /// Note whose document this connection edits.
    note: String,
}

pub async fn upgrade(
    State(state): State<AppState>,
    Query(params): Query<Connect>,
    ws: WebSocketUpgrade,
) -> AppResult<Response> {
    // Single use: taken out of the map whether or not it turns out to be fresh.
    let issued = {
        let mut tickets = state.tickets.lock().await;
        tickets.remove(&params.ticket)
    };
    let fresh = issued
        .map(|at| Instant::now().duration_since(at) < TICKET_TTL)
        .unwrap_or(false);
    if !fresh {
        return Err(AppError::Unauthorized);
    }

    // A room for a note that does not exist would accept edits nobody can ever
    // list, so check before upgrading while we can still answer with a status.
    let exists: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM notes WHERE id = ?1 AND deleted_at IS NULL")
            .bind(&params.note)
            .fetch_optional(&state.pool)
            .await?;
    if exists.is_none() {
        return Err(AppError::NotFound);
    }

    let note_id = params.note;
    Ok(ws.on_upgrade(move |socket| serve(socket, state, note_id)))
}

fn frame(kind: u8, payload: &[u8]) -> Message {
    let mut buf = Vec::with_capacity(payload.len() + 1);
    buf.push(kind);
    buf.extend_from_slice(payload);
    Message::Binary(buf.into())
}

async fn serve(socket: WebSocket, state: AppState, note_id: String) {
    let room = match state.rooms.join(&note_id).await {
        Ok(room) => room,
        Err(err) => {
            tracing::error!(note = %note_id, %err, "failed to open room");
            return;
        }
    };

    let connection = state.rooms.next_connection_id();
    let mut relayed = room.subscribe();
    let (mut sink, mut stream) = socket.split();

    // Open with our state vector so the client knows what to send.
    if sink
        .send(frame(MSG_STATE_VECTOR, &room.state_vector()))
        .await
        .is_err()
    {
        state.rooms.leave(&room).await;
        return;
    }

    loop {
        tokio::select! {
            incoming = stream.next() => {
                let Some(Ok(message)) = incoming else { break };
                match handle(&room, connection, &state, message).await {
                    Ok(Some(reply)) => {
                        if sink.send(reply).await.is_err() {
                            break;
                        }
                    }
                    Ok(None) => {}
                    Err(Closed) => break,
                }
            }
            update = relayed.recv() => {
                match update {
                    // Skip our own echo.
                    Ok(relay) if relay.from == connection => {}
                    Ok(relay) => {
                        if sink.send(frame(MSG_UPDATE, &relay.update)).await.is_err() {
                            break;
                        }
                    }
                    // Lagged: this connection fell behind. Closing makes it
                    // reconnect and resync from its state vector, which is
                    // cheaper and safer than guessing what it missed.
                    Err(_) => break,
                }
            }
        }
    }

    state.rooms.leave(&room).await;
}

/// Signals that the connection should be torn down.
struct Closed;

async fn handle(
    room: &Arc<Room>,
    connection: usize,
    state: &AppState,
    message: Message,
) -> Result<Option<Message>, Closed> {
    let payload = match message {
        Message::Binary(bytes) => bytes,
        Message::Close(_) => return Err(Closed),
        // Text, ping and pong are not part of this protocol; axum answers pings.
        _ => return Ok(None),
    };

    let Some((&kind, body)) = payload.split_first() else {
        return Ok(None);
    };

    match kind {
        MSG_STATE_VECTOR => match room.diff(body) {
            Ok(diff) => Ok(Some(frame(MSG_DIFF, &diff))),
            Err(err) => {
                tracing::warn!(note = %room.note_id, %err, "bad state vector");
                Err(Closed)
            }
        },
        MSG_DIFF | MSG_UPDATE => {
            if let Err(err) = room.apply(connection, body.to_vec(), &state.pool).await {
                tracing::warn!(note = %room.note_id, %err, "rejected update");
                return Err(Closed);
            }
            Ok(None)
        }
        other => {
            tracing::warn!(note = %room.note_id, other, "unknown frame type");
            Ok(None)
        }
    }
}
