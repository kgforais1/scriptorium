//! Pre-M1 flipbook spike: lists a folder of page scans for the webview and admits those
//! files to the asset protocol scope. `tauri dev` builds only. M1 replaces this with real book loading.

use std::path::{Path, PathBuf};
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

fn is_page(p: &Path) -> bool {
    p.extension()
        .and_then(|x| x.to_str())
        .is_some_and(|x| EXTS.contains(&x.to_ascii_lowercase().as_str()))
}

/// Returns absolute paths of the spike's page images, sorted by file name. Only regular files
/// whose real path stays inside the folder and whose name is UTF-8 are listed, so a symlink
/// cannot admit an outside file and the webview gets back exactly the path that was admitted.
#[tauri::command]
pub fn spike_pages(app: AppHandle) -> Result<Vec<String>, String> {
    let dir = spike_dir().canonicalize().map_err(|e| {
        format!("spike page folder not found ({e}); run scripts/make-spike-fixtures.sh")
    })?;
    let mut pages = Vec::new();
    for entry in std::fs::read_dir(&dir).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if !is_page(&path) {
            continue;
        }
        let Ok(real) = path.canonicalize() else {
            continue;
        };
        // Check the resolved target too, so an image-named symlink cannot admit another file.
        if !is_page(&real) || !real.starts_with(&dir) || !real.is_file() {
            continue;
        }
        if let Some(s) = real.to_str() {
            pages.push(s.to_owned());
        }
    }
    pages.sort();
    // Admit only the listed images, not every file in the folder. Grants are process-wide and
    // never revoked: a path stays admitted after its file is replaced or removed, until restart.
    let scope = app.asset_protocol_scope();
    for page in &pages {
        scope.allow_file(page).map_err(|e| e.to_string())?;
    }
    Ok(pages)
}
