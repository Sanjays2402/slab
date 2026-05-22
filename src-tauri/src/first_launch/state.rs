//! Atomic load/save of the first-launch `LaunchState` to disk.
//!
//! Writes go through a `.tmp` sidecar then `rename(2)` so a crash mid-write
//! can never produce a half-written `launch.toml` that bricks future
//! launches.

use super::LaunchState;
use std::io::Write;
use std::path::Path;

/// Load the launch state from `path`. Returns the default state if the
/// file does not exist. Returns `Err` only for I/O errors other than
/// `NotFound` or for genuinely unrecoverable disk errors.
///
/// **Note:** malformed TOML is *not* an error — we return `Err` so the
/// caller can decide, but `should_prompt` treats it as "default" so a
/// corrupt state file never blocks the app from launching.
pub fn load(path: &Path) -> std::io::Result<LaunchState> {
    match std::fs::read_to_string(path) {
        Ok(s) => {
            toml::from_str(&s).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(LaunchState::default()),
        Err(e) => Err(e),
    }
}

/// Atomically write the launch state to `path`. Creates the parent
/// directory if needed. Writes to `path.toml.tmp` first, fsyncs, then
/// renames — so concurrent readers and crash-mid-write are safe.
pub fn save(path: &Path, state: &LaunchState) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let body = toml::to_string_pretty(state)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let tmp = path.with_extension("toml.tmp");
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(body.as_bytes())?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::{LaunchDecision, LaunchState};
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn save_then_load_roundtrips() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("launch.toml");
        let s = LaunchState {
            decision: LaunchDecision::RunFromHere,
            ..Default::default()
        };
        save(&path, &s).unwrap();
        let back = load(&path).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn load_missing_returns_default() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("nope.toml");
        let s = load(&path).unwrap();
        assert_eq!(s, LaunchState::default());
    }

    #[test]
    fn save_is_atomic_via_tmp_rename() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("launch.toml");
        save(&path, &LaunchState::default()).unwrap();
        // tmp sidecar must not linger after a successful save.
        assert!(!dir.path().join("launch.toml.tmp").exists());
        // The real file must exist.
        assert!(path.exists());
    }

    #[test]
    fn save_creates_missing_parent_dir() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("nested/dirs/launch.toml");
        save(&path, &LaunchState::default()).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn load_returns_err_on_malformed_toml() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("bad.toml");
        std::fs::write(&path, b"][not toml===").unwrap();
        let err = load(&path).expect_err("malformed TOML must surface as Err");
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    }

    #[test]
    fn save_then_load_preserves_installed_path() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("launch.toml");
        let s = LaunchState {
            decision: LaunchDecision::Installed,
            installed_at: Some("2026-05-22T09:13:42Z".to_string()),
            installed_path: Some(std::path::PathBuf::from("/Users/x/Applications/Slab.app")),
            ..Default::default()
        };
        save(&path, &s).unwrap();
        let back = load(&path).unwrap();
        assert_eq!(s, back);
    }
}
