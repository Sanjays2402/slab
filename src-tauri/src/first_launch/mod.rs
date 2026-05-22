//! First-launch self-install flow (issue #25, v2.0.3).
//!
//! Decides whether to show the "Install or Run from here?" modal on
//! application startup. OS-specific install code lives in cfg-gated
//! submodules. The `Probe` trait abstracts filesystem queries so the
//! decision logic is unit-testable on any host without touching the
//! user's real disk.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub mod state;

// All three OS submodules compile on every platform so the path-helper
// unit tests run in CI on any host. The `install()` functions are
// gated internally — calling Windows install on macOS returns
// `Unsupported`, never UB.
pub mod linux;
pub mod macos;
pub mod windows;

/// User's choice on the first-launch modal. Persisted in `launch.toml`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LaunchDecision {
    /// First launch hasn't happened yet (or state file missing).
    #[default]
    Pending,
    /// User chose "Run from here" — never prompt again.
    RunFromHere,
    /// User chose "Install" — we relocated the binary.
    Installed,
}

/// Persistent record of the user's first-launch decision.
///
/// Lives at `~/.config/slab/launch.toml` (XDG-style, even on macOS —
/// kept deliberately separate from `~/.slab/config.toml` so a config
/// reset doesn't re-trigger the modal).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchState {
    #[serde(default = "default_schema")]
    pub schema_version: u32,
    #[serde(default)]
    pub decision: LaunchDecision,
    /// RFC 3339 timestamp of the install action, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installed_at: Option<String>,
    /// Absolute path to the installed binary/.app/.exe.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installed_path: Option<PathBuf>,
}

fn default_schema() -> u32 {
    1
}

impl Default for LaunchState {
    fn default() -> Self {
        Self {
            schema_version: 1,
            decision: LaunchDecision::Pending,
            installed_at: None,
            installed_path: None,
        }
    }
}

/// Filesystem queries the decision logic needs. Implemented by the
/// real `OsProbe` and by `MockProbe` in tests.
pub trait Probe {
    /// Path to the currently-running executable.
    fn current_exe(&self) -> std::io::Result<PathBuf>;
    /// Path to the persistent state file (None on systems without HOME).
    fn state_path(&self) -> Option<PathBuf>;
    /// Where Slab *should* live on this OS, if we installed it.
    fn canonical_install_dir(&self) -> Option<PathBuf>;
}

/// Default OS probe used at runtime.
pub struct OsProbe;

impl Probe for OsProbe {
    fn current_exe(&self) -> std::io::Result<PathBuf> {
        std::env::current_exe()
    }
    fn state_path(&self) -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(".config").join("slab").join("launch.toml"))
    }
    fn canonical_install_dir(&self) -> Option<PathBuf> {
        #[cfg(target_os = "macos")]
        {
            macos::canonical_install_dir()
        }
        #[cfg(target_os = "windows")]
        {
            windows::canonical_install_dir()
        }
        #[cfg(target_os = "linux")]
        {
            linux::canonical_install_dir()
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        {
            None
        }
    }
}

