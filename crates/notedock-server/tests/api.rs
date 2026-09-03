//! End-to-end tests against a real router and a real (temporary) SQLite file.
//!
//! An on-disk database rather than `:memory:` is deliberate: in-memory SQLite is
//! per-connection, and these tests exercise the pool.

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
    Router,
};
use notedock_server::{auth, config::Config};
use serde_json::{json, Value};
use tempfile::TempDir;
use tower::ServiceExt;

const PASSWORD: &str = "correct horse battery staple";

struct Harness {
    app: Router,
    /// Held so the temporary directory outlives the test.
    _dir: TempDir,
}

async fn harness() -> Harness {
    let dir = tempfile::tempdir().expect("temp dir");
    let config = Config {
        bind: "127.0.0.1:0".parse().unwrap(),
        db_path: dir.path().join("notedock.db"),
        password_hash: auth::hash_password(PASSWORD).unwrap(),
        session_ttl_days: 1,
        cors_origins: Vec::new(),
        web_dir: None,
    };
    let app = notedock_server::build(config).await.expect("build app");
    Harness { app, _dir: dir }
}

impl Harness {
    async fn send(&self, req: Request<Body>) -> (StatusCode, Value) {
        let res = self.app.clone().oneshot(req).await.expect("response");
        let status = res.status();
        let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .expect("body");
        let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, body)
    }

    async fn login(&self) -> String {
        let (status, body) = self
            .send(request(
                Method::POST,
                "/api/v1/auth/login",
                None,
                Some(json!({ "password": PASSWORD })),
            ))
            .await;
        assert_eq!(status, StatusCode::OK, "login failed: {body}");
        body["token"].as_str().expect("token").to_owned()
    }
}

fn request(method: Method, path: &str, token: Option<&str>, body: Option<Value>) -> Request<Body> {
    let mut builder = Request::builder().method(method).uri(path);
    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    match body {
        Some(value) => builder
            .header("content-type", "application/json")
            .body(Body::from(value.to_string()))
            .unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    }
}

async fn create_note(h: &Harness, token: &str, title: &str) -> Value {
    let (status, body) = h
        .send(request(
            Method::POST,
            "/api/v1/notes",
            Some(token),
            Some(json!({ "title": title })),
        ))
        .await;
    assert_eq!(status, StatusCode::CREATED, "create failed: {body}");
    body
}

#[tokio::test]
async fn healthz_needs_no_token() {
    let h = harness().await;
    let (status, _) = h.send(request(Method::GET, "/healthz", None, None)).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn wrong_password_is_rejected() {
    let h = harness().await;
    let (status, body) = h
        .send(request(
            Method::POST,
            "/api/v1/auth/login",
            None,
            Some(json!({ "password": "wrong" })),
        ))
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["code"], "unauthorized");
}

#[tokio::test]
async fn notes_require_a_token() {
    let h = harness().await;
    for req in [
        request(Method::GET, "/api/v1/notes", None, None),
        request(Method::GET, "/api/v1/sync?since=0", None, None),
        request(Method::POST, "/api/v1/ws-ticket", None, None),
    ] {
        let (status, _) = h.send(req).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
}

/// A note starts with only the name the client offered; the real title arrives
/// when the body is first edited over the WebSocket.
#[tokio::test]
async fn create_keeps_the_provisional_title_until_the_body_says_otherwise() {
    let h = harness().await;
    let token = h.login().await;

    let note = create_note(&h, &token, "会议记录").await;
    assert_eq!(note["title"], "会议记录");
    assert_eq!(note["preview"], "", "no body yet");
    assert_eq!(note["rev"], 1);
    assert_eq!(note["deleted"], false);

    let (status, list) = h
        .send(request(Method::GET, "/api/v1/notes", Some(&token), None))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["title"], "会议记录");
}

