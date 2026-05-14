// Slab — fast, free, offline PDF tool.
// All operations run locally; nothing is ever uploaded.

mod pdf;

use pdf::{merge::merge_pdfs, PdfError};
use serde::Serialize;
use std::path::PathBuf;

/// Result returned from any Slab command. JSON-friendly for the Svelte side.
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CmdResult<T: Serialize> {
    Ok { value: T },
    Err { message: String },
}

impl<T: Serialize> From<Result<T, PdfError>> for CmdResult<T> {
    fn from(r: Result<T, PdfError>) -> Self {
        match r {
            Ok(v) => CmdResult::Ok { value: v },
            Err(e) => CmdResult::Err {
                message: e.to_string(),
            },
        }
    }
}

#[derive(Serialize)]
pub struct AppInfo {
    pub name: &'static str,
    pub version: &'static str,
    pub commit: &'static str,
}

#[tauri::command]
fn app_info() -> AppInfo {
    AppInfo {
        name: "Slab",
        version: env!("CARGO_PKG_VERSION"),
        commit: option_env!("SLAB_COMMIT").unwrap_or("dev"),
    }
}

#[tauri::command]
fn slab_merge(inputs: Vec<PathBuf>, output: PathBuf) -> CmdResult<PathBuf> {
    let out = output.clone();
    merge_pdfs(&inputs, output).map(|_| out).into()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .invoke_handler(tauri::generate_handler![app_info, slab_merge])
        .run(tauri::generate_context!())
        .expect("error while running Slab");
}
