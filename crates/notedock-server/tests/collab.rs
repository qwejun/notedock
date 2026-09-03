//! Collaboration tests: two clients on one note, over a real WebSocket.
//!
//! The clients here are `yrs` documents driven by hand — the same CRDT the browser
//! runs. What is being checked is the thing the whole Yjs migration exists for: two
//! concurrent edits to one note both survive, with nobody asked to resolve
//! anything.

use futures_util::{SinkExt, StreamExt};
use notedock_server::{auth, config::Config, ydoc};
use serde_json::{json, Value};
use std::net::SocketAddr;
use tempfile::TempDir;
use tokio::net::TcpStream;
use tokio_tungstenite::{tungstenite::Message, MaybeTlsStream, WebSocketStream};
use yrs::{
    types::xml::{XmlElementPrelim, XmlFragment, XmlTextPrelim},
    updates::{decoder::Decode, encoder::Encode},
    Doc, ReadTxn, StateVector, Transact, Update,
};

/// Frame tags, mirroring `notedock_server::ws`.
const MSG_STATE_VECTOR: u8 = 1;
const MSG_DIFF: u8 = 2;
const MSG_UPDATE: u8 = 3;

const PASSWORD: &str = "collab-test-password";

struct Server {
    base: String,
    token: String,
    http: reqwest::Client,
    _dir: TempDir,
}

impl Server {
    async fn start() -> Self {
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

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr: SocketAddr = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        let http = reqwest::Client::new();
        let base = format!("http://{addr}");
        let token: String = http
            .post(format!("{base}/api/v1/auth/login"))
            .json(&json!({ "password": PASSWORD }))
            .send()
            .await
            .expect("login")
            .json::<Value>()
            .await
            .expect("login body")["token"]
            .as_str()
            .expect("token")
            .to_owned();

        Self {
            base,
            token,
            http,
            _dir: dir,
        }
    }

    async fn create_note(&self, title: &str) -> String {
        self.http
            .post(format!("{}/api/v1/notes", self.base))
            .bearer_auth(&self.token)
            .json(&json!({ "title": title }))
            .send()
            .await
            .expect("create")
            .json::<Value>()
            .await
            .expect("create body")["id"]
            .as_str()
            .expect("id")
            .to_owned()
    }

    async fn note(&self, id: &str) -> Value {
        self.http
            .get(format!("{}/api/v1/notes/{id}", self.base))
            .bearer_auth(&self.token)
            .send()
            .await
            .expect("get note")
            .json()
            .await
            .expect("note body")
    }

    /// Opens an authenticated document socket the way a real client does: swap the
    /// bearer token for a single-use ticket, then connect to the URL it hands back.
    async fn connect(&self, note_id: &str) -> Client {
        let ticket: Value = self
            .http
            .post(format!("{}/api/v1/ws-ticket", self.base))
            .bearer_auth(&self.token)
            .send()
            .await
            .expect("ticket")
            .json()
            .await
            .expect("ticket body");

        let url = format!("{}&note={note_id}", ticket["url"].as_str().expect("url"));
        let (socket, _) = tokio_tungstenite::connect_async(&url)
            .await
            .expect("websocket");

        Client {
            socket,
            doc: Doc::new(),
        }
    }
}

struct Client {
    socket: WebSocketStream<MaybeTlsStream<TcpStream>>,
    doc: Doc,
}

impl Client {
    async fn send(&mut self, kind: u8, payload: &[u8]) {
        let mut frame = Vec::with_capacity(payload.len() + 1);
        frame.push(kind);
        frame.extend_from_slice(payload);
        self.socket
            .send(Message::Binary(frame.into()))
            .await
            .expect("send");
    }

    /// Reads one frame and folds it into the local document. Returns the tag.
    async fn recv(&mut self) -> u8 {
        loop {
            let message = self
                .socket
                .next()
                .await
                .expect("stream open")
                .expect("frame");
            if let Message::Binary(bytes) = message {
                let (&kind, body) = bytes.split_first().expect("non-empty frame");
                if kind == MSG_DIFF || kind == MSG_UPDATE {
                    let update = Update::decode_v1(body).expect("decode update");
                    self.doc.transact_mut().apply_update(update).expect("apply");
                } else if kind == MSG_STATE_VECTOR {
                    // The server opens with its state vector; answer with what it
                    // is missing, then ask for what we are missing.
                    let peer = StateVector::decode_v1(body).expect("decode sv");
                    let diff = self.doc.transact().encode_diff_v1(&peer);
                    self.send(MSG_DIFF, &diff).await;
                    let ours = self.doc.transact().state_vector().encode_v1();
                    self.send(MSG_STATE_VECTOR, &ours).await;
                }
                return kind;
            }
        }
    }

