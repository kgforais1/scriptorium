//! Scriptorium Tauri library entry.
//!
//! Hosts the desktop app runtime, the shared Rust modules (book format types, ingest, the
//! book library), and the Tauri commands the webview calls.

#![warn(missing_docs)]

pub mod book_format;
mod commands;
pub mod ingest;
pub mod library;

use tauri::{Manager, RunEvent, WindowEvent};

/// Loads the pdfium library from the bundle (`Contents/Frameworks` on macOS, the resource
/// directory elsewhere). A failure is logged and
/// nothing more: only PDF import reports it, so images and existing books keep working.
///
/// A debug build falls back to the directory `scripts/fetch_pdfium.sh` fills, for `tauri dev`.
fn bind_pdfium(app: &tauri::App) {
    let mut dirs = Vec::new();
    if let Ok(resources) = app.path().resource_dir() {
        // macOS ships it in Contents/Frameworks, so the bundler signs it with the app's
        // identity. A dylib signed by anyone else fails library validation.
        if cfg!(target_os = "macos") {
            if let Some(contents) = resources.parent() {
                dirs.push(contents.join("Frameworks"));
            }
        }
        dirs.push(resources);
    }
    if cfg!(debug_assertions) {
        dirs.push(std::path::PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/pdfium"
        )));
    }
    // Binding is once per process, so pick the first directory that holds the library.
    let Some(dir) = dirs
        .iter()
        .find(|d| pdfium_render::prelude::Pdfium::pdfium_platform_library_name_at_path(d).is_file())
        .or(dirs.first())
    else {
        eprintln!("PDF import is unavailable: the app has no resource directory");
        return;
    };
    if let Err(e) = ingest::pdf::bind(dir) {
        eprintln!("PDF import is unavailable: {e}");
    }
}

/// Starts the Tauri app and returns when the app exits.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            bind_pdfium(app);
            commands::setup(app)
        })
        // Flush in Rust: a JavaScript close handler would race the window teardown.
        .on_window_event(|window, event| {
            if matches!(event, WindowEvent::CloseRequested { .. }) {
                commands::flush_all(window.app_handle());
            }
        });
    let builder = builder.invoke_handler(tauri::generate_handler![
        commands::inspect_folder,
        commands::create_book,
        commands::open_book,
        commands::save_progress,
        commands::close_book,
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
