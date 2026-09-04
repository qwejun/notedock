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
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager,
};
use tracing_subscriber::EnvFilter;

/// What a tray click and the 显示 / 隐藏 item both do.
fn toggle_window(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    // Minimized counts as away rather than as shown: `−` leaves the window visible as
    // far as Windows is concerned, so without this branch the first tray click would
    // hide an already-invisible window and it would take two to get the note back.
    if window.is_minimized().unwrap_or(false) {
        let _ = window.unminimize();
        let _ = window.set_focus();
        return;
    }
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
        // No launch arguments: the window is meant to be on screen after a sign-in,
        // which is the whole point of a notepad that floats. `MacosLauncher` is
        // ignored on Windows and is only here because the signature wants it.
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
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
                    // Left click is the toggle; the menu is right click's, as it is for
                    // every other tray-resident program on Windows. Left-click-opens-menu
                    // is Tauri's default and it fights this window: the menu would appear
                    // over the note the same click just summoned.
                    .show_menu_on_left_click(false)
                    .tooltip("NoteDock")
                    .on_menu_event(|app, event| match event.id.as_ref() {
                        "toggle" => toggle_window(app),
                        "quit" => app.exit(0),
                        _ => {}
                    })
                    .on_tray_icon_event(|tray, event| {
                        // The release of the left button, specifically. `Click` fires twice
                        // for one physical click — once pressed, once released — so
                        // matching the variant alone toggled the window and toggled it
                        // back: it flashed and was gone before the finger came off the
                        // mouse. Right click is left to the menu.
                        if let TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        } = event
                        {
                            toggle_window(tray.app_handle());
                        }
                    })
                    .build(app)
                    .map_err(|err| format!("build system tray: {err}"))?;
            }

            Ok(())
        })
        // Reported from here rather than from the command, because maximizing does
        // not always go through one: `Win`+`↑`, a drag to the top of the screen and
        // a double-click on the title bar all arrive as a plain resize.
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::Resized(_)) {
                if let Ok(maximized) = window.is_maximized() {
                    let _ = window.emit(commands::MAXIMIZED_EVENT, maximized);
                }
            }
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
            commands::export_notes,
            commands::set_click_through,
            commands::set_always_on_top,
            commands::window_prefs,
            commands::set_opacity,
            commands::app_info,
            commands::autostart,
            commands::set_autostart,
            commands::quit,
            commands::minimize_window,
            commands::hide_window,
            commands::toggle_maximize,
            commands::is_maximized,
        ])
        .run(tauri::generate_context!())
        .expect("error while running NoteDock");
}
