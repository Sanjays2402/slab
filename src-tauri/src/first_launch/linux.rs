//! Linux first-launch install (issue #25, v2.0.3).
//!
//! Installs to `~/.local/bin/slab` (XDG, always on PATH for modern
//! distros) and writes a Desktop Entry to `~/.local/share/applications/
//! slab.desktop` so KDE / GNOME / XFCE menus pick it up. No `sudo`,
//! no `pkexec`, no `dbus-launch` games.

use std::path::{Path, PathBuf};

/// Canonical install destination: `~/.local/bin/slab` (the executable
/// itself, not a directory — Linux distros don't bundle apps).
pub fn canonical_install_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".local").join("bin").join("slab"))
}

/// Desktop Entry path: `~/.local/share/applications/slab.desktop`.
pub fn desktop_entry_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| {
        h.join(".local")
            .join("share")
            .join("applications")
            .join("slab.desktop")
    })
}

/// MIME default-handler config file: `~/.config/mimeapps.list`.
pub fn mimeapps_list_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".config").join("mimeapps.list"))
}

/// Detect if the user is running from a temporary / unattractive
/// location — Downloads, `/tmp`, an AppImage mount point, etc.
pub fn looks_like_temporary_location(exe: &Path) -> bool {
    let s = exe.to_string_lossy();
    s.contains("/Downloads/")
        || s.contains("/Desktop/")
        || s.starts_with("/tmp/")
        || s.contains("/.mount_") // AppImage FUSE mount
        || s.starts_with("/run/")
        || s.starts_with("/media/")
        || s.starts_with("/mnt/")
}

/// Build the Desktop Entry body. Public so tests can assert its shape.
pub fn build_desktop_entry(exec_path: &Path) -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Slab\n\
         GenericName=PDF Workstation\n\
         Comment=Fast, free, offline PDF editor with local AI\n\
         Exec={} %F\n\
         Icon=slab\n\
         Terminal=false\n\
         Categories=Office;Viewer;\n\
         MimeType=application/pdf;\n\
         StartupWMClass=Slab\n\
         StartupNotify=true\n\
         X-Slab-Installed=1\n",
        exec_path.display()
    )
}

/// Copy the running binary to the canonical install path.
fn copy_binary(src: &Path, dest: &Path) -> std::io::Result<()> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if dest.exists() {
        std::fs::remove_file(dest)?;
    }
    std::fs::copy(src, dest)?;
    // chmod 0755 so the binary is executable post-copy.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o755);
        std::fs::set_permissions(dest, perms)?;
    }
    Ok(())
}

/// Write the Desktop Entry to disk.
fn write_desktop_entry(target_exe: &Path) -> std::io::Result<()> {
    let path = desktop_entry_path()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no HOME"))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, build_desktop_entry(target_exe))?;
    // Best-effort `update-desktop-database` refresh — non-fatal.
    let _ = std::process::Command::new("update-desktop-database")
        .arg(path.parent().unwrap())
        .status();
    Ok(())
}

/// Perform the install. Returns the new exe path.
pub fn install() -> std::io::Result<PathBuf> {
    let src = std::env::current_exe()?;
    let dest = canonical_install_dir()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no HOME"))?;
    copy_binary(&src, &dest)?;
    let _ = write_desktop_entry(&dest);
    Ok(dest)
}

/// Best-effort: mark Slab as the default PDF handler via xdg-mime.
pub fn set_default_pdf_handler() -> std::io::Result<()> {
    let status = std::process::Command::new("xdg-mime")
        .arg("default")
        .arg("slab.desktop")
        .arg("application/pdf")
        .status()?;
    if !status.success() {
        return Err(std::io::Error::other("xdg-mime exited non-zero"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_install_dir_ends_in_local_bin_slab() {
        let d = canonical_install_dir().expect("home on test host");
        assert!(d.ends_with(".local/bin/slab"), "{d:?}");
    }

    #[test]
    fn desktop_entry_path_lives_under_share_applications() {
        let p = desktop_entry_path().expect("home on test host");
        assert!(
            p.ends_with(".local/share/applications/slab.desktop"),
            "{p:?}"
        );
    }

    #[test]
    fn build_desktop_entry_uses_exec_path() {
        let body = build_desktop_entry(Path::new("/home/x/.local/bin/slab"));
        assert!(body.contains("Exec=/home/x/.local/bin/slab %F"));
        assert!(body.contains("MimeType=application/pdf;"));
        assert!(body.contains("Categories=Office;Viewer;"));
        assert!(body.contains("Type=Application"));
        assert!(body.contains("StartupWMClass=Slab"));
    }

    #[test]
    fn build_desktop_entry_includes_install_marker() {
        // The X-Slab-Installed marker lets us distinguish a self-install
        // .desktop from one shipped by a distro package.
        let body = build_desktop_entry(Path::new("/usr/local/bin/slab"));
        assert!(body.contains("X-Slab-Installed=1"));
    }

    #[test]
    fn looks_like_temporary_location_detects_appimage_mount() {
        assert!(looks_like_temporary_location(Path::new(
            "/tmp/.mount_Slab-12345/usr/bin/slab"
        )));
    }

    #[test]
    fn looks_like_temporary_location_detects_downloads() {
        assert!(looks_like_temporary_location(Path::new(
            "/home/x/Downloads/Slab.AppImage"
        )));
    }

    #[test]
    fn looks_like_temporary_location_is_false_for_local_bin() {
        assert!(!looks_like_temporary_location(Path::new(
            "/home/x/.local/bin/slab"
        )));
    }
}
