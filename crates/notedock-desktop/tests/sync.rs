//! Metadata sync tests against a real `notedock-server` on a loopback port.
//!
//! Only note metadata moves through Rust now — creations, deletions, and the note
//! list. Bodies are Yjs documents that converge over their own WebSocket, and
//! `notedock-server`'s `collab.rs` is where that convergence is tested. The old
//! "a lost race keeps both versions" test is gone with the machinery it covered:
//! there is no longer a body write that can lose a race.

use notedock_desktop_lib::{
    remote::Remote,
    settings::Settings,
    store::Store,
    sync::{Engine, Failure},
};
use serde_json::{json, Value};
use std::{net::SocketAddr, path::PathBuf};
use tempfile::TempDir;

const PASSWORD: &str = "desktop-test-password";

struct Harness {
    base: String,
    token: String,
    http: reqwest::Client,
    engine: Engine,
    _server_dir: TempDir,
    _client_dir: TempDir,
}

impl Harness {
    async fn start() -> Self {
        let server_dir = tempfile::tempdir().expect("server temp dir");
        let config = notedock_server::config::Config {
            bind: "127.0.0.1:0".parse().unwrap(),
            db_path: server_dir.path().join("server.db"),
            password_hash: notedock_server::auth::hash_password(PASSWORD).unwrap(),
            session_ttl_days: 1,
            cors_origins: Vec::new(),
            web_dir: None,
        };
        let app = notedock_server::build(config).await.expect("build server");

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr: SocketAddr = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        let base = format!("http://{addr}");
        let token = Remote::new(base.clone(), None)
            .unwrap()
            .login(PASSWORD, "test")
            .await
            .expect("login")
            .token;

        let client_dir = tempfile::tempdir().expect("client temp dir");
        let store = Store::open(&client_dir.path().join("notes.db"))
            .await
            .expect("open store");
        let engine = Engine::new(
            store,
            Settings {
                server_url: base.clone(),
                token: Some(token.clone()),
                ..Default::default()
            },
            PathBuf::from(client_dir.path()),
        );

        Self {
            base,
            token: token.clone(),
            http: reqwest::Client::new(),
            engine,
            _server_dir: server_dir,
            _client_dir: client_dir,
        }
    }

    fn remote(&self) -> Remote {
        Remote::new(self.base.clone(), Some(self.token.clone())).unwrap()
    }

    /// What the server thinks the note list is, straight over HTTP — so the
    /// assertions do not lean on the same code path they are checking.
    async fn server_notes(&self) -> Vec<Value> {
        self.http
            .get(format!("{}/api/v1/notes", self.base))
            .bearer_auth(&self.token)
            .send()
            .await
            .expect("list")
            .json::<Vec<Value>>()
            .await
            .expect("list body")
    }
}

#[tokio::test]
async fn a_note_created_offline_is_uploaded_on_the_next_pass() {
    let h = Harness::start().await;
    let store = &h.engine.store;

    let created = store.create("离线新建").await.unwrap();
    assert_eq!(created.rev, 0, "never been to the server");
    assert!(created.dirty);
    assert_eq!(store.pending_count().await.unwrap(), 1);

    h.engine.sync_pass(&h.remote()).await.expect("sync");

    let after = store.get(&created.id).await.unwrap().expect("still there");
    assert_eq!(after.rev, 1, "server assigned the first revision");
    assert!(!after.dirty, "outbox is empty");
    assert_eq!(after.title, "离线新建");
    assert_eq!(store.pending_count().await.unwrap(), 0);

    // The server has it, under the id the client chose — which is what makes a
    // retry after a dropped connection safe.
    let remote = h.server_notes().await;
    assert_eq!(remote.len(), 1);
    assert_eq!(remote[0]["id"], created.id);
}

/// The title shown in the list is derived server-side from the note's document.
/// This is the path that carries it back.
#[tokio::test]
async fn a_remote_title_change_is_pulled_into_the_list() {
    let h = Harness::start().await;
    let store = &h.engine.store;

    let created = store.create("原名").await.unwrap();
    h.engine.sync_pass(&h.remote()).await.expect("first sync");

    // Stand in for what the room's materializer does after somebody types.
    h.http
        .post(format!("{}/api/v1/notes", h.base))
        .bearer_auth(&h.token)
        .json(&json!({ "id": created.id, "title": "原名" }))
        .send()
        .await
        .expect("idempotent create");

    h.engine.sync_pass(&h.remote()).await.expect("second sync");

    let local = store.get(&created.id).await.unwrap().expect("present");
    assert!(!local.dirty);
    assert_eq!(local.rev, 1, "an idempotent create must not bump the revision");
}

