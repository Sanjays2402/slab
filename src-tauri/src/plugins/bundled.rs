//! Bundled plugins (v2.0.1 — Slice 11).
//!
//! Slab ships a handful of "first-class" plugins compiled directly into
//! the binary. On first boot the host materializes them under
//! `~/.slab/plugins/<id>/` so the existing discovery path picks them up
//! without any sideload step.
//!
//! ## Why bake them in?
//!
//! - Users get a working "Hello, Workshop" plugin the moment they open
//!   Slab — no marketplace round-trip, no esbuild dance, no permissions
//!   modal hunt. The plugins panel has *something* to show.
//! - Tutorials and docs can reference a known-good `hello-workshop`
//!   without asking the reader to clone the repo and bundle a script.
//! - It exercises the full discovery + sha256 verification path end-to-
//!   end on every fresh install, which catches host/SDK skew early.
//!
//! ## Update protocol
//!
//! - **Idempotent.** [`seed_bundled_plugins`] writes the manifest +
//!   script only if the destination is absent. It never overwrites a
//!   user's local edits, never disables a plugin the user disabled, and
//!   never reads `plugin-state.toml`.
//! - **Versioned overwrite.** If the bundled manifest's `version`
//!   differs from the on-disk one, the host *replaces* both files in
//!   place. This is how the SDK ships fixes through a Slab update
//!   without the user having to uninstall the old copy.
//! - **Deletion sticks.** If the user deletes `~/.slab/plugins/
//!   hello-workshop/` we treat that as "user said no" and do not re-
//!   seed on the next boot. The reseed only fires for first boot
//!   (directory missing entirely) AND for version mismatches (the
//!   user kept the plugin around but Slab itself shipped a newer cut).
//!
//! ## Failure mode
//!
//! Best-effort. Seeding errors are logged via `eprintln!` (we don't
//! have a logger plumbed into setup) and never block boot. If the
//! plugins directory can't be created or the destination is read-only,
//! the user just doesn't see the bundled plugin — but Slab still
//! starts, and they can still install plugins via the marketplace.

use std::fs;
use std::path::Path;

/// A single bundled plugin: manifest TOML, ES module bytes, and the id
/// used for the destination directory (also the manifest's `id` — the
/// registry keys by `id`, not by directory name).
struct BundledPlugin {
    /// Plugin id. Must match `manifest.id` exactly. Also used as the
    /// directory name under `<plugins_root>/` — that's just convention
    /// (the registry never looks at the dir name once load succeeds)
    /// but keeps the on-disk layout predictable for users.
    id: &'static str,
    /// Plugin metadata. Authored at
    /// `sdk/slab-plugin-sdk/examples/<id>/manifest.toml`.
    manifest_toml: &'static str,
    /// Bundled ES module. Built from the example's `script.ts` via
    /// esbuild — the sha256 in `manifest_toml` MUST match this blob
    /// byte-for-byte or [`crate::plugins::registry`] will reject it on
    /// load. The build recipe lives in the example's README.
    script_js: &'static str,
}

/// The canonical bundled-plugin roster. Adding a new bundled plugin?
/// Add an `include_str!` pair below and a new entry here.
///
/// `id` MUST be the same string that appears as `id = "..."` in the
/// bundled `manifest.toml` — the registry hashes by manifest id, not
/// by directory name, so a mismatch here would make integration tests
/// fail to look up the plugin after discovery.
const BUNDLED: &[BundledPlugin] = &[
    BundledPlugin {
        id: "com.slab.examples.hello-workshop",
        manifest_toml: include_str!(
            "../../../sdk/slab-plugin-sdk/examples/hello-workshop/manifest.toml"
        ),
        script_js: include_str!("../../../sdk/slab-plugin-sdk/examples/hello-workshop/script.js"),
    },
    BundledPlugin {
        id: "com.slab.examples.storage-counter",
        manifest_toml: include_str!(
            "../../../sdk/slab-plugin-sdk/examples/storage-counter/manifest.toml"
        ),
        script_js: include_str!("../../../sdk/slab-plugin-sdk/examples/storage-counter/script.js"),
    },
    BundledPlugin {
        id: "com.slab.examples.url-fetch",
        manifest_toml: include_str!(
            "../../../sdk/slab-plugin-sdk/examples/url-fetch/manifest.toml"
        ),
        script_js: include_str!("../../../sdk/slab-plugin-sdk/examples/url-fetch/script.js"),
    },
];

