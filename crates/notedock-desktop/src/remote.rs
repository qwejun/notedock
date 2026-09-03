//! HTTP client for the NAS server.
//!
//! Metadata only: listing, creating and deleting notes, and the ticket the webview
//! needs to open a document socket. Note *bodies* never pass through here — they
//! travel as Yjs updates over that socket.
//!
//! Lives in Rust rather than the webview so the bearer token never enters
//! JavaScript, and so retrying a failed request is the sync loop's business
//! instead of the UI's.

use notedock_api::{
    ApiErrorBody, CreateNoteRequest, ErrorCode, LoginRequest, LoginResponse, NoteSummary,
    SyncResponse, TicketResponse, API_PREFIX,
};
use serde::Serialize;
use std::time::Duration;

/// Long enough for a slow home uplink, short enough that a black-holed
/// connection does not wedge the sync loop.
const TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, thiserror::Error)]
pub enum RemoteError {
    /// Never reached the server. The caller keeps the local change and retries.
    #[error("无法连接到服务器")]
    Offline(#[source] reqwest::Error),

    #[error("登录已过期")]
    Unauthorized,

    #[error("服务器返回错误：{message}")]
    Api { code: ErrorCode, message: String },

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type RemoteResult<T> = Result<T, RemoteError>;

pub struct Remote {
    http: reqwest::Client,
    base: String,
    token: Option<String>,
}

impl Remote {
    pub fn new(base: String, token: Option<String>) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder().timeout(TIMEOUT).build()?;
        Ok(Self { http, base, token })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}{}", self.base, API_PREFIX, path)
    }

    pub async fn login(&self, password: &str, label: &str) -> RemoteResult<LoginResponse> {
        self.send(
            reqwest::Method::POST,
            "/auth/login",
            Some(&LoginRequest {
                password: password.to_owned(),
                label: Some(label.to_owned()),
            }),
        )
        .await
    }

    pub async fn create(&self, req: &CreateNoteRequest) -> RemoteResult<NoteSummary> {
        self.send(reqwest::Method::POST, "/notes", Some(req)).await
    }

    pub async fn list(&self) -> RemoteResult<Vec<NoteSummary>> {
        self.send::<(), _>(reqwest::Method::GET, "/notes", None).await
    }

    pub async fn delete(&self, id: &str) -> RemoteResult<()> {
        self.send_unit(reqwest::Method::DELETE, &format!("/notes/{id}"))
            .await
    }

    pub async fn sync(&self, since: i64) -> RemoteResult<SyncResponse> {
        self.send::<(), _>(reqwest::Method::GET, &format!("/sync?since={since}"), None)
            .await
    }

    /// Trades the bearer token for a single-use WebSocket URL, which is the only
    /// credential that reaches the webview.
    pub async fn ws_ticket(&self) -> RemoteResult<TicketResponse> {
        self.send::<(), _>(reqwest::Method::POST, "/ws-ticket", None)
            .await
    }

    async fn request(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<&impl Serialize>,
    ) -> RemoteResult<reqwest::Response> {
        let mut req = self.http.request(method, self.url(path));
        if let Some(token) = &self.token {
            req = req.bearer_auth(token);
        }
        if let Some(body) = body {
            req = req.json(body);
        }
        req.send().await.map_err(RemoteError::Offline)
    }

    async fn send<B: Serialize, T: serde::de::DeserializeOwned>(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<&B>,
    ) -> RemoteResult<T> {
        let res = self.request(method, path, body).await?;
        let res = self.check(res).await?;
        res.json::<T>()
            .await
            .map_err(|err| RemoteError::Other(err.into()))
    }

    async fn send_unit(&self, method: reqwest::Method, path: &str) -> RemoteResult<()> {
        let res = self.request(method, path, None::<&()>).await?;
        self.check(res).await?;
        Ok(())
    }

    /// Turns a non-2xx response into the matching [`RemoteError`]. A body that is
    /// not the expected error shape (a proxy's HTML 502, say) still produces a
    /// usable message rather than a parse failure.
    async fn check(&self, res: reqwest::Response) -> RemoteResult<reqwest::Response> {
        if res.status().is_success() {
            return Ok(res);
        }

        let status = res.status();
        let body: Option<ApiErrorBody> = res.json().await.ok();

        match body {
            Some(body) => match body.code {
                ErrorCode::Unauthorized => Err(RemoteError::Unauthorized),
                code => Err(RemoteError::Api {
                    code,
                    message: body.message,
                }),
            },
            None => Err(RemoteError::Api {
                code: ErrorCode::Internal,
                message: format!("服务器返回 {status}"),
            }),
        }
    }
}
