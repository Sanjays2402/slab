//! macOS first-launch install (issue #25, v2.0.3).
//!
//! Installs Slab to `~/Applications/Slab.app` — a per-user directory
//! that Finder, Spotlight, and Launch Services all index, but which
//! requires **no admin rights** and never triggers a sudo prompt.
//!
//! After copy we register the bundle with `lsregister` so it shows up
//! in "Open with…" menus immediately.

use std::path::{Path, PathBuf};

/// Walk up from the current binary path to the `.app` bundle root.
/// `/path/Slab.app/Contents/MacOS/slab` → `/path/Slab.app`.
///
/// Returns `None` if the path doesn't look like it lives inside a
/// `.app` bundle — in which case we shouldn't try to install (the user
/// is running an `slab` CLI binary, not the GUI).
pub fn app_bundle_root(exe: &Path) -> Option<PathBuf> {
    let macos = exe.parent()?;
    let contents = macos.parent()?;
    let app = contents.parent()?;
    if app.extension().and_then(|s| s.to_str()) == Some("app")
        && macos.file_name().and_then(|s| s.to_str()) == Some("MacOS")
        && contents.file_name().and_then(|s| s.to_str()) == Some("Contents")
    {
        Some(app.to_path_buf())
    } else {
        None
    }
}

/// Canonical install location for the Slab.app bundle on macOS.
pub fn canonical_install_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join("Applications").join("Slab.app"))
}

/// Whether the given exe path is somewhere the user *probably* shouldn't
/// be running from long-term (Downloads, Desktop, a mounted DMG, /tmp).
/// Used by the modal to nudge harder toward "Install".
pub fn looks_like_temporary_location(exe: &Path) -> bool {
    let s = exe.to_string_lossy();
    s.contains("/Downloads/")
        || s.contains("/Desktop/")
        || s.starts_with("/Volumes/")
        || s.starts_with("/tmp/")
        || s.starts_with("/private/var/folders/")
}

/// Copy `src` (.app bundle) → `dest`. Uses `cp -R` because the bundle
/// contains symlinks and per-file permissions that `std::fs::copy`
/// would flatten.
fn copy_bundle(src: &Path, dest: &Path) -> std::io::Result<()> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Idempotent re-install — remove any stale copy first.
    if dest.exists() {
        std::fs::remove_dir_all(dest)?;
    }
    let status = std::process::Command::new("cp")
        .arg("-R")
        .arg(src)
        .arg(dest)
        .status()?;
    if !status.success() {
        return Err(std::io::Error::other("cp -R failed"));
    }
    Ok(())
}

/// Try to register the freshly-installed bundle with Launch Services so
/// "Open With…" + Spotlight surface it without a logout. This is a
/// best-effort call — non-fatal if `lsregister` is missing.
fn register_with_launch_services(app: &Path) {
    let _ = std::process::Command::new(
        "/System/Library/Frameworks/CoreServices.framework/Frameworks/\
         LaunchServices.framework/Support/lsregister",
    )
    .arg("-f")
    .arg(app)
    .status();
}

/// Perform the install. Returns the path to the new executable.
pub fn install() -> std::io::Result<PathBuf> {
    let exe = std::env::current_exe()?;
    let src = app_bundle_root(&exe).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "slab is not running from a .app bundle — can't self-install",
        )
    })?;
    let dest = canonical_install_dir()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no HOME directory"))?;
    copy_bundle(&src, &dest)?;
    register_with_launch_services(&dest);
    Ok(dest.join("Contents/MacOS/slab"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_bundle_root_climbs_three_levels() {
        let p = PathBuf::from("/A/B/Slab.app/Contents/MacOS/slab");
        assert_eq!(app_bundle_root(&p), Some(PathBuf::from("/A/B/Slab.app")));
    }

    #[test]
    fn app_bundle_root_returns_none_for_non_app() {
        let p = PathBuf::from("/A/B/C/slab");
        assert_eq!(app_bundle_root(&p), None);
    }

    #[test]
    fn app_bundle_root_rejects_wrong_extension() {
        let p = PathBuf::from("/A/Slab.bundle/Contents/MacOS/slab");
        assert_eq!(app_bundle_root(&p), None);
    }

    #[test]
    fn app_bundle_root_rejects_wrong_intermediate_names() {
        // /A/Slab.app/Wrong/MacOS/slab
        let p = PathBuf::from("/A/Slab.app/Wrong/MacOS/slab");
        assert_eq!(app_bundle_root(&p), None);
    }

    #[test]
    fn looks_like_temporary_location_detects_downloads() {
        assert!(looks_like_temporary_location(&PathBuf::from(
            "/Users/x/Downloads/Slab.app/Contents/MacOS/slab"
        )));
    }

    #[test]
    fn looks_like_temporary_location_detects_desktop() {
        assert!(looks_like_temporary_location(&PathBuf::from(
            "/Users/x/Desktop/Slab.app/Contents/MacOS/slab"
        )));
    }

    #[test]
    fn looks_like_temporary_location_detects_dmg_mount() {
        assert!(looks_like_temporary_location(&PathBuf::from(
            "/Volumes/Slab Installer/Slab.app/Contents/MacOS/slab"
        )));
    }

    #[test]
    fn looks_like_temporary_location_is_false_for_applications() {
        assert!(!looks_like_temporary_location(&PathBuf::from(
            "/Users/x/Applications/Slab.app/Contents/MacOS/slab"
        )));
        assert!(!looks_like_temporary_location(&PathBuf::from(
            "/Applications/Slab.app/Contents/MacOS/slab"
        )));
    }

    #[test]
    fn canonical_install_dir_ends_in_slab_app() {
        let d = canonical_install_dir().expect("home dir on test host");
        assert!(d.ends_with("Applications/Slab.app"), "{d:?}");
    }
}
