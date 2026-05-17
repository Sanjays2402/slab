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
}
