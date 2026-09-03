//! The surface the webview is allowed to touch.
//!
//! Credentials and every HTTP request stay on this side. The webview gets the
//! note list, a sync state, and a short-lived WebSocket URL — never the bearer
//! token.
//!
//! Note bodies are conspicuously absent: there is no `save_note`. Each open note
//! is a Yjs document in the webview that converges over its own socket, so
//! "saving" is not an operation any client performs.

use crate::{
    remote::Remote,
    settings::normalize_url,
    store::{LocalNote, DB_FILE},
    sync::{Engine, SyncState},
};
use notedock_api::NoteSummary;
use serde::Serialize;
use std::sync::Arc;
use tauri::{AppHandle, State, WebviewWindow};
use ts_rs::TS;

/// Commands report failures as plain strings: the webview only displays them,
/// and structured error handling belongs to the sync engine, not the UI.
type CmdResult<T> = Result<T, String>;

fn fail(context: &str, err: impl std::fmt::Display) -> String {
    tracing::error!(%err, "{context}");
    format!("{context}：{err}")
}

#[tauri::command]
pub async fn list_notes(engine: State<'_, Arc<Engine>>) -> CmdResult<Vec<NoteSummary>> {
    engine
        .store
        .list()
        .await
        .map_err(|err| fail("无法读取笔记列表", err))
}

#[tauri::command]
pub async fn get_note(id: String, engine: State<'_, Arc<Engine>>) -> CmdResult<Option<LocalNote>> {
    engine
        .store
        .get(&id)
        .await
        .map_err(|err| fail("无法打开笔记", err))
}

/// Creates the note locally and returns immediately, so it can be typed into with
/// the network down. The upload is the sync loop's problem; the body the user
/// types goes straight into the note's Yjs document.
#[tauri::command]
pub async fn create_note(
    title: String,
    app: AppHandle,
    engine: State<'_, Arc<Engine>>,
) -> CmdResult<LocalNote> {
    let note = engine
        .store
        .create(&title)
        .await
        .map_err(|err| fail("无法新建笔记", err))?;
    engine.notify(&app).await;
    Ok(note)
}

#[tauri::command]
pub async fn delete_note(
    id: String,
    app: AppHandle,
    engine: State<'_, Arc<Engine>>,
) -> CmdResult<()> {
    engine
        .store
        .delete_local(&id)
        .await
        .map_err(|err| fail("无法删除笔记", err))?;
    engine.notify(&app).await;
    Ok(())
}

/// Hands the webview a single-use URL for a note's document socket.
///
/// This is the one place a credential crosses into JavaScript, and what crosses is
/// a ticket that expires in 30 seconds — never the month-long bearer token. Called
/// again for every reconnect, because a ticket is spent on use.
#[tauri::command]
pub async fn ws_url(engine: State<'_, Arc<Engine>>) -> CmdResult<String> {
    let remote = engine
        .remote()
        .await
        .ok_or_else(|| "尚未登录".to_owned())?;
    let ticket = remote
        .ws_ticket()
        .await
        .map_err(|err| fail("无法获取连接凭证", err))?;
    Ok(ticket.url)
}

#[tauri::command]
pub async fn sync_now(app: AppHandle, engine: State<'_, Arc<Engine>>) -> CmdResult<SyncState> {
    engine.run_once(&app).await;
    Ok(engine.snapshot().await)
}

#[tauri::command]
pub async fn sync_state(engine: State<'_, Arc<Engine>>) -> CmdResult<SyncState> {
    Ok(engine.snapshot().await)
}

#[tauri::command]
pub async fn get_spotlight(engine: State<'_, Arc<Engine>>) -> CmdResult<Option<String>> {
    Ok(engine.spotlight_note_id().await)
}

#[tauri::command]
pub async fn set_spotlight(
    id: Option<String>,
    app: AppHandle,
    engine: State<'_, Arc<Engine>>,
) -> CmdResult<()> {
    engine
        .set_spotlight_note(&app, id)
        .await
        .map_err(|err| fail("无法保存桌面置顶设置", err))
}