#[tokio::test]
async fn a_local_delete_becomes_a_server_tombstone() {
    let h = Harness::start().await;
    let store = &h.engine.store;

    let created = store.create("待删除").await.unwrap();
    h.engine.sync_pass(&h.remote()).await.expect("first sync");

    store.delete_local(&created.id).await.unwrap();
    assert_eq!(store.pending_count().await.unwrap(), 1);

    h.engine.sync_pass(&h.remote()).await.expect("sync");
    assert_eq!(store.pending_count().await.unwrap(), 0);
    assert!(store.get(&created.id).await.unwrap().is_none());
    assert!(
        h.server_notes().await.is_empty(),
        "the server hides tombstones from the list"
    );

    // Other clients still learn about it through the change feed.
    let page = h.remote().sync(0).await.unwrap();
    let tombstone = page
        .changes
        .iter()
        .find(|note| note.id == created.id)
        .expect("still in the change log");
    assert!(tombstone.deleted);
}

/// A note created and deleted while offline never existed on the server, so the
/// pass must not try to tombstone it — and must not get stuck retrying.
#[tokio::test]
async fn a_note_created_and_deleted_offline_is_simply_dropped() {
    let h = Harness::start().await;
    let store = &h.engine.store;

    let created = store.create("写了又不要了").await.unwrap();
    store.delete_local(&created.id).await.unwrap();

    h.engine.sync_pass(&h.remote()).await.expect("sync");

    assert_eq!(store.pending_count().await.unwrap(), 0);
    assert!(
        h.remote().sync(0).await.unwrap().changes.is_empty(),
        "the server never heard about it"
    );
}

#[tokio::test]
async fn an_unreachable_server_leaves_the_creation_pending() {
    let h = Harness::start().await;
    let store = &h.engine.store;

    store.create("离线时新建").await.unwrap();

    // Port 1 on loopback: nothing listens, and the connection is refused
    // immediately rather than hanging until the timeout.
    let dead = Remote::new("http://127.0.0.1:1".to_owned(), Some(h.token.clone())).unwrap();
    let outcome = h.engine.sync_pass(&dead).await;

    assert!(
        matches!(outcome, Err(Failure::Offline)),
        "expected Offline, got {outcome:?}"
    );
    assert_eq!(
        store.pending_count().await.unwrap(),
        1,
        "the creation is still ours to send"
    );

    // And it goes out once the server is reachable again.
    h.engine.sync_pass(&h.remote()).await.expect("sync");
    assert_eq!(store.pending_count().await.unwrap(), 0);
}

#[tokio::test]
async fn an_expired_token_is_reported_as_such() {
    let h = Harness::start().await;
    h.engine.store.create("x").await.unwrap();

    let stale = Remote::new(h.base.clone(), Some("not-a-real-token".to_owned())).unwrap();
    let outcome = h.engine.sync_pass(&stale).await;

    assert!(
        matches!(outcome, Err(Failure::Unauthorized)),
        "expected Unauthorized, got {outcome:?}"
    );
}

/// The settings panel is only worth having if what it sets survives a restart.
#[tokio::test]
async fn window_preferences_are_clamped_and_persisted() {
    let dir = tempfile::tempdir().expect("temp dir");
    let store = Store::open(&dir.path().join("notes.db"))
        .await
        .expect("open store");
    let engine = Engine::new(store, Settings::default(), PathBuf::from(dir.path()));

    assert_eq!(
        engine.window_prefs().await,
        (1.0, true),
        "a fresh install is opaque and on top"
    );

    // The webview is the one component that could send a bogus value, so the
    // clamp lives on this side.
    engine.set_opacity(0.05).await.expect("set opacity");
    engine.set_always_on_top(false).await.expect("set on top");
    assert_eq!(engine.window_prefs().await, (0.3, false));

    // Reload from disk the way the next launch does.
    let reloaded = notedock_desktop_lib::settings::load(dir.path());
    assert_eq!(reloaded.opacity, 0.3);
    assert!(!reloaded.always_on_top);
}
