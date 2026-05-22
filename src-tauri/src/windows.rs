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
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Geometry {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    /// When true, the window is built fullscreen (covers the active
    /// display, hides chrome). Used by the Theater "audience" window
    /// so a presenter's PDF fills the projector screen.
    #[serde(default)]
    pub fullscreen: bool,
    /// When false, the window is built without title-bar / borders.
    /// Theater audience windows render without decorations so the
    /// projection is uninterrupted; defaults to true to keep every
    /// existing panel decorated as before.
    #[serde(default = "default_decorations")]
    pub decorations: bool,
    /// When `Some(true)`, the window is pinned above all others. Used
    /// for Theater audience so OS notifications can't slip in front of
    /// a presentation. `None` means "OS default" (effectively false).
    #[serde(default)]
    pub always_on_top: Option<bool>,
    /// When false, the window cannot be resized after spawn. Theater
    /// audience uses this to keep the canvas locked to the projector
    /// resolution. Defaults to true (resizable) for normal panels.
    #[serde(default = "default_resizable")]
    pub resizable: bool,
}

fn default_decorations() -> bool {
    true
}

fn default_resizable() -> bool {
    true
}

impl Default for Geometry {
    fn default() -> Self {
        Self {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            fullscreen: false,
            decorations: true,
            always_on_top: None,
            resizable: true,
        }
    }
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
            ..Default::default()
        },
        // Tall narrow chat column — slots next to the main reader.
        "beacon" => Geometry {
            x: 120,
            y: 100,
            width: 520,
            height: 760,
            ..Default::default()
        },
        // Roomy enough for a 1-page render at ~125 % zoom.
        "reader" => Geometry {
            x: 100,
            y: 100,
            width: 900,
            height: 760,
            ..Default::default()
        },
        "search" => Geometry {
            x: 140,
            y: 120,
            width: 720,
            height: 680,
            ..Default::default()
        },
        "pii" => Geometry {
            x: 140,
            y: 120,
            width: 760,
            height: 720,
            ..Default::default()
        },
        // Theater (v2.3.0) — audience window for projector / second display.
        // Fullscreen, undecorated, pinned above other windows, locked
        // resolution so a stray drag-resize can't reflow mid-talk.
        "theater" => Geometry {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
            fullscreen: true,
            decorations: false,
            always_on_top: Some(true),
            resizable: false,
        },
        // Theater (v2.3.0) — presenter control window. Decorated, resizable,
        // sized for a typical laptop screen so the operator can see current
        // slide + next slide + speaker notes + timer at a glance.
        "theater_control" => Geometry {
            x: 80,
            y: 80,
            width: 1280,
            height: 800,
            ..Default::default()
        },
        _ => Geometry {
            x: 140,
            y: 120,
            width: 720,
            height: 640,
            ..Default::default()
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

// ---------- Persistence to ~/.slab/windows.json (Slice 4) ----------

/// Resolve the on-disk path for the windows registry.
///
/// Honours two override env vars so tests don't trample the real home:
///   - `SLAB_HOME_OVERRIDE` — full path used as the `.slab/` parent
///   - `SLAB_CONFIG_DIR` — same convention used by `ai::config`, kept
///     in sync so a single env override redirects every Slab data file
///
/// Falls back to `$HOME/.slab/`. Creates the parent directory.
fn windows_json_path() -> Result<PathBuf, String> {
    let base = if let Ok(o) = std::env::var("SLAB_HOME_OVERRIDE") {
        PathBuf::from(o).join(".slab")
    } else if let Ok(d) = std::env::var("SLAB_CONFIG_DIR") {
        PathBuf::from(d)
    } else {
        let home = std::env::var("HOME").map_err(|_| "no HOME env var".to_string())?;
        PathBuf::from(home).join(".slab")
    };
    fs::create_dir_all(&base).map_err(|e| format!("creating {}: {}", base.display(), e))?;
    Ok(base.join("windows.json"))
}

/// Persist a snapshot of the registry to disk. Atomic-ish: write to a
/// temp sibling then rename. Returns Err on IO failure; callers are
/// free to ignore (we don't want a transient disk hiccup to crash the
/// whole detach flow).
pub fn save_windows(states: &[WindowState]) -> Result<(), String> {
    let p = windows_json_path()?;
    let tmp = p.with_extension("json.tmp");
    let body = serde_json::to_string_pretty(states).map_err(|e| e.to_string())?;
    fs::write(&tmp, body).map_err(|e| format!("writing {}: {}", tmp.display(), e))?;
    fs::rename(&tmp, &p).map_err(|e| format!("renaming to {}: {}", p.display(), e))?;
    Ok(())
}

/// Load the persisted registry from disk. Returns an empty vec if the
/// file is missing (first-ever launch). Parse errors propagate so the
/// caller can decide whether to ignore-and-continue or surface.
pub fn load_windows() -> Result<Vec<WindowState>, String> {
    let p = windows_json_path()?;
    if !p.exists() {
        return Ok(vec![]);
    }
    let body = fs::read_to_string(&p).map_err(|e| format!("reading {}: {}", p.display(), e))?;
    let v: Vec<WindowState> =
        serde_json::from_str(&body).map_err(|e| format!("parsing {}: {}", p.display(), e))?;
    Ok(v)
}

/// Best-effort flush of the registry to disk. Logs but never throws —
/// we never want a disk problem to break the in-app detach flow.
fn flush_to_disk(reg: &WindowRegistry) {
    if let Err(e) = save_windows(&reg.list()) {
        eprintln!("[cabinet] failed to persist windows.json: {}", e);
    }
}

/// Wire the standard event handlers (Destroyed/Moved/Resized) onto a
/// freshly-built `WebviewWindow`. Extracted so `slab_window_open` and
/// the launch-restore path in `lib::run` use identical bookkeeping.
pub fn wire_window_events(window: &tauri::WebviewWindow, app: tauri::AppHandle, label: String) {
    use tauri::Manager;
    window.on_window_event(move |e| match e {
        tauri::WindowEvent::Destroyed => {
            if let Some(reg) = app.try_state::<WindowRegistry>() {
                reg.remove(&label);
                flush_to_disk(&reg);
            }
        }
        tauri::WindowEvent::Moved(p) => {
            if let Some(reg) = app.try_state::<WindowRegistry>() {
                if let Some(mut s) = reg.get(&label) {
                    s.geometry.x = p.x;
                    s.geometry.y = p.y;
                    reg.upsert(s);
                    flush_to_disk(&reg);
                }
            }
        }
        tauri::WindowEvent::Resized(sz) => {
            if let Some(reg) = app.try_state::<WindowRegistry>() {
                if let Some(mut s) = reg.get(&label) {
                    s.geometry.width = sz.width;
                    s.geometry.height = sz.height;
                    reg.upsert(s);
                    flush_to_disk(&reg);
                }
            }
        }
        _ => {}
    });
}

/// Hard cap on concurrent detached windows. Each WebviewWindow runs its
/// own webview process (~30-60 MB RSS); past this point we're hurting
/// the user's machine more than we're helping. Tunable later.
const MAX_DETACHED_WINDOWS: usize = 6;

#[tauri::command]
pub fn slab_window_open(
    app: tauri::AppHandle,
    state: tauri::State<'_, WindowRegistry>,
    panel_id: String,
    target_doc: Option<String>,
) -> Result<String, String> {
    use tauri::{WebviewUrl, WebviewWindowBuilder};

    if state.list().len() >= MAX_DETACHED_WINDOWS {
        return Err(format!(
            "Too many detached windows ({} max). Close one before opening another.",
            MAX_DETACHED_WINDOWS
        ));
    }

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
        .resizable(geom.resizable)
        .decorations(geom.decorations)
        .fullscreen(geom.fullscreen)
        .always_on_top(geom.always_on_top.unwrap_or(false))
        .build()
        .map_err(|e| format!("failed to build window: {}", e))?;

    state.upsert(WindowState {
        label: label.clone(),
        panel_id,
        geometry: geom,
        target_doc,
    });
    flush_to_disk(&state);

    wire_window_events(&window, app.clone(), label.clone());

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
    flush_to_disk(&state);
    Ok(())
}

#[tauri::command]
pub fn slab_window_list(state: tauri::State<'_, WindowRegistry>) -> Vec<WindowState> {
    state.list()
}

/// Spawn or focus a singleton window for a given `panel_id`. Unlike
/// `slab_window_open`, which always mints a new label, this function
/// reuses an existing window for the same `panel_id` if one is already
/// open — used by the Theater audience/control windows which must be
/// strictly unique (you can't run two presenter sessions at once).
///
/// Returns the resulting window label so callers can target it for
/// follow-up `slab_window_close`, focus, or event emission.
pub fn ensure_panel_window(
    app: &tauri::AppHandle,
    state: &WindowRegistry,
    panel_id: &str,
    target_doc: Option<String>,
) -> Result<String, String> {
    use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

    // Singleton: if a window with this panel_id is registered, focus it
    // and return early. Avoids stacking two audience windows on a
    // double-click of the operator's Present button.
    if let Some(existing) = state.list().into_iter().find(|s| s.panel_id == panel_id) {
        if let Some(w) = app.get_webview_window(&existing.label) {
            let _ = w.set_focus();
            return Ok(existing.label);
        }
        // Registry entry exists but window is gone — clean it up and
        // re-spawn below.
        state.remove(&existing.label);
    }

    let label = state.next_label(panel_id);
    let geom = default_geometry_for_panel(panel_id);

    let mut url = format!("/?panel={}&windowId={}", panel_id, label);
    if let Some(p) = &target_doc {
        url.push_str("&doc=");
        url.push_str(&encode_doc_param(p));
    }
    let webview_url = WebviewUrl::App(url.into());
    let title = format!("Slab — {}", title_case(panel_id));

    let window = WebviewWindowBuilder::new(app, &label, webview_url)
        .title(title)
        .inner_size(geom.width as f64, geom.height as f64)
        .position(geom.x as f64, geom.y as f64)
        .resizable(geom.resizable)
        .decorations(geom.decorations)
        .fullscreen(geom.fullscreen)
        .always_on_top(geom.always_on_top.unwrap_or(false))
        .build()
        .map_err(|e| format!("failed to build window: {}", e))?;

    state.upsert(WindowState {
        label: label.clone(),
        panel_id: panel_id.to_string(),
        geometry: geom,
        target_doc,
    });
    flush_to_disk(state);

    wire_window_events(&window, app.clone(), label.clone());

    Ok(label)
}

/// Close a specific labelled window and remove its registry entry.
/// Mirrors `slab_window_close` but exposed at module scope so other
/// commands (e.g. `slab_theater_close_windows`) can chain it.
pub fn close_label(
    app: &tauri::AppHandle,
    state: &WindowRegistry,
    label: &str,
) -> Result<(), String> {
    use tauri::Manager;
    if let Some(w) = app.get_webview_window(label) {
        w.close().map_err(|e| e.to_string())?;
    }
    state.remove(label);
    flush_to_disk(state);
    Ok(())
}

/// Restore previously-persisted detached windows on app launch. Called
/// from the Tauri `setup` hook in `lib::run`. Quiet on every error —
/// a missing/corrupt windows.json should never block app boot.
///
/// Each restored window is re-spawned at its last geometry and wired to
/// the same Destroyed/Moved/Resized save handlers as a fresh detach.
pub fn restore_windows(app: &tauri::AppHandle) {
    use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

    let states = match load_windows() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[cabinet] skipping window restore: {}", e);
            return;
        }
    };
    if states.is_empty() {
        return;
    }

    let registry = match app.try_state::<WindowRegistry>() {
        Some(r) => r,
        None => {
            eprintln!("[cabinet] WindowRegistry not managed — cannot restore");
            return;
        }
    };

    let mut restored = 0usize;
    for s in states {
        if s.label == "main" {
            continue;
        }
        if restored >= MAX_DETACHED_WINDOWS {
            eprintln!(
                "[cabinet] hit MAX_DETACHED_WINDOWS={} during restore, skipping remainder",
                MAX_DETACHED_WINDOWS
            );
            break;
        }

        let mut url = format!("/?panel={}&windowId={}", s.panel_id, s.label);
        if let Some(d) = &s.target_doc {
            url.push_str("&doc=");
            url.push_str(&encode_doc_param(d));
        }
        let webview_url = WebviewUrl::App(url.into());
        let title = format!("Slab — {}", title_case(&s.panel_id));

        match WebviewWindowBuilder::new(app, &s.label, webview_url)
            .title(title)
            .inner_size(s.geometry.width as f64, s.geometry.height as f64)
            .position(s.geometry.x as f64, s.geometry.y as f64)
            .resizable(s.geometry.resizable)
            .decorations(s.geometry.decorations)
            .fullscreen(s.geometry.fullscreen)
            .always_on_top(s.geometry.always_on_top.unwrap_or(false))
            .build()
        {
            Ok(window) => {
                registry.upsert(s.clone());
                wire_window_events(&window, app.clone(), s.label.clone());
                restored += 1;
            }
            Err(e) => {
                eprintln!("[cabinet] failed to restore window {}: {}", s.label, e);
            }
        }
    }

    // Re-flush the registry so any windows that failed to restore are
    // dropped from disk and we don't keep retrying the same bad entry.
    flush_to_disk(&registry);
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
                ..Default::default()
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
                ..Default::default()
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
                ..Default::default()
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
                ..Default::default()
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
                ..Default::default()
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
                ..Default::default()
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
                    ..Default::default()
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
    fn default_geometry_for_panel_theater_is_fullscreen_borderless() {
        let g = default_geometry_for_panel("theater");
        assert!(g.fullscreen, "theater must be fullscreen");
        assert!(!g.decorations, "theater must be undecorated");
        assert_eq!(g.always_on_top, Some(true), "theater must pin on top");
        assert!(!g.resizable, "theater must lock resolution");
    }

    #[test]
    fn default_geometry_for_panel_theater_control_is_decorated_window() {
        let g = default_geometry_for_panel("theater_control");
        assert!(!g.fullscreen);
        assert!(g.decorations);
        assert!(g.resizable);
        assert!(g.width >= 1024, "control window too narrow: {}", g.width);
        assert!(g.height >= 700, "control window too short: {}", g.height);
    }

    #[test]
    fn geometry_default_keeps_existing_panel_behaviour() {
        // Every pre-Theater panel should still come out decorated,
        // resizable, non-fullscreen, never pinned. Regression guard
        // so adding new fields with serde defaults can't silently flip
        // the behaviour for `library`, `beacon`, `reader`, etc.
        for id in ["library", "beacon", "reader", "search", "pii", "unknown"] {
            let g = default_geometry_for_panel(id);
            assert!(!g.fullscreen, "{id} should not be fullscreen");
            assert!(g.decorations, "{id} should be decorated");
            assert!(g.resizable, "{id} should be resizable");
            assert_eq!(g.always_on_top, None, "{id} should not pin");
        }
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

    // ---- Persistence (Slice 4) ---------------------------------------
    //
    // These mutate process env vars (SLAB_HOME_OVERRIDE), which is a
    // shared resource. Cargo's default thread-per-test scheduler would
    // race them — we serialise via a single Mutex. The tests are still
    // cheap (just ~10ms apiece) so this doesn't slow the suite.

    use std::sync::Mutex as StdMutex;
    static ENV_LOCK: StdMutex<()> = StdMutex::new(());

    /// RAII guard: sets `SLAB_HOME_OVERRIDE`, removes it on drop.
    /// Also wipes `SLAB_CONFIG_DIR` so an inherited env can't leak in.
    struct EnvOverride {
        _guard: std::sync::MutexGuard<'static, ()>,
        _tmp: tempfile::TempDir,
    }
    impl EnvOverride {
        fn new() -> Self {
            let guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
            let tmp = tempfile::tempdir().unwrap();
            std::env::set_var("SLAB_HOME_OVERRIDE", tmp.path());
            std::env::remove_var("SLAB_CONFIG_DIR");
            Self {
                _guard: guard,
                _tmp: tmp,
            }
        }
    }
    impl Drop for EnvOverride {
        fn drop(&mut self) {
            std::env::remove_var("SLAB_HOME_OVERRIDE");
        }
    }

    #[test]
    fn save_then_load_roundtrips() {
        let _env = EnvOverride::new();
        let states = vec![WindowState {
            label: "panel-library-1".into(),
            panel_id: "library".into(),
            geometry: Geometry {
                x: 10,
                y: 20,
                width: 800,
                height: 600,
                ..Default::default()
            },
            target_doc: None,
        }];
        save_windows(&states).expect("save");
        let loaded = load_windows().expect("load");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].label, "panel-library-1");
        assert_eq!(loaded[0].panel_id, "library");
        assert_eq!(loaded[0].geometry.width, 800);
    }

    #[test]
    fn load_returns_empty_when_file_missing() {
        let _env = EnvOverride::new();
        let loaded = load_windows().expect("load on empty home dir");
        assert!(loaded.is_empty());
    }

    #[test]
    fn save_creates_parent_directory() {
        let _env = EnvOverride::new();
        // `.slab/` doesn't exist yet — save should `mkdir -p`.
        save_windows(&[]).expect("save into fresh dir");
        let p = windows_json_path().unwrap();
        assert!(p.exists());
    }

    #[test]
    fn save_overwrites_previous_contents() {
        let _env = EnvOverride::new();
        let initial = vec![WindowState {
            label: "panel-beacon-1".into(),
            panel_id: "beacon".into(),
            geometry: Geometry {
                x: 0,
                y: 0,
                width: 500,
                height: 700,
                ..Default::default()
            },
            target_doc: Some("/tmp/a.pdf".into()),
        }];
        save_windows(&initial).unwrap();

        let updated = vec![WindowState {
            label: "panel-beacon-1".into(),
            panel_id: "beacon".into(),
            geometry: Geometry {
                x: 200,
                y: 300,
                width: 600,
                height: 800,
                ..Default::default()
            },
            target_doc: Some("/tmp/b.pdf".into()),
        }];
        save_windows(&updated).unwrap();

        let loaded = load_windows().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].geometry.x, 200);
        assert_eq!(loaded[0].target_doc.as_deref(), Some("/tmp/b.pdf"));
    }

    #[test]
    fn load_returns_error_on_corrupt_json() {
        let _env = EnvOverride::new();
        // First save a valid file so the path + parent dir exist.
        save_windows(&[]).unwrap();
        // Then trash it.
        let p = windows_json_path().unwrap();
        std::fs::write(&p, b"this is not json {{{").unwrap();

        let err = load_windows().expect_err("expected parse error");
        assert!(
            err.contains("parsing"),
            "error should mention parsing: {}",
            err
        );
    }
}
