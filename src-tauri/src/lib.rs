//! Scriptorium Tauri library entry.
//!
//! Hosts the desktop app runtime, the shared Rust modules (book format types, ingest, the
//! book library), and the Tauri commands the webview calls.

#![warn(missing_docs)]

pub mod book_format;
mod commands;
pub mod ingest;
pub mod library;
// `dev` is Tauri's cfg for `tauri dev`, including `tauri dev --release`, where the spike UI runs.
#[cfg(dev)]
mod spike;

use tauri::{Manager, RunEvent, WindowEvent};

/// Starts the Tauri app and returns when the app exits.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(commands::setup)
        // Flush in Rust: a JavaScript close handler would race the window teardown.
        .on_window_event(|window, event| {
            if matches!(event, WindowEvent::CloseRequested { .. }) {
                commands::flush_all(window.app_handle());
            }
        });
    // Exactly one `generate_handler!` per cfg: a second `invoke_handler` call would replace
    // the first.
    #[cfg(dev)]
    let builder = builder.invoke_handler(tauri::generate_handler![
        commands::inspect_folder,
        commands::create_book,
        commands::open_book,
        commands::save_progress,
        commands::last_book,
        spike::spike_pages,
    ]);
    #[cfg(not(dev))]
    let builder = builder.invoke_handler(tauri::generate_handler![
        commands::inspect_folder,
        commands::create_book,
        commands::open_book,
        commands::save_progress,
        commands::last_book,
    ]);
    builder
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if matches!(event, RunEvent::ExitRequested { .. } | RunEvent::Exit) {
                commands::flush_all(app);
            }
        });
}
