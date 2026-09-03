//! Tauri wiring: tray, window, state, and the background sync loop.

pub mod commands;
pub mod remote;
pub mod settings;
pub mod store;
pub mod sync;

use std::sync::Arc;
use tauri::{
    image::Image,
    menu::{Menu, MenuItem},
    tray::{TrayIconBuilder, TrayIconEvent},
    Manager,
};
use tracing_subscriber::EnvFilter;

fn toggle_window(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    // `unwrap_or(true)` errs towards hiding: if the state cannot be read, the
    // click should still do something rather than nothing.
    if window.is_visible().unwrap_or(true) {
        let _ = window.hide();
    } else {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_env("NOTEDOCK_LOG")
                .unwrap_or_else(|_| EnvFilter::new("notedock_desktop_lib=info")),
        )
        .init();

    tauri::Builder::default()
        .setup(|app| {
            let handle = app.handle().clone();
            let app_data = handle
                .path()
                .app_data_dir()
                .map_err(|err| format!("app data directory: {err}"))?;
            std::fs::create_dir_all(&app_data)
                .map_err(|err| format!("create app data directory: {err}"))?;

            let loaded = settings::load(&app_data);
            let db_path = app_data.join(store::DB_FILE);
            let start_on_top = loaded.always_on_top;

            // The store has to exist before the first command can run, so this
            // one blocking wait at startup is deliberate.
            let store = tauri::async_runtime::block_on(store::Store::open(&db_path))
                .map_err(|err| format!("open local database: {err}"))?;
            let engine = Arc::new(sync::Engine::new(store, loaded, app_data));

            app.manage(Arc::clone(&engine));
            sync::spawn(handle.clone(), engine);

            // `tauri.conf.json` starts the window on top, which is the right
            // default; honour a saved preference to the contrary.
            if !start_on_top {
                if let Some(window) = handle.get_webview_window("main") {
                    let _ = window.set_always_on_top(false);
                }
            }

            if std::env::var_os("NOTEDOCK_NO_TRAY").is_none() {
                let show = MenuItem::with_id(app, "toggle", "显示 / 隐藏", true, None::<&str>)
                    .map_err(|err| format!("create tray show item: {err}"))?;
                let quit = MenuItem::with_id(app, "quit", "退出", true, Some("CmdOrCtrl+Q"))
                    .map_err(|err| format!("create tray quit item: {err}"))?;
                let menu = Menu::with_items(app, &[&show, &quit])
                    .map_err(|err| format!("create tray menu: {err}"))?;

                let icon = Image::from_bytes(include_bytes!("../icons/icon.png"))
                    .map_err(|err| format!("load tray icon: {err}"))?;

                TrayIconBuilder::new()
                    .icon(icon)
                    .menu(&menu)
                    .tooltip("NoteDock")
                    .on_menu_event(|app, event| match event.id.as_ref() {
                        "toggle" => toggle_window(app),
                        "quit" => app.exit(0),
                        _ => {}
                    })
                    .on_tray_icon_event(|tray, event| {
                        if let TrayIconEvent::Click { .. } = event {
                            toggle_window(tray.app_handle());
                        }
                    })
                    .build(app)
                    .map_err(|err| format!("build system tray: {err}"))?;
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_notes,
            commands::get_note,
            commands::create_note,
            commands::delete_note,
            commands::ws_url,
            commands::sync_now,
            commands::sync_state,
            commands::get_spotlight,
            commands::set_spotlight,
            commands::login,
            commands::logout,
            commands::open_web,
            commands::set_click_through,
            commands::set_always_on_top,
            commands::window_prefs,
            commands::set_opacity,
            commands::app_info,
            commands::quit,
            commands::minimize_window,
            commands::close_window,
        ])
        .run(tauri::generate_context!())
        .expect("error while running NoteDock");
}
