// Cabinet (v1.1.0) — multi-window floating panels.
//
// Each detached window is a child `WebviewWindow` running the same Svelte
// bundle as the main app, but with `?panel=<id>&windowId=<wid>&doc=<path>`
// query params that flip the frontend into "detached mode" — one panel,
// no sidebar, no tabs.
//
// `WindowRegistry` is the in-memory source of truth for currently-open
// detached windows, keyed by Tauri window label. Persistence to
// `~/.slab/windows.json` lands in Slice 4.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Geometry {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WindowState {
    pub label: String,
    pub panel_id: String,
    pub geometry: Geometry,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_doc: Option<String>,
}

/// In-memory registry of currently-open detached panel windows, keyed
/// by Tauri window label (e.g. `panel-beacon-1`). Thread-safe.
///
/// Stored via `tauri::Manager::manage` so command handlers can look up
/// `tauri::State<'_, WindowRegistry>` without plumbing.
#[derive(Default)]
pub struct WindowRegistry {
    inner: Mutex<HashMap<String, WindowState>>,
}

impl WindowRegistry {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }

    /// Insert or replace a window record. The label inside `s` is used
    /// as the map key.
    pub fn upsert(&self, s: WindowState) {
        self.inner.lock().unwrap().insert(s.label.clone(), s);
    }

    pub fn get(&self, label: &str) -> Option<WindowState> {
        self.inner.lock().unwrap().get(label).cloned()
    }

    pub fn remove(&self, label: &str) -> Option<WindowState> {
        self.inner.lock().unwrap().remove(label)
    }

    /// Snapshot of all registered windows, sorted by label for stable
    /// ordering in the UI.
    pub fn list(&self) -> Vec<WindowState> {
        let mut v: Vec<_> = self.inner.lock().unwrap().values().cloned().collect();
        v.sort_by(|a, b| a.label.cmp(&b.label));
        v
    }

    /// Next available label for a panel kind, e.g. `panel-beacon-3`.
    /// Scans existing labels for the highest `panel-<panel_id>-N` suffix
    /// and returns N+1. Reused IDs after a close are *not* reclaimed;
    /// this keeps labels stable while a session is running and avoids
    /// surprising users with re-numbered windows.
    pub fn next_label(&self, panel_id: &str) -> String {
        let g = self.inner.lock().unwrap();
        let mut max_n = 0u32;
        let prefix = format!("panel-{}-", panel_id);
        for k in g.keys() {
            if let Some(rest) = k.strip_prefix(&prefix) {
                if let Ok(n) = rest.parse::<u32>() {
                    max_n = max_n.max(n);
                }
            }
        }
        format!("{}{}", prefix, max_n + 1)
    }
}

/// Sensible per-panel default size + position for newly-spawned detached
/// windows. We don't try to be clever about screen geometry here; the
/// OS handles edge clamping, and Slice 4 will persist user-resized
/// dimensions on subsequent opens.
pub fn default_geometry_for_panel(panel_id: &str) -> Geometry {
    match panel_id {
        // Wide for the document table + tags sidebar.
        "library" => Geometry {
            x: 80,
            y: 80,
            width: 1000,
            height: 720,
        },
        // Tall narrow chat column — slots next to the main reader.
        "beacon" => Geometry {
            x: 120,
            y: 100,
            width: 520,
            height: 760,
        },
        // Roomy enough for a 1-page render at ~125 % zoom.
        "reader" => Geometry {
            x: 100,
            y: 100,
            width: 900,
            height: 760,
        },
        "search" => Geometry {
            x: 140,
            y: 120,
            width: 720,
            height: 680,
        },
        "pii" => Geometry {
            x: 140,
            y: 120,
            width: 760,
            height: 720,
        },
        _ => Geometry {
            x: 140,
            y: 120,
            width: 720,
            height: 640,
        },
    }
}

