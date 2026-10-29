//! Pre-M1 flipbook spike: lists a folder of page scans for the webview and admits that folder
//! to the asset protocol scope. Debug builds only. M1 replaces this with real book loading.

use std::path::PathBuf;
use tauri::{AppHandle, Manager};

const EXTS: [&str; 3] = ["jpg", "jpeg", "png"];

fn spike_dir() -> PathBuf {
    std::env::var_os("SCRIPTORIUM_SPIKE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../tmp/fixtures/spike-pages"
            ))
        })
}

/// Returns absolute paths of the spike's page images, sorted by file name.
#[tauri::command]
pub fn spike_pages(app: AppHandle) -> Result<Vec<String>, String> {
    let dir = spike_dir().canonicalize().map_err(|e| {
        format!("spike page folder not found ({e}); run scripts/make-spike-fixtures.sh")
    })?;
    let mut pages: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map_err(|e| e.to_string())?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension()
                .and_then(|x| x.to_str())
                .is_some_and(|x| EXTS.contains(&x.to_ascii_lowercase().as_str()))
        })
        .collect();
    pages.sort();
    app.asset_protocol_scope()
        .allow_directory(&dir, false)
        .map_err(|e| e.to_string())?;
    Ok(pages
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect())
}