#[tauri::command]
pub async fn login(
    server_url: String,
    password: String,
    app: AppHandle,
    engine: State<'_, Arc<Engine>>,
) -> CmdResult<SyncState> {
    let url = normalize_url(&server_url);
    if url.is_empty() {
        return Err("请填写服务器地址".to_owned());
    }

    let remote = Remote::new(url.clone(), None).map_err(|err| fail("无法创建连接", err))?;
    let label = hostname_label();
    let session = remote
        .login(&password, &label)
        .await
        .map_err(|err| err.to_string())?;

    engine
        .set_credentials(&app, url, session.token)
        .await
        .map_err(|err| fail("无法保存登录信息", err))?;

    engine.run_once(&app).await;
    Ok(engine.snapshot().await)
}

#[tauri::command]
pub async fn logout(app: AppHandle, engine: State<'_, Arc<Engine>>) -> CmdResult<SyncState> {
    engine
        .forget_credentials(&app)
        .await
        .map_err(|err| fail("无法退出登录", err))?;
    Ok(engine.snapshot().await)
}

#[tauri::command]
pub async fn open_web(engine: State<'_, Arc<Engine>>) -> CmdResult<()> {
    let url = engine.server_url().await;
    if url.is_empty() {
        return Err("尚未设置服务器地址".to_owned());
    }

    #[cfg(target_os = "windows")]
    std::process::Command::new("explorer.exe")
        .arg(&url)
        .spawn()
        .map_err(|err| fail("无法打开 Web 端", err))?;

    Ok(())
}

/// Clicks pass straight through the window. `Ctrl+Shift+K` in the UI is the way
/// back out — without it the window would be impossible to switch off again.
#[tauri::command]
pub fn set_click_through(window: WebviewWindow, active: bool) -> CmdResult<()> {
    window
        .set_ignore_cursor_events(active)
        .map_err(|err| fail("无法切换点击穿透", err))
}

/// Window preferences that outlive the session.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "desktop.ts")]
pub struct WindowPrefs {
    pub opacity: f64,
    pub always_on_top: bool,
}

#[tauri::command]
pub async fn window_prefs(engine: State<'_, Arc<Engine>>) -> CmdResult<WindowPrefs> {
    let (opacity, always_on_top) = engine.window_prefs().await;
    Ok(WindowPrefs {
        opacity,
        always_on_top,
    })
}

/// Called when the slider is released, not on every tick: the value is persisted
/// to disk, and live preview is the webview's own business.
#[tauri::command]
pub async fn set_opacity(value: f64, engine: State<'_, Arc<Engine>>) -> CmdResult<()> {
    engine
        .set_opacity(value)
        .await
        .map_err(|err| fail("无法保存不透明度", err))
}

#[tauri::command]
pub async fn set_always_on_top(
    window: WebviewWindow,
    on_top: bool,
    engine: State<'_, Arc<Engine>>,
) -> CmdResult<()> {
    window
        .set_always_on_top(on_top)
        .map_err(|err| fail("无法切换置顶", err))?;
    engine
        .set_always_on_top(on_top)
        .await
        .map_err(|err| fail("无法保存置顶设置", err))
}

/// What the settings panel shows under "关于". The data directory is worth
/// surfacing: it is where the offline cache and the bearer token live.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "desktop.ts")]
pub struct AppInfo {
    pub version: String,
    pub data_dir: String,
    pub db_path: String,
}

#[tauri::command]
pub fn app_info(app: AppHandle, engine: State<'_, Arc<Engine>>) -> CmdResult<AppInfo> {
    let data_dir = engine.app_data().to_path_buf();
    Ok(AppInfo {
        version: app.package_info().version.to_string(),
        db_path: data_dir.join(DB_FILE).display().to_string(),
        data_dir: data_dir.display().to_string(),
    })
}

#[tauri::command]
pub fn quit(app: AppHandle) {
    app.exit(0);
}

#[tauri::command]
pub fn minimize_window(window: WebviewWindow) -> CmdResult<()> {
    window
        .minimize()
        .map_err(|err| fail("无法最小化窗口", err))
}

#[tauri::command]
pub fn close_window(window: WebviewWindow) -> CmdResult<()> {
    window.close().map_err(|err| fail("无法关闭窗口", err))
}

/// Names the session in the server's session list. Best-effort: a machine
/// without a hostname just shows up as a desktop.
fn hostname_label() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .map(|name| format!("desktop:{name}"))
        .unwrap_or_else(|_| "desktop".to_owned())
}