/// URL-encode just enough of the path to survive the query string.
/// We deliberately don't pull in a full percent-encoding crate: the
/// frontend only needs to round-trip an opaque path string. Special
/// chars handled: space, `#`, `&`, `?`, `%`.
fn encode_doc_param(p: &str) -> String {
    let mut out = String::with_capacity(p.len());
    for c in p.chars() {
        match c {
            ' ' => out.push_str("%20"),
            '#' => out.push_str("%23"),
            '&' => out.push_str("%26"),
            '?' => out.push_str("%3F"),
            '%' => out.push_str("%25"),
            _ => out.push(c),
        }
    }
    out
}

#[tauri::command]
pub fn slab_window_open(
    app: tauri::AppHandle,
    state: tauri::State<'_, WindowRegistry>,
    panel_id: String,
    target_doc: Option<String>,
) -> Result<String, String> {
    use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

    let label = state.next_label(&panel_id);
    let geom = default_geometry_for_panel(&panel_id);

    let mut url = format!("/?panel={}&windowId={}", panel_id, label);
    if let Some(p) = &target_doc {
        url.push_str("&doc=");
        url.push_str(&encode_doc_param(p));
    }

    let webview_url = WebviewUrl::App(url.into());
    let title = format!("Slab — {}", title_case(&panel_id));

    let window = WebviewWindowBuilder::new(&app, &label, webview_url)
        .title(title)
        .inner_size(geom.width as f64, geom.height as f64)
        .position(geom.x as f64, geom.y as f64)
        .resizable(true)
        .decorations(true)
        .build()
        .map_err(|e| format!("failed to build window: {}", e))?;

    state.upsert(WindowState {
        label: label.clone(),
        panel_id,
        geometry: geom,
        target_doc,
    });

    // When the OS destroys the window (user clicked X, app shutdown,
    // etc.), drop it from the registry so `slab_window_list` stays
    // accurate.
    let app_clone = app.clone();
    let label_clone = label.clone();
    window.on_window_event(move |e| {
        if let tauri::WindowEvent::Destroyed = e {
            if let Some(reg) = app_clone.try_state::<WindowRegistry>() {
                reg.remove(&label_clone);
            }
        }
    });

    Ok(label)
}

#[tauri::command]
pub fn slab_window_close(
    app: tauri::AppHandle,
    state: tauri::State<'_, WindowRegistry>,
    label: String,
) -> Result<(), String> {
    use tauri::Manager;
    if let Some(w) = app.get_webview_window(&label) {
        w.close().map_err(|e| e.to_string())?;
    }
    state.remove(&label);
    Ok(())
}

#[tauri::command]
pub fn slab_window_list(state: tauri::State<'_, WindowRegistry>) -> Vec<WindowState> {
    state.list()
}