/// What makes an offline-created note safe to upload: the desktop app allocates
/// the id itself, so retrying after a dropped connection cannot double-create.
#[tokio::test]
async fn create_with_a_client_id_is_idempotent() {
    let h = harness().await;
    let token = h.login().await;
    let id = "01a05c05-4658-76b0-bcf4-e06354b030c1";
    let body = json!({ "id": id, "title": "离线创建" });

    let (status, first) = h
        .send(request(Method::POST, "/api/v1/notes", Some(&token), Some(body.clone())))
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(first["id"], id);

    let (status, again) = h
        .send(request(Method::POST, "/api/v1/notes", Some(&token), Some(body)))
        .await;
    assert_eq!(status, StatusCode::OK, "retry is not a failure");
    assert_eq!(again["rev"], 1, "retry must not bump the revision");

    let (_, sync) = h
        .send(request(Method::GET, "/api/v1/sync?since=0", Some(&token), None))
        .await;
    assert_eq!(sync["changes"].as_array().unwrap().len(), 1);
    assert_eq!(sync["cursor"], 1);
}

#[tokio::test]
async fn create_rejects_an_id_that_is_not_a_uuid() {
    let h = harness().await;
    let token = h.login().await;

    let (status, body) = h
        .send(request(
            Method::POST,
            "/api/v1/notes",
            Some(&token),
            Some(json!({ "id": "../../etc/passwd", "title": "x" })),
        ))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "bad_request");
}

#[tokio::test]
async fn delete_tombstones_the_note_and_is_idempotent() {
    let h = harness().await;
    let token = h.login().await;
    let id = create_note(&h, &token, "待删除").await["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let path = format!("/api/v1/notes/{id}");

    let (status, _) = h
        .send(request(Method::DELETE, &path, Some(&token), None))
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, _) = h
        .send(request(Method::GET, &path, Some(&token), None))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (_, list) = h
        .send(request(Method::GET, "/api/v1/notes", Some(&token), None))
        .await;
    assert!(list.as_array().unwrap().is_empty(), "list hides tombstones");

    // Sync still carries it, so other clients learn to drop their copy.
    let (_, sync) = h
        .send(request(Method::GET, "/api/v1/sync?since=0", Some(&token), None))
        .await;
    assert_eq!(sync["changes"][0]["deleted"], true);
    let cursor_after_delete = sync["cursor"].as_i64().unwrap();

    let (status, _) = h
        .send(request(Method::DELETE, &path, Some(&token), None))
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (_, sync) = h
        .send(request(Method::GET, "/api/v1/sync?since=0", Some(&token), None))
        .await;
    assert_eq!(
        sync["cursor"].as_i64().unwrap(),
        cursor_after_delete,
        "deleting a tombstone must not manufacture another change"
    );
}

#[tokio::test]
async fn sync_is_incremental_and_the_cursor_holds_steady() {
    let h = harness().await;
    let token = h.login().await;

    create_note(&h, &token, "笔记一").await;
    create_note(&h, &token, "笔记二").await;

    let sync = |since: i64| {
        request(
            Method::GET,
            &format!("/api/v1/sync?since={since}"),
            Some(&token),
            None,
        )
    };

    let (status, body) = h.send(sync(0)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["changes"].as_array().unwrap().len(), 2);
    let cursor = body["cursor"].as_i64().unwrap();
    assert_eq!(cursor, 2);

    // Caught up: no changes, and the cursor must not drift.
    let (_, body) = h.send(sync(cursor)).await;
    assert!(body["changes"].as_array().unwrap().is_empty());
    assert_eq!(body["cursor"].as_i64().unwrap(), cursor);
}

/// The ticket is the only thing that can authenticate a WebSocket, so it must be
/// spendable exactly once. The upgrade itself needs a real TCP connection, so
/// that half lives in `collab.rs`; here we only check the ticket is issued and
/// that the endpoint is reachable without a bearer header.
#[tokio::test]
async fn a_websocket_ticket_is_issued_with_a_ready_to_open_url() {
    let h = harness().await;
    let token = h.login().await;

    let (status, body) = h
        .send(request(Method::POST, "/api/v1/ws-ticket", Some(&token), None))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["ticket"].as_str().is_some_and(|t| t.len() == 64));
    assert!(
        body["url"].as_str().unwrap().contains("/api/v1/ws?ticket="),
        "url should be ready to open: {}",
        body["url"]
    );
}