    /// Appends a paragraph, the way `y-prosemirror` shapes a ProseMirror document.
    fn write_paragraph(&mut self, text: &str) {
        let root = self.doc.get_or_insert_xml_fragment(ydoc::ROOT);
        let mut txn = self.doc.transact_mut();
        let paragraph = root.push_back(&mut txn, XmlElementPrelim::empty("paragraph"));
        paragraph.push_back(&mut txn, XmlTextPrelim::new(text));
    }

    /// Sends the whole local state. Yjs updates are idempotent, so re-sending what
    /// the peer already has is a no-op — which saves tracking its state vector.
    async fn push_all(&mut self) {
        let update = self.doc.transact().encode_diff_v1(&StateVector::default());
        self.send(MSG_UPDATE, &update).await;
    }

    /// Round-trips a state vector, so everything sent before it is known to have
    /// been processed: frames on one connection are handled in order.
    async fn barrier(&mut self) {
        let ours = self.doc.transact().state_vector().encode_v1();
        self.send(MSG_STATE_VECTOR, &ours).await;
        while self.recv().await != MSG_DIFF {}
    }

    /// Completes the handshake the server opens: its state vector, our diff, its diff.
    async fn handshake(&mut self) {
        assert_eq!(self.recv().await, MSG_STATE_VECTOR, "server opens with its SV");
        assert_eq!(self.recv().await, MSG_DIFF, "server answers with a diff");
    }

    fn text(&self) -> String {
        ydoc::plain_text(&self.doc)
    }

    async fn close(mut self) {
        let _ = self.socket.close(None).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn two_clients_editing_one_note_converge() {
    let server = Server::start().await;
    let id = server.create_note("协同").await;

    let mut a = server.connect(&id).await;
    a.handshake().await;
    a.write_paragraph("来自甲");
    a.push_all().await;
    a.barrier().await;

    // B joins after the fact and is brought up to date by the handshake alone.
    let mut b = server.connect(&id).await;
    b.handshake().await;
    assert_eq!(b.text(), "来自甲", "a late joiner receives the history");

    b.write_paragraph("来自乙");
    b.push_all().await;
    b.barrier().await;

    // A learns about it without asking: the room relays it.
    a.recv().await;

    let (left, right) = (a.text(), b.text());
    assert_eq!(left, right, "both clients converge on the same document");
    assert!(
        left.contains("来自甲") && left.contains("来自乙"),
        "neither edit was dropped: {left:?}"
    );

    a.close().await;
    b.close().await;
}

/// The offline case, which is where last-write-wins used to cost somebody their
/// work: two clients edit the same note with no connection between them, and both
/// edits survive the reunion.
#[tokio::test(flavor = "multi_thread")]
async fn concurrent_offline_edits_both_survive() {
    let server = Server::start().await;
    let id = server.create_note("离线").await;

    // A writes and disconnects without ever telling the server.
    let mut a = server.connect(&id).await;
    a.handshake().await;
    a.write_paragraph("甲离线写的");
    let offline = a.doc.transact().encode_diff_v1(&StateVector::default());
    a.close().await;

    // B writes the same note in the meantime.
    let mut b = server.connect(&id).await;
    b.handshake().await;
    b.write_paragraph("乙同时写的");
    b.push_all().await;
    b.barrier().await;

    // A comes back with its offline work.
    let mut a = server.connect(&id).await;
    a.doc
        .transact_mut()
        .apply_update(Update::decode_v1(&offline).expect("decode"))
        .expect("apply offline work");
    a.handshake().await;
    a.push_all().await;
    a.barrier().await;

    let text = a.text();
    assert!(
        text.contains("甲离线写的") && text.contains("乙同时写的"),
        "both offline edits should be present, got {text:?}"
    );

    b.recv().await;
    assert_eq!(b.text(), text, "b converges on the same result");

    a.close().await;
    b.close().await;
}

/// Titles and previews stay server-derived. The room materializes them when the
/// last connection leaves, so a closed note is immediately correct in the list.
#[tokio::test(flavor = "multi_thread")]
async fn the_server_derives_the_title_from_the_document() {
    let server = Server::start().await;
    let id = server.create_note("").await;

    let mut client = server.connect(&id).await;
    client.handshake().await;
    client.write_paragraph("第一行会变成标题");
    client.write_paragraph("第二行只进预览");
    client.push_all().await;
    client.barrier().await;
    client.close().await;

    // Materialization happens on the way out; give the room a moment to evict.
    let note = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let note = server.note(&id).await;
            if note["title"] != "" {
                return note;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("title should be derived within 5s");

    assert_eq!(note["title"], "第一行会变成标题");
    assert_eq!(note["preview"], "第一行会变成标题 第二行只进预览");
    assert!(
        note["rev"].as_i64().unwrap() > 1,
        "materializing counts as a metadata change"
    );
}
