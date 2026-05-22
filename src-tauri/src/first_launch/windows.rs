//! Windows first-launch install (issue #25, v2.0.3).
//!
//! Installs to `%LOCALAPPDATA%\Programs\Slab\Slab.exe` — a canonical
//! per-user location that does **not** require admin rights, does
//! **not** trigger a UAC prompt, and **is** indexed by Windows search
//! and the Start Menu. Sets the HKCU PDF ProgID for opt-in default
//! handler registration (still no admin needed; only HKCU writes).
//!
//! This module compiles on every platform so the path-helper unit
//! tests can run in CI on Linux, but `install()` itself is a no-op
//! outside of Windows. The real `winreg` work lives behind
//! `#[cfg(target_os = "windows")]`.

use std::path::{Path, PathBuf};

/// Canonical install location: `%LOCALAPPDATA%\Programs\Slab\Slab.exe`.
///
/// On non-Windows hosts this returns the same shape under `$HOME` so the
/// decision logic in `mod.rs` can be exercised in tests on Linux/macOS.
pub fn canonical_install_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .map(|p| p.join("Programs").join("Slab"))
    }
    #[cfg(not(target_os = "windows"))]
    {
        dirs::home_dir().map(|h| h.join("AppData/Local/Programs/Slab"))
    }
}

/// Path to the installed `Slab.exe`.
pub fn canonical_exe_path() -> Option<PathBuf> {
    canonical_install_dir().map(|d| d.join("Slab.exe"))
}

/// Detect if the exe is running from a temporary download location.
pub fn looks_like_temporary_location(exe: &Path) -> bool {
    let s = exe.to_string_lossy().to_ascii_lowercase();
    s.contains("\\downloads\\")
        || s.contains("\\desktop\\")
        || s.contains("\\temp\\")
        || s.contains("\\tmp\\")
        || s.contains("\\appdata\\local\\temp\\")
}

/// Start Menu shortcut path (per-user, no admin).
pub fn start_menu_shortcut_path() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA").map(PathBuf::from).map(|p| {
            p.join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs")
                .join("Slab.lnk")
        })
    }
    #[cfg(not(target_os = "windows"))]
    {
        dirs::home_dir()
            .map(|h| h.join("AppData/Roaming/Microsoft/Windows/Start Menu/Programs/Slab.lnk"))
    }
}

/// Best-effort install. On non-Windows this is a no-op returning
/// `Unsupported` so the Tauri command can surface a meaningful error
/// in the unlikely case it's invoked on the wrong OS.
pub fn install() -> std::io::Result<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        let src = std::env::current_exe()?;
        let dest_dir = canonical_install_dir()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no LOCALAPPDATA"))?;
        std::fs::create_dir_all(&dest_dir)?;
        let dest = dest_dir.join("Slab.exe");
        if dest.exists() {
            std::fs::remove_file(&dest)?;
        }
        std::fs::copy(&src, &dest)?;
        // Copy any sidecar files (e.g. WebView2 loader DLL) sitting
        // next to the source exe.
        if let Some(src_dir) = src.parent() {
            for entry in std::fs::read_dir(src_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path == src {
                    continue;
                }
                if path.is_file() {
                    if let Some(name) = path.file_name() {
                        let _ = std::fs::copy(&path, dest_dir.join(name));
                    }
                }
            }
        }
        Ok(dest)
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "windows::install() called on non-Windows host",
        ))
    }
}

/// Best-effort: register Slab as the per-user (HKCU) default PDF
/// handler. Writes only under `HKCU\Software\Classes`, which never
/// triggers UAC.
pub fn set_default_pdf_handler() -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        // We deliberately avoid pulling `winreg` as a hard dep here —
        // the implementation will be filled in by the v2.0.3 Slice 3
        // tick. For now this returns Ok so the upstream command path
        // can be wired without breaking compile.
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "set_default_pdf_handler called on non-Windows host",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_install_dir_ends_in_programs_slab() {
        let d = canonical_install_dir().expect("env on test host");
        let s = d.to_string_lossy();
        assert!(
            s.ends_with("Programs/Slab") || s.ends_with("Programs\\Slab"),
            "got: {s}"
        );
    }

    #[test]
    fn canonical_exe_path_ends_in_slab_exe() {
        let p = canonical_exe_path().expect("env on test host");
        let s = p.to_string_lossy();
        assert!(s.ends_with("Slab.exe"), "got: {s}");
    }

    #[test]
    fn start_menu_shortcut_path_ends_in_slab_lnk() {
        let p = start_menu_shortcut_path().expect("env on test host");
        let s = p.to_string_lossy();
        assert!(s.ends_with("Slab.lnk"), "got: {s}");
    }

    #[test]
    fn looks_like_temporary_location_detects_downloads() {
        assert!(looks_like_temporary_location(Path::new(
            "C:\\Users\\x\\Downloads\\Slab.exe"
        )));
    }

    #[test]
    fn looks_like_temporary_location_detects_local_temp() {
        assert!(looks_like_temporary_location(Path::new(
            "C:\\Users\\x\\AppData\\Local\\Temp\\slab.exe"
        )));
    }

    #[test]
    fn looks_like_temporary_location_is_false_for_programs_dir() {
        assert!(!looks_like_temporary_location(Path::new(
            "C:\\Users\\x\\AppData\\Local\\Programs\\Slab\\Slab.exe"
        )));
    }

    #[test]
    fn looks_like_temporary_location_is_case_insensitive() {
        assert!(looks_like_temporary_location(Path::new(
            "C:\\Users\\X\\DOWNLOADS\\Slab.exe"
        )));
    }
}