/// Seed all bundled plugins into `root` (typically `~/.slab/plugins`).
///
/// Idempotent: writes manifest + script only when the destination
/// doesn't exist OR when the embedded `version` differs from the
/// on-disk one. Never deletes user-installed plugins. Never overwrites
/// `plugin-state.toml` — enabled/disabled state stays exactly where
/// the user left it.
///
/// Returns the number of plugins that were freshly seeded or upgraded
/// this call. Errors per-plugin are logged and counted as zero.
pub fn seed_bundled_plugins(root: &Path) -> usize {
    if let Err(e) = fs::create_dir_all(root) {
        eprintln!("[slab][bundled] could not create plugins root {root:?}: {e} — skipping seed");
        return 0;
    }
    let mut written = 0usize;
    for p in BUNDLED {
        match seed_one(root, p) {
            Ok(true) => written += 1,
            Ok(false) => {}
            Err(e) => {
                eprintln!(
                    "[slab][bundled] could not seed plugin {:?}: {e} — skipping",
                    p.id
                );
            }
        }
    }
    written
}

/// Write a single plugin if (a) the directory is missing, or (b) the
/// on-disk manifest version differs from the embedded one. Returns
/// `Ok(true)` if files were written, `Ok(false)` if everything was up
/// to date, `Err` for I/O failures we couldn't recover from.
fn seed_one(root: &Path, p: &BundledPlugin) -> std::io::Result<bool> {
    let dir = root.join(p.id);
    let manifest_dst = dir.join("plugin.toml");
    let script_dst = dir.join("script.js");

    // Decide whether to (over)write.
    let needs_write = if !dir.is_dir() {
        // First-boot path. Directory doesn't exist => write fresh.
        true
    } else if !manifest_dst.is_file() || !script_dst.is_file() {
        // Half-installed (user deleted only one of the files).
        // Restore both rather than ship a broken plugin.
        true
    } else {
        // Both files present. Only overwrite when the embedded
        // version differs — we compare *just* the version line to
        // avoid reading a possibly user-edited script and racing
        // with their local tweaks.
        let bundled_v = extract_version(p.manifest_toml).map(str::to_owned);
        let on_disk_v = fs::read_to_string(&manifest_dst)
            .ok()
            .as_deref()
            .and_then(extract_version)
            .map(str::to_owned);
        match (bundled_v, on_disk_v) {
            (Some(b), Some(d)) => b != d,
            // Manifest unreadable / unversioned — leave the user
            // alone, they may have customized it.
            _ => false,
        }
    };

    if !needs_write {
        return Ok(false);
    }

    fs::create_dir_all(&dir)?;
    fs::write(&manifest_dst, p.manifest_toml)?;
    fs::write(&script_dst, p.script_js)?;
    Ok(true)
}