/// Decide whether to show the install/run modal at startup.
///
/// Rules:
/// 1. `Decision != Pending`            → never prompt (user already chose).
/// 2. `exe` already inside `canonical_install_dir` → never prompt
///    (idempotency: launching the installed copy is always silent).
/// 3. Otherwise → prompt.
///
/// Returns `false` defensively when the probe can't tell us where state
/// lives — better to never nag than to nag every launch.
pub fn should_prompt<P: Probe>(probe: &P) -> bool {
    let Some(state_path) = probe.state_path() else {
        return false;
    };
    let st = state::load(&state_path).unwrap_or_default();
    if !matches!(st.decision, LaunchDecision::Pending) {
        return false;
    }
    let Ok(exe) = probe.current_exe() else {
        return false;
    };
    if let Some(install) = probe.canonical_install_dir() {
        if exe.starts_with(&install) {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_state_defaults_to_pending() {
        let s = LaunchState::default();
        assert!(matches!(s.decision, LaunchDecision::Pending));
        assert!(s.installed_at.is_none());
        assert!(s.installed_path.is_none());
        assert_eq!(s.schema_version, 1);
    }

    #[test]
    fn launch_state_roundtrips_toml() {
        let s = LaunchState {
            decision: LaunchDecision::RunFromHere,
            ..Default::default()
        };
        let text = toml::to_string(&s).unwrap();
        let back: LaunchState = toml::from_str(&text).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn launch_decision_serializes_snake_case() {
        let s = LaunchState {
            decision: LaunchDecision::RunFromHere,
            ..Default::default()
        };
        let text = toml::to_string(&s).unwrap();
        assert!(
            text.contains("run_from_here"),
            "expected snake_case; got:\n{text}"
        );
    }
}

#[cfg(test)]
struct MockProbe {
    pub exe: PathBuf,
    pub install_dir: PathBuf,
    pub state_path: PathBuf,
}

#[cfg(test)]
impl Probe for MockProbe {
    fn current_exe(&self) -> std::io::Result<PathBuf> {
        Ok(self.exe.clone())
    }
    fn state_path(&self) -> Option<PathBuf> {
        Some(self.state_path.clone())
    }
    fn canonical_install_dir(&self) -> Option<PathBuf> {
        Some(self.install_dir.clone())
    }
}

#[cfg(test)]
mod decision_tests {
    use super::*;
    use tempfile::tempdir;

    fn probe(
        exe: &std::path::Path,
        install_dir: &std::path::Path,
        state: &std::path::Path,
    ) -> MockProbe {
        MockProbe {
            exe: exe.to_path_buf(),
            install_dir: install_dir.to_path_buf(),
            state_path: state.to_path_buf(),
        }
    }

    #[test]
    fn pending_state_and_running_from_downloads_triggers_prompt() {
        let dir = tempdir().unwrap();
        let downloads = dir.path().join("Downloads");
        std::fs::create_dir_all(&downloads).unwrap();
        let exe = downloads.join("Slab.app/Contents/MacOS/slab");
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::write(&exe, b"binary").unwrap();
        let install = dir.path().join("Applications/Slab.app");
        let state = dir.path().join("state.toml");
        let p = probe(&exe, &install, &state);
        assert!(should_prompt(&p));
    }

    #[test]
    fn run_from_here_decision_skips_prompt() {
        let dir = tempdir().unwrap();
        let exe = dir.path().join("foo/slab");
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::write(&exe, b"x").unwrap();
        let state = dir.path().join("s.toml");
        state::save(
            &state,
            &LaunchState {
                decision: LaunchDecision::RunFromHere,
                ..Default::default()
            },
        )
        .unwrap();
        let p = probe(&exe, &dir.path().join("Apps/Slab"), &state);
        assert!(!should_prompt(&p));
    }

    #[test]
    fn already_in_canonical_dir_skips_prompt_even_when_state_pending() {
        let dir = tempdir().unwrap();
        let install = dir.path().join("Applications/Slab.app");
        let exe = install.join("Contents/MacOS/slab");
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::write(&exe, b"x").unwrap();
        let state = dir.path().join("s.toml");
        let p = probe(&exe, &install, &state);
        assert!(!should_prompt(&p));
    }

    #[test]
    fn installed_decision_skips_prompt() {
        let dir = tempdir().unwrap();
        let exe = dir.path().join("anywhere/slab");
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::write(&exe, b"x").unwrap();
        let state = dir.path().join("s.toml");
        state::save(
            &state,
            &LaunchState {
                decision: LaunchDecision::Installed,
                ..Default::default()
            },
        )
        .unwrap();
        let p = probe(&exe, &dir.path().join("X"), &state);
        assert!(!should_prompt(&p));
    }

    #[test]
    fn corrupt_state_file_falls_back_to_default_and_prompts() {
        let dir = tempdir().unwrap();
        let exe = dir.path().join("downloads/slab");
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::write(&exe, b"x").unwrap();
        let state = dir.path().join("s.toml");
        // Intentionally invalid TOML — load() returns Err, should_prompt
        // must treat it as default (Pending) rather than panic.
        std::fs::write(&state, b"this is not toml ===").unwrap();
        let p = probe(&exe, &dir.path().join("Apps/Slab"), &state);
        assert!(should_prompt(&p));
    }
}
