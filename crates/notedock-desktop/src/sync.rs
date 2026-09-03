//! The metadata sync loop: push local creations and deletions, then pull the
//! note list.
//!
//! Note *bodies* are not synced here. Each open note keeps a Yjs document that
//! converges over its own WebSocket, in the webview — which is why this file has
//! no conflict handling left: there is no body write that can lose a race.
//!
//! Push still comes first. A note created offline has to exist on the server
//! before the pull can tell us anything useful about it.

use crate::{
    remote::{Remote, RemoteError},
    settings::Settings,
    store::Store,
};
use notedock_api::CreateNoteRequest;
use serde::Serialize;
use std::{path::PathBuf, sync::Arc, time::Duration};
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;
use ts_rs::TS;

/// How often the note list is refreshed. Body edits do not wait for this — they
/// travel over the document socket the moment they happen.
const INTERVAL: Duration = Duration::from_secs(5);

/// Bound on pages pulled per run, so a first sync of a large library converges
/// quickly without blocking the loop indefinitely.
const MAX_PAGES: usize = 20;

/// Event the webview listens on for every state change.
pub const SYNC_EVENT: &str = "notedock:sync";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "desktop.ts")]
pub enum SyncStatus {
    Synced,
    Syncing,
    Offline,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "desktop.ts")]
pub struct SyncState {
    pub status: SyncStatus,
    /// Human-readable detail, shown only when there is something to say.
    pub message: Option<String>,
    /// Local creations and deletions not yet accepted by the server.
    #[ts(type = "number")]
    pub pending: i64,
    pub logged_in: bool,
    pub server_url: String,
}

pub struct Engine {
    pub store: Store,
    settings: Mutex<Settings>,
    app_data: PathBuf,
    state: Mutex<SyncState>,
    /// True until the first background pass has completed. Startup failures
    /// should return to the login screen; later network drops should keep the
    /// editor open and retry in the background.
    startup: Mutex<bool>,
    /// Held for the duration of a run so the timer and a manual `sync_now`
    /// cannot interleave two passes over the same outbox.
    run_lock: Mutex<()>,
}

impl Engine {
    pub fn new(store: Store, settings: Settings, app_data: PathBuf) -> Self {
        let state = SyncState {
            status: if settings.is_configured() {
                SyncStatus::Syncing
            } else {
                SyncStatus::Offline
            },
            message: None,
            pending: 0,
            logged_in: settings.is_configured(),
            server_url: settings.server_url.clone(),
        };

        Self {
            store,
            settings: Mutex::new(settings),
            app_data,
            state: Mutex::new(state),
            startup: Mutex::new(true),
            run_lock: Mutex::new(()),
        }
    }

    pub async fn snapshot(&self) -> SyncState {
        self.state.lock().await.clone()
    }

    pub async fn server_url(&self) -> String {
        self.settings.lock().await.server_url.clone()
    }

    /// An HTTP client bound to the current credentials, or `None` when there are
    /// none. Public because `ws_url` needs one to mint a ticket.
    pub async fn remote(&self) -> Option<Remote> {
        let settings = self.settings.lock().await;
        if !settings.is_configured() {
            return None;
        }
        Remote::new(settings.server_url.clone(), settings.token.clone())
            .inspect_err(|err| tracing::error!(%err, "failed to build HTTP client"))
            .ok()
    }

    /// Replaces the stored credentials. The local cache is dropped when the
    /// server changes: ids and change-log positions from another server are
    /// meaningless here, and keeping them would corrupt the next sync.
    pub async fn set_credentials(
        &self,
        app: &AppHandle,
        server_url: String,
        token: String,
    ) -> anyhow::Result<()> {
        let mut settings = self.settings.lock().await;
        let switching = !settings.server_url.is_empty() && settings.server_url != server_url;
        settings.server_url = server_url;
        settings.token = Some(token);
        crate::settings::save(&self.app_data, &settings)?;
        let url = settings.server_url.clone();
        drop(settings);

        if switching {
            self.store.clear().await?;
        }
        self.update(app, |state| {
            state.logged_in = true;
            state.server_url = url;
            state.status = SyncStatus::Syncing;
            state.message = None;
        })
        .await;
        Ok(())
    }

    pub async fn forget_credentials(&self, app: &AppHandle) -> anyhow::Result<()> {
        let mut settings = self.settings.lock().await;
        settings.token = None;
        crate::settings::save(&self.app_data, &settings)?;
        drop(settings);

        self.update(app, |state| {
            state.logged_in = false;
            state.status = SyncStatus::Offline;
            state.message = None;
        })
        .await;
        Ok(())
    }