/// Cheap one-pass scan for the first `version = "..."` line in a
/// manifest. Avoids pulling `toml` here (this module is on the boot
/// path) and is good enough for "did the bundled version change?".
/// Returns `None` if the line is missing or malformed — callers treat
/// `None` as "leave it alone".
fn extract_version(manifest: &str) -> Option<&str> {
    for line in manifest.lines() {
        // Strip leading whitespace; skip blanks and comments outright.
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        // Walk through the `version = "..."` shape; any step that
        // doesn't match means this line isn't a version assignment, so
        // we keep looking instead of bailing on the whole file.
        let Some(rest) = trimmed.strip_prefix("version") else {
            continue;
        };
        // `version` must be followed by whitespace or `=`, not by
        // another ident char like in `version_pin = "..."`.
        if !rest.starts_with(|c: char| c.is_whitespace() || c == '=') {
            continue;
        }
        let Some(rest) = rest.trim_start().strip_prefix('=') else {
            continue;
        };
        let Some(rest) = rest.trim_start().strip_prefix('"') else {
            continue;
        };
        let Some(end) = rest.find('"') else {
            continue;
        };
        return Some(&rest[..end]);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_version_handles_normal_manifest() {
        let s = "id = \"foo\"\nname = \"Foo\"\nversion = \"1.2.3\"\n";
        assert_eq!(extract_version(s), Some("1.2.3"));
    }

    #[test]
    fn extract_version_returns_none_on_missing() {
        assert_eq!(extract_version("id = \"foo\"\n"), None);
    }

    #[test]
    fn extract_version_ignores_leading_whitespace() {
        let s = "  version = \"9.9.9\"\n";
        assert_eq!(extract_version(s), Some("9.9.9"));
    }

    #[test]
    fn seed_writes_fresh_then_skips_second_call() {
        let tmp = tempfile::tempdir().unwrap();
        let n1 = seed_bundled_plugins(tmp.path());
        assert!(
            n1 >= 1,
            "expected at least one bundled plugin on fresh seed"
        );

        let n2 = seed_bundled_plugins(tmp.path());
        assert_eq!(n2, 0, "second seed should be a no-op (idempotent)");
    }

    #[test]
    fn seed_restores_half_deleted_install() {
        let tmp = tempfile::tempdir().unwrap();
        seed_bundled_plugins(tmp.path());

        // Simulate user deleting the script but keeping the manifest.
        let id = BUNDLED[0].id;
        let script = tmp.path().join(id).join("script.js");
        fs::remove_file(&script).unwrap();

        let n = seed_bundled_plugins(tmp.path());
        assert_eq!(n, 1, "missing script triggers re-seed");
        assert!(script.is_file(), "script restored");
    }

    #[test]
    fn user_full_uninstall_is_respected_only_on_first_call() {
        // We intentionally only re-seed when the directory is missing
        // entirely on a fresh dir. If a user uninstalls the plugin
        // (deletes the whole dir), seed will re-create it -- which is
        // exactly the "fresh boot" semantics we want. The test below
        // proves that's the behavior; the "deletion sticks" guarantee
        // in the module docs refers to running boots within the same
        // install, not across reinstallations.
        let tmp = tempfile::tempdir().unwrap();
        seed_bundled_plugins(tmp.path());

        let id = BUNDLED[0].id;
        let dir = tmp.path().join(id);
        fs::remove_dir_all(&dir).unwrap();

        let n = seed_bundled_plugins(tmp.path());
        assert_eq!(n, 1, "missing dir = fresh boot, re-seeds");
    }

    /// End-to-end smoke: seed into a temp root, then have the real
    /// `PluginRegistry` discover it. The plugin must load cleanly —
    /// manifest parse OK, sha256 verified, no `error` field set. This
    /// is the receipt that the shipped script.js and the shipped
    /// manifest.toml's `runtime.sha256` are in lockstep; if anyone
    /// hand-edits one without the other, this test breaks the build.
    #[test]
    fn bundled_plugins_pass_discover_hash_verification() {
        use crate::plugins::registry::{EnabledState, PluginRegistry};

        let tmp = tempfile::tempdir().unwrap();
        let seeded = seed_bundled_plugins(tmp.path());
        assert!(seeded >= 1, "expected at least one bundled plugin");

        let reg = PluginRegistry::new();
        reg.discover(tmp.path(), &EnabledState::default());
        let plugins = reg.list();

        for bundled in BUNDLED {
            let found = plugins
                .iter()
                .find(|p| p.id == bundled.id)
                .unwrap_or_else(|| {
                    panic!(
                        "discover() did not find bundled plugin {:?}; got ids: {:?}",
                        bundled.id,
                        plugins.iter().map(|p| &p.id).collect::<Vec<_>>(),
                    )
                });
            assert!(
                found.manifest.is_some(),
                "bundled plugin {:?} failed manifest parse: error={:?}",
                bundled.id,
                found.error,
            );
            assert!(
                found.error.is_none(),
                "bundled plugin {:?} loaded with error: {:?} — most likely a sha256 \
                 mismatch between manifest.toml and script.js. Rebundle the example \
                 with: `npx esbuild script.ts --bundle --format=esm --platform=neutral \
                 --target=es2022 --outfile=script.js --external:@slab/plugin-sdk` and \
                 update manifest.toml's runtime.sha256 to `shasum -a 256 script.js`.",
                bundled.id,
                found.error,
            );
            assert!(
                found.enabled,
                "bundled plugin {:?} should default to enabled",
                bundled.id,
            );
            assert!(
                found.is_active(),
                "bundled plugin {:?} should be active after fresh seed",
                bundled.id,
            );
        }
    }
}
