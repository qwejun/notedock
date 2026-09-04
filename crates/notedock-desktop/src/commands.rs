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
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{AppHandle, Manager, State, WebviewWindow};
use tauri_plugin_autostart::ManagerExt;
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

/// One note on its way to disk. The webview does the Markdown conversion because
/// only it knows what the schema means; this side only decides where files go.
#[derive(Debug, Clone, Deserialize)]
pub struct ExportNote {
    pub title: String,
    pub markdown: String,
}

/// Writes every note into `Documents\NoteDock\<timestamp>\` and opens the folder.
///
/// A folder per run rather than files dumped in one directory: exporting twice
/// otherwise leaves two interleaved generations behind, and there would be no way
/// to tell which `笔记 (2).md` belonged to which export. The webview never learns
/// a path, which is why the capability set still needs no filesystem plugin.
#[tauri::command]
pub fn export_notes(notes: Vec<ExportNote>, app: AppHandle) -> CmdResult<String> {
    if notes.is_empty() {
        return Err("没有可导出的笔记".to_owned());
    }

    let dir = app
        .path()
        .document_dir()
        .map_err(|err| fail("找不到文档目录", err))?
        .join("NoteDock")
        .join(chrono::Local::now().format("%Y-%m-%d %H%M%S").to_string());
    std::fs::create_dir_all(&dir).map_err(|err| fail("无法创建导出目录", err))?;

    for note in &notes {
        let path = unique_path(&dir, &safe_stem(&note.title));
        // CRLF, because the file is written for whatever the user opens it in on
        // Windows rather than for this program.
        let body = note.markdown.replace("\r\n", "\n").replace('\n', "\r\n");
        std::fs::write(&path, body).map_err(|err| fail("无法写入导出文件", err))?;
    }

    #[cfg(target_os = "windows")]
    std::process::Command::new("explorer.exe")
        .arg(&dir)
        .spawn()
        .map_err(|err| fail("无法打开导出目录", err))?;

    Ok(dir.display().to_string())
}

/// A note title is free text; a Windows filename is not.
fn safe_stem(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|ch| match ch {
            '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            ch if ch.is_control() => ' ',
            ch => ch,
        })
        .collect();

    // Trailing dots and spaces are legal in a string and not in a filename.
    let trimmed = cleaned.trim().trim_end_matches('.').trim();
    if trimmed.is_empty() {
        return "未命名".to_owned();
    }

    let mut stem: String = trimmed.chars().take(60).collect();
    stem = stem.trim().to_owned();

    // `CON`, `NUL`, `COM1`… stay device names even with an extension attached,
    // and writing to one fails with an error nobody could act on.
    let upper = stem.to_ascii_uppercase();
    let device = matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((upper.starts_with("COM") || upper.starts_with("LPT"))
            && upper.len() == 4
            && upper.as_bytes()[3].is_ascii_digit());
    if device {
        stem.push('_');
    }

    stem
}