    pub async fn spotlight_note_id(&self) -> Option<String> {
        self.settings.lock().await.spotlight_note_id.clone()
    }

    /// Window opacity and always-on-top, as persisted. Read at startup so the
    /// window comes back the way it was left.
    pub async fn window_prefs(&self) -> (f64, bool) {
        let settings = self.settings.lock().await;
        (settings.opacity, settings.always_on_top)
    }

    /// Clamped here rather than trusting the slider: the webview is the one part
    /// of this app that could be fed a bogus value.
    pub async fn set_opacity(&self, opacity: f64) -> anyhow::Result<()> {
        let clamped = opacity.clamp(crate::settings::MIN_OPACITY, 1.0);
        let mut settings = self.settings.lock().await;
        settings.opacity = clamped;
        crate::settings::save(&self.app_data, &settings)
    }

    pub async fn set_always_on_top(&self, on_top: bool) -> anyhow::Result<()> {
        let mut settings = self.settings.lock().await;
        settings.always_on_top = on_top;
        crate::settings::save(&self.app_data, &settings)
    }

    /// Where the local cache lives, shown in the settings panel so the notes are
    /// findable without hunting through AppData.
    pub fn app_data(&self) -> &std::path::Path {
        &self.app_data
    }

    pub async fn set_spotlight_note(
        &self,
        app: &AppHandle,
        note_id: Option<String>,
    ) -> anyhow::Result<()> {
        let mut settings = self.settings.lock().await;
        settings.spotlight_note_id = note_id;
        crate::settings::save(&self.app_data, &settings)?;
        drop(settings);
        self.notify(app).await;
        Ok(())
    }

    async fn update(&self, app: &AppHandle, mutate: impl FnOnce(&mut SyncState)) {
        let snapshot = {
            let mut state = self.state.lock().await;
            mutate(&mut state);
            state.pending = self.store.pending_count().await.unwrap_or(state.pending);
            state.clone()
        };
        if let Err(err) = app.emit(SYNC_EVENT, &snapshot) {
            tracing::warn!(%err, "failed to emit sync state");
        }
    }

    /// Re-emits the current state, e.g. after a local edit changed the outbox.
    pub async fn notify(&self, app: &AppHandle) {
        self.update(app, |_| {}).await;
    }

    /// One full pass. Never returns an error: a failed sync is a state to show,
    /// not something the caller can fix.
    pub async fn run_once(&self, app: &AppHandle) {
        let _guard = self.run_lock.lock().await;
        let initial_pass = {
            let mut startup = self.startup.lock().await;
            let initial = *startup;
            *startup = false;
            initial
        };

        let Some(remote) = self.remote().await else {
            if initial_pass {
                let _ = self.forget_credentials(app).await;
            } else {
                self.update(app, |state| state.status = SyncStatus::Offline)
                    .await;
            }
            return;
        };

        self.update(app, |state| state.status = SyncStatus::Syncing)
            .await;

        match self.sync_pass(&remote).await {
            Ok(()) => {
                if initial_pass {
                    match remote.list().await {
                        Ok(notes) => {
                            if let Err(err) = self.store.reconcile(&notes).await {
                                self.settle(app, Failure::from_local(err), initial_pass).await;
                                return;
                            }
                        }
                        Err(err) => {
                            self.settle(app, Failure::from_remote(err), initial_pass).await;
                            return;
                        }
                    }
                }
                self.update(app, |state| {
                    state.status = SyncStatus::Synced;
                    state.message = None;
                })
                .await;
            }
            Err(outcome) => self.settle(app, outcome, initial_pass).await,
        }
    }

    /// Push then pull.
    ///
    /// Push comes first: a note created offline has to exist on the server before
    /// the pull can say anything useful about it.
    ///
    /// Separate from [`Self::run_once`] so the sync logic can be exercised
    /// against a real server without a Tauri app handle.
    pub async fn sync_pass(&self, remote: &Remote) -> Result<(), Failure> {
        self.push(remote).await?;
        self.pull(remote).await
    }

