// Windows: no console window behind the floating notepad in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    notedock_desktop_lib::run()
}
