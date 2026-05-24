//! Tauri commands for Atelier — recipe persistence + batch runner with
//! streaming progress over a `tauri::ipc::Channel`.
//!
//! Recipes are stored as pretty-printed JSON under
//! `$APP_CONFIG/atelier/recipes/<safe-name>.json`. On first load, if no
//! recipes exist, a "Nightly Discovery" preset is seeded so new users
//! see a working example in under 60 seconds.

use crate::pdf::atelier::batch::{run_recipe_batch, BatchProgress, BatchReport};
use crate::pdf::atelier::recipe::{Recipe, Step};
use std::path::{Path, PathBuf};
use tauri::ipc::Channel;
use tauri::Manager;

/// Curate a default recipe so a fresh install isn't an empty panel.
/// Targets the "scanned discovery PDFs" workflow that drives the
/// Atelier launch story: OCR everything → redact PII → bates → flatten.
pub fn nightly_discovery_preset() -> Recipe {
    Recipe {
        name: "Nightly Discovery".into(),
        version: 1,
        steps: vec![
            Step::Ocr {
                language: "eng".into(),
            },
            Step::AutoRedact {
                patterns: vec![],
                presets: vec!["ssn".into(), "email".into(), "phone".into()],
            },
            Step::Bates {
                prefix: "ACME".into(),
                start: 1,
                digits: 6,
            },
            Step::Flatten { dpi: 150 },
        ],
    }
}

/// Sanitize a recipe name into a safe filename.
fn safe_filename(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == ' ' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    let trimmed = out.trim().to_string();
    if trimmed.is_empty() {
        "Untitled".into()
    } else {
        trimmed
    }
}

pub fn save_recipe_to_dir(dir: &Path, recipe: &Recipe) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("{}.json", safe_filename(&recipe.name)));
    let json = serde_json::to_string_pretty(recipe).map_err(|e| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, format!("serialize: {e}"))
    })?;
    std::fs::write(&path, json)?;
    Ok(path)
}

pub fn list_recipes_in_dir(dir: &Path) -> std::io::Result<Vec<Recipe>> {
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut out = vec![];
    for entry in std::fs::read_dir(dir)? {
        let p = entry?.path();
        if p.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let s = match std::fs::read_to_string(&p) {
            Ok(s) => s,
            Err(_) => continue,
        };
        if let Ok(r) = serde_json::from_str::<Recipe>(&s) {
            out.push(r);
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

pub fn delete_recipe_from_dir(dir: &Path, name: &str) -> std::io::Result<()> {
    let path = dir.join(format!("{}.json", safe_filename(name)));
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

fn recipes_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_config_dir()
        .map_err(|e| e.to_string())?
        .join("atelier")
        .join("recipes"))
}

#[tauri::command]
pub fn atelier_save_recipe(app: tauri::AppHandle, recipe: Recipe) -> Result<String, String> {
    let dir = recipes_dir(&app)?;
    save_recipe_to_dir(&dir, &recipe)
        .map(|p| p.to_string_lossy().into_owned())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn atelier_load_recipes(app: tauri::AppHandle) -> Result<Vec<Recipe>, String> {
    let dir = recipes_dir(&app)?;
    let mut list = list_recipes_in_dir(&dir).map_err(|e| e.to_string())?;
    if list.is_empty() {
        let preset = nightly_discovery_preset();
        let _ = save_recipe_to_dir(&dir, &preset);
        list.push(preset);
    }
    Ok(list)
}

#[tauri::command]
pub fn atelier_delete_recipe(app: tauri::AppHandle, name: String) -> Result<(), String> {
    let dir = recipes_dir(&app)?;
    delete_recipe_from_dir(&dir, &name).map_err(|e| e.to_string())
}

/// Run `recipe` over every PDF in `in_dir`, writing to `out_dir`. Events
/// (per-file start, per-step start/completed/failed, per-file complete/fail)
/// stream over the `on_event` channel for live UI updates.
#[tauri::command]
pub fn atelier_run_batch(
    in_dir: String,
    out_dir: String,
    recipe: Recipe,
    on_event: Channel<BatchProgress>,
) -> Result<BatchReport, String> {
    let cb = move |p: BatchProgress| {
        let _ = on_event.send(p);
    };
    run_recipe_batch(Path::new(&in_dir), Path::new(&out_dir), &recipe, &cb)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn recipe_storage_round_trip() {
        let dir = tempdir().unwrap();
        let r = Recipe {
            name: "Nightly".into(),
            version: 1,
            steps: vec![],
        };
        let path = save_recipe_to_dir(dir.path(), &r).unwrap();
        assert!(path.exists());
        let list = list_recipes_in_dir(dir.path()).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "Nightly");
    }

    #[test]
    fn safe_filename_strips_path_separators() {
        assert_eq!(safe_filename("Foo/Bar"), "Foo_Bar");
        assert_eq!(safe_filename("../etc/passwd"), "___etc_passwd");
        assert_eq!(safe_filename(""), "Untitled");
        assert_eq!(safe_filename("   "), "Untitled");
        assert_eq!(safe_filename("My Recipe v2"), "My Recipe v2");
    }

    #[test]
    fn delete_removes_recipe() {
        let dir = tempdir().unwrap();
        let r = Recipe {
            name: "Doomed".into(),
            version: 1,
            steps: vec![],
        };
        save_recipe_to_dir(dir.path(), &r).unwrap();
        assert_eq!(list_recipes_in_dir(dir.path()).unwrap().len(), 1);
        delete_recipe_from_dir(dir.path(), "Doomed").unwrap();
        assert_eq!(list_recipes_in_dir(dir.path()).unwrap().len(), 0);
    }

    #[test]
    fn delete_missing_is_ok() {
        let dir = tempdir().unwrap();
        delete_recipe_from_dir(dir.path(), "Ghost").unwrap();
    }

    #[test]
    fn list_ignores_non_json_files() {
        let dir = tempdir().unwrap();
        std::fs::create_dir_all(dir.path()).unwrap();
        std::fs::write(dir.path().join("readme.txt"), b"hi").unwrap();
        std::fs::write(dir.path().join("malformed.json"), b"not json").unwrap();
        let r = Recipe {
            name: "Good".into(),
            version: 1,
            steps: vec![],
        };
        save_recipe_to_dir(dir.path(), &r).unwrap();
        let list = list_recipes_in_dir(dir.path()).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "Good");
    }

    #[test]
    fn nightly_preset_has_four_steps() {
        let p = nightly_discovery_preset();
        assert_eq!(p.steps.len(), 4);
        assert!(matches!(p.steps[0], Step::Ocr { .. }));
        assert!(matches!(p.steps[3], Step::Flatten { .. }));
    }

    #[test]
    fn list_sorted_alphabetically() {
        let dir = tempdir().unwrap();
        for name in ["Charlie", "Alpha", "Bravo"] {
            save_recipe_to_dir(
                dir.path(),
                &Recipe {
                    name: name.into(),
                    version: 1,
                    steps: vec![],
                },
            )
            .unwrap();
        }
        let list = list_recipes_in_dir(dir.path()).unwrap();
        let names: Vec<&str> = list.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["Alpha", "Bravo", "Charlie"]);
    }
}