/// Never overwrites: a second export of the same note lands beside the first.
fn unique_path(dir: &Path, stem: &str) -> PathBuf {
    let first = dir.join(format!("{stem}.md"));
    if !first.exists() {
        return first;
    }
    for n in 2..1000 {
        let candidate = dir.join(format!("{stem} ({n}).md"));
        if !candidate.exists() {
            return candidate;
        }
    }
    dir.join(format!(
        "{stem} ({}).md",
        chrono::Local::now().format("%Y%m%d%H%M%S")
    ))
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

/// Whether Windows launches NoteDock at sign-in.
///
/// Read from the registry on every call rather than kept in `settings.json`: 任务
/// 管理器 → 启动 can switch this off without this program ever running, and a
/// switch that disagrees with the system is worse than no switch at all.
#[tauri::command]
pub fn autostart(app: AppHandle) -> CmdResult<bool> {
    app.autolaunch()
        .is_enabled()
        .map_err(|err| fail("无法读取开机自启动设置", err))
}

/// The registry entry records the path of the exe that wrote it, so turning this on
/// registers *this* copy — move or reinstall the program and the switch reads off
/// again, which is the honest answer rather than a stale entry pointing nowhere.
#[tauri::command]
pub fn set_autostart(app: AppHandle, enabled: bool) -> CmdResult<()> {
    let manager = app.autolaunch();
    if enabled {
        manager.enable()
    } else {
        manager.disable()
    }
    .map_err(|err| fail("无法保存开机自启动设置", err))
}

#[tauri::command]
pub fn minimize_window(window: WebviewWindow) -> CmdResult<()> {
    window
        .minimize()
        .map_err(|err| fail("无法最小化窗口", err))
}

/// Hides to the tray. Deliberately not a quit.
///
/// `skipTaskbar` is on, so this window has no taskbar button: a reflexive click on
/// × that ended the process would take the tray icon and the sync loop with it and
/// leave nothing on screen to click. Dismissing is the frequent action and it is
/// reversible — the tray icon, or its 显示 / 隐藏 item, brings the window back.
/// Quitting stays where it is deliberate: 设置 and the tray menu.
#[tauri::command]
pub fn hide_window(window: WebviewWindow) -> CmdResult<()> {
    window.hide().map_err(|err| fail("无法隐藏窗口", err))
}

/// Emitted whenever the window's maximized state changes.
///
/// An event rather than only a return value: `Win`+`↑` and a double-click on the
/// drag region both maximize without going through a command, and a button
/// offering 全屏 on an already-full-screen window is worse than no button.
pub const MAXIMIZED_EVENT: &str = "notedock:maximized";

/// Fills the screen, or goes back to the floating size. Returns where it landed.
///
/// Maximize rather than true fullscreen: this window has no OS chrome and is
/// normally on top, so covering the taskbar as well would take away the way back
/// to everything else.
#[tauri::command]
pub fn toggle_maximize(window: WebviewWindow) -> CmdResult<bool> {
    let maximized = window
        .is_maximized()
        .map_err(|err| fail("无法读取窗口状态", err))?;
    if maximized {
        window
            .unmaximize()
            .map_err(|err| fail("无法还原窗口", err))?;
    } else {
        window.maximize().map_err(|err| fail("无法全屏", err))?;
    }
    Ok(!maximized)
}

/// Asked once on startup. The window is never maximized when it first opens, but
/// the webview can be reloaded while it is.
#[tauri::command]
pub fn is_maximized(window: WebviewWindow) -> CmdResult<bool> {
    window
        .is_maximized()
        .map_err(|err| fail("无法读取窗口状态", err))
}

/// Names the session in the server's session list. Best-effort: a machine
/// without a hostname just shows up as a desktop.
fn hostname_label() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .map(|name| format!("desktop:{name}"))
        .unwrap_or_else(|_| "desktop".to_owned())
}

#[cfg(test)]
mod tests {
    use super::{safe_stem, unique_path};

    #[test]
    fn stem_keeps_cjk_and_replaces_illegal_characters() {
        assert_eq!(safe_stem("blender学习"), "blender学习");
        assert_eq!(safe_stem("2026/09/04 会议"), "2026-09-04 会议");
        assert_eq!(safe_stem("a:b*c?d\"e<f>g|h"), "a-b-c-d-e-f-g-h");
    }

    #[test]
    fn stem_falls_back_when_there_is_nothing_usable() {
        assert_eq!(safe_stem(""), "未命名");
        assert_eq!(safe_stem("   "), "未命名");
        assert_eq!(safe_stem("..."), "未命名");
    }

    /// A trailing dot is legal in the title and not in the filename.
    #[test]
    fn stem_trims_trailing_dots_and_spaces() {
        assert_eq!(safe_stem("  草稿.  "), "草稿");
    }

    #[test]
    fn stem_escapes_windows_device_names() {
        assert_eq!(safe_stem("CON"), "CON_");
        assert_eq!(safe_stem("com1"), "com1_");
        // Not a device: only COM1..COM9 are, so a longer name is left alone.
        assert_eq!(safe_stem("COM10"), "COM10");
        assert_eq!(safe_stem("CONTEXT"), "CONTEXT");
    }

    #[test]
    fn stem_is_length_capped() {
        assert_eq!(safe_stem(&"字".repeat(200)).chars().count(), 60);
    }

    #[test]
    fn exporting_twice_does_not_overwrite() {
        let dir = tempfile::tempdir().expect("temp dir");
        let first = unique_path(dir.path(), "笔记");
        assert_eq!(first.file_name().unwrap(), "笔记.md");

        std::fs::write(&first, "one").expect("write");
        let second = unique_path(dir.path(), "笔记");
        assert_eq!(second.file_name().unwrap(), "笔记 (2).md");

        std::fs::write(&second, "two").expect("write");
        assert_eq!(
            unique_path(dir.path(), "笔记").file_name().unwrap(),
            "笔记 (3).md"
        );
        // The first export is still there, untouched.
        assert_eq!(std::fs::read_to_string(&first).expect("read"), "one");
    }
}
