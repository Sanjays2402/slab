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
}
