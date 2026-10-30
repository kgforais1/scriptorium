//! Scriptorium Tauri library entry.
//!
//! Hosts the desktop app runtime and shared Rust modules (book format types).
//! Tauri commands for ingest/load land in later milestones; M0 ships types only.

#![warn(missing_docs)]

pub mod book_format;
pub mod ingest;
pub mod library;
// `dev` is Tauri's cfg for `tauri dev`, including `tauri dev --release`, where the spike UI runs.
#[cfg(dev)]
mod spike;

/// Starts the Tauri app and returns when the app exits.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default().plugin(tauri_plugin_dialog::init());
    #[cfg(dev)]
    let builder = builder.invoke_handler(tauri::generate_handler![spike::spike_pages]);
    builder
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