/// Title-case a panel id for the window chrome — "beacon" → "Beacon",
/// "pii" → "PII". Avoids pulling in heck for one fn.
fn title_case(s: &str) -> String {
    match s {
        "pii" => "PII".to_string(),
        _ => {
            let mut chars = s.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => c.to_uppercase().chain(chars).collect(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_state_roundtrips_json() {
        let s = WindowState {
            label: "panel-beacon-1".into(),
            panel_id: "beacon".into(),
            geometry: Geometry {
                x: 100,
                y: 80,
                width: 640,
                height: 720,
            },
            target_doc: Some("/tmp/example.pdf".into()),
        };
        let j = serde_json::to_string(&s).unwrap();
        let back: WindowState = serde_json::from_str(&j).unwrap();
        assert_eq!(s.label, back.label);
        assert_eq!(s.geometry.width, 640);
        assert_eq!(s.target_doc.as_deref(), Some("/tmp/example.pdf"));
    }

    #[test]
    fn window_state_target_doc_omitted_when_none() {
        let s = WindowState {
            label: "panel-library-1".into(),
            panel_id: "library".into(),
            geometry: Geometry {
                x: 0,
                y: 0,
                width: 800,
                height: 600,
            },
            target_doc: None,
        };
        let j = serde_json::to_string(&s).unwrap();
        assert!(!j.contains("target_doc"));
    }

    #[test]
    fn registry_insert_and_get() {
        let reg = WindowRegistry::new();
        let s = WindowState {
            label: "lib-1".into(),
            panel_id: "library".into(),
            geometry: Geometry {
                x: 0,
                y: 0,
                width: 800,
                height: 600,
            },
            target_doc: None,
        };
        reg.upsert(s.clone());
        assert_eq!(reg.get("lib-1").unwrap().panel_id, "library");
        assert_eq!(reg.list().len(), 1);
    }

    #[test]
    fn registry_remove() {
        let reg = WindowRegistry::new();
        reg.upsert(WindowState {
            label: "x".into(),
            panel_id: "beacon".into(),
            geometry: Geometry {
                x: 0,
                y: 0,
                width: 100,
                height: 100,
            },
            target_doc: None,
        });
        assert!(reg.remove("x").is_some());
        assert!(reg.list().is_empty());
        assert!(reg.remove("x").is_none(), "double-remove should be None");
    }

    #[test]
    fn registry_next_label_increments_per_panel() {
        let reg = WindowRegistry::new();
        assert_eq!(reg.next_label("beacon"), "panel-beacon-1");
        reg.upsert(WindowState {
            label: "panel-beacon-1".into(),
            panel_id: "beacon".into(),
            geometry: Geometry {
                x: 0,
                y: 0,
                width: 100,
                height: 100,
            },
            target_doc: None,
        });
        assert_eq!(reg.next_label("beacon"), "panel-beacon-2");
        // Library namespace stays at 1
        assert_eq!(reg.next_label("library"), "panel-library-1");
    }

    #[test]
    fn registry_next_label_handles_gaps() {
        let reg = WindowRegistry::new();
        reg.upsert(WindowState {
            label: "panel-reader-5".into(),
            panel_id: "reader".into(),
            geometry: Geometry {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
            target_doc: None,
        });
        // After a window at -5 exists, the next should be -6 — not -1.
        assert_eq!(reg.next_label("reader"), "panel-reader-6");
    }

    #[test]
    fn registry_list_is_sorted_by_label() {
        let reg = WindowRegistry::new();
        for label in ["panel-beacon-2", "panel-beacon-1", "panel-library-1"] {
            reg.upsert(WindowState {
                label: label.into(),
                panel_id: "x".into(),
                geometry: Geometry {
                    x: 0,
                    y: 0,
                    width: 1,
                    height: 1,
                },
                target_doc: None,
            });
        }
        let labels: Vec<_> = reg.list().into_iter().map(|s| s.label).collect();
        assert_eq!(
            labels,
            vec!["panel-beacon-1", "panel-beacon-2", "panel-library-1"]
        );
    }

    #[test]
    fn default_geometry_for_panel_beacon_is_narrow_tall() {
        let g = default_geometry_for_panel("beacon");
        // Beacon should be a tall narrow column.
        assert!(g.width <= 600, "beacon too wide: {}", g.width);
        assert!(g.height >= 600, "beacon too short: {}", g.height);
    }

    #[test]
    fn default_geometry_for_panel_library_is_wide() {
        let g = default_geometry_for_panel("library");
        // Library needs room for the document table.
        assert!(g.width >= 900, "library too narrow: {}", g.width);
    }

    #[test]
    fn default_geometry_for_panel_unknown_falls_back() {
        let g = default_geometry_for_panel("custom-thing");
        assert_eq!(g.width, 720);
        assert_eq!(g.height, 640);
    }

    #[test]
    fn encode_doc_param_escapes_special_chars() {
        assert_eq!(encode_doc_param("/tmp/a.pdf"), "/tmp/a.pdf");
        assert_eq!(encode_doc_param("/tmp/a b.pdf"), "/tmp/a%20b.pdf");
        assert_eq!(encode_doc_param("a#b&c?d%e"), "a%23b%26c%3Fd%25e");
    }

    #[test]
    fn title_case_handles_panel_ids() {
        assert_eq!(title_case("beacon"), "Beacon");
        assert_eq!(title_case("library"), "Library");
        assert_eq!(title_case("pii"), "PII");
        assert_eq!(title_case(""), "");
    }
}