    /// How a failed pass is reported. An expired token is worth surfacing
    /// loudly; a dropped connection is just "offline" and will retry.
    async fn settle(&self, app: &AppHandle, outcome: Failure, initial_pass: bool) {
        match outcome {
            Failure::Offline => {
                if initial_pass {
                    let _ = self.forget_credentials(app).await;
                    self.update(app, |state| {
                        state.message = Some("无法连接服务器，请重新登录".to_owned());
                    })
                    .await;
                } else {
                    self.update(app, |state| {
                        state.status = SyncStatus::Offline;
                        state.message = None;
                    })
                    .await;
                }
            }
            Failure::Unauthorized => {
                let _ = self.forget_credentials(app).await;
                self.update(app, |state| {
                    state.status = SyncStatus::Offline;
                    state.message = Some("登录已过期，请重新登录".to_owned());
                })
                .await;
            }
            Failure::Message(message) => {
                if initial_pass {
                    let _ = self.forget_credentials(app).await;
                    self.update(app, |state| state.message = Some(message)).await;
                } else {
                    self.update(app, |state| {
                        state.status = SyncStatus::Offline;
                        state.message = Some(message);
                    })
                    .await;
                }
            }
        }
    }

    /// Uploads local creations and deletions. Bodies are not touched: whatever
    /// was typed into a note lives in its Yjs document and reaches the server
    /// through the document socket instead.
    async fn push(&self, remote: &Remote) -> Result<(), Failure> {
        for note in self.store.pending().await.map_err(Failure::from_local)? {
            if note.deleted {
                // Never uploaded and already deleted: there is nothing on the
                // server to tombstone.
                if note.rev > 0 {
                    match remote.delete(&note.id).await {
                        // A 404 is the desired end state too, so any API-level
                        // rejection is treated as "it is not there".
                        Ok(()) | Err(RemoteError::Api { .. }) => {}
                        Err(RemoteError::Offline(_)) => return Err(Failure::Offline),
                        Err(RemoteError::Unauthorized) => return Err(Failure::Unauthorized),
                        Err(err) => return Err(Failure::Message(err.to_string())),
                    }
                }
                self.store
                    .mark_pushed_missing(&note.id)
                    .await
                    .map_err(Failure::from_local)?;
                continue;
            }

            // Created offline. The id travels with it, so a retry after a dropped
            // connection cannot produce a duplicate.
            let created = remote
                .create(&CreateNoteRequest {
                    id: Some(note.id.clone()),
                    title: note.title.clone(),
                })
                .await;

            match created {
                Ok(saved) => self
                    .store
                    .mark_pushed(&saved)
                    .await
                    .map_err(Failure::from_local)?,
                Err(RemoteError::Offline(_)) => return Err(Failure::Offline),
                Err(RemoteError::Unauthorized) => return Err(Failure::Unauthorized),
                Err(err) => return Err(Failure::Message(err.to_string())),
            }
        }
        Ok(())
    }

    async fn pull(&self, remote: &Remote) -> Result<(), Failure> {
        let mut cursor = self.store.cursor().await.map_err(Failure::from_local)?;

        for _ in 0..MAX_PAGES {
            let page = match remote.sync(cursor).await {
                Ok(page) => page,
                Err(RemoteError::Offline(_)) => return Err(Failure::Offline),
                Err(RemoteError::Unauthorized) => return Err(Failure::Unauthorized),
                Err(err) => return Err(Failure::Message(err.to_string())),
            };

            let empty = page.changes.is_empty();
            // One request per page now, not one per note: `/sync` already carries
            // everything the list needs, because bodies are somewhere else.
            for note in &page.changes {
                self.store
                    .apply_remote(note)
                    .await
                    .map_err(Failure::from_local)?;
            }

            cursor = page.cursor;
            self.store
                .set_cursor(cursor)
                .await
                .map_err(Failure::from_local)?;

            if empty {
                break;
            }
        }
        Ok(())
    }
}

/// Why a pass stopped. Separate from [`RemoteError`] because the local database
/// can fail too, and both collapse to the same three outcomes for the UI.
#[derive(Debug)]
pub enum Failure {
    Offline,
    Unauthorized,
    Message(String),
}

impl Failure {
    fn from_local(err: anyhow::Error) -> Self {
        tracing::error!(error = ?err, "local database error during sync");
        Self::Message("本地数据库出错".to_owned())
    }

    fn from_remote(err: RemoteError) -> Self {
        match err {
            RemoteError::Offline(_) => Self::Offline,
            RemoteError::Unauthorized => Self::Unauthorized,
            other => Self::Message(other.to_string()),
        }
    }
}

/// Starts the background loop. Runs immediately, then on [`INTERVAL`].
pub fn spawn(app: AppHandle, engine: Arc<Engine>) {
    tauri::async_runtime::spawn(async move {
        let mut ticker = tokio::time::interval(INTERVAL);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;
            engine.run_once(&app).await;
        }
    });
}
