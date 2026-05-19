//! Workshop (v2.0.0 Slice 3) — per-plugin capability grant store.
//!
//! ## Threat model
//!
//! Plugin manifests declare an *upper bound* of capabilities they
//! intend to use (see [`Manifest::runtime`] + [`Capabilities`]). The
//! user has to *grant* each capability before the plugin can actually
//! use it. This module owns the persistence layer for those grants
//! and the runtime-side enforcement check.
//!
//! Defaults are *deny*: a plugin that has never been granted a
//! capability cannot use it, even if its manifest declares the
//! upper bound. This is the v1.6.x AI-provider parked-item finally
//! getting its honest answer — users decide what code does what.
//!
//! ## File format
//!
//! Persisted as TOML at `~/.slab/plugin-grants.toml`:
//!
//! ```toml
//! ["com.example.foo"]
//! fs = "read"
//! net = "specific"
//! net_allow_hosts = ["api.openai.com"]
//! ui = "panel"
//! beacon = "tool-provider"
//! fs_allow_paths = ["~/Documents/**"]
//!
//! ["com.example.bar"]
//! fs = "none"
//! ```
//!
//! Plugins absent from the file have no grants (full default-deny).
//!
//! ## Out of scope for this slice
//!
//! The capability-grant *prompt UI* (Cabinet modal) lives in a later
//! frontend slice. This module exposes:
//! - The persistence types ([`PluginGrants`], [`GrantStore`])
//! - A `read_grants`/`write_grants` pair
//! - The [`enforce`] helper that returns whether a plugin may invoke
//!   a given capability (used by the runtime host shim in Slice 4+)

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::manifest::{BeaconCap, Capabilities, FsCap, NetCap, UiCap};

/// One plugin's granted capabilities. Identical shape to
/// [`Capabilities`] — declared upper bound vs granted reality have
/// the same vocabulary, just different semantics.
///
/// `Default` is "all none" — no grant means no access.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PluginGrants {
    #[serde(default)]
    pub fs: FsCap,
    #[serde(default)]
    pub net: NetCap,
    #[serde(default)]
    pub ui: UiCap,
    #[serde(default)]
    pub beacon: BeaconCap,
    #[serde(default)]
    pub net_allow_hosts: Vec<String>,
    #[serde(default)]
    pub fs_allow_paths: Vec<String>,
}

impl PluginGrants {
    /// Build a `PluginGrants` that matches `caps` exactly — i.e. the
    /// user granted everything the plugin asked for. Useful as the
    /// default action in test fixtures and the "Grant all" button.
    pub fn grant_all(caps: &Capabilities) -> Self {
        Self {
            fs: caps.fs,
            net: caps.net,
            ui: caps.ui,
            beacon: caps.beacon,
            net_allow_hosts: caps.net_allow_hosts.clone(),
            fs_allow_paths: caps.fs_allow_paths.clone(),
        }
    }

    /// Build a `PluginGrants` that grants nothing — the result of the
    /// user clicking "Deny" on the prompt. Equivalent to default but
    /// explicit at the call site.
    pub fn deny_all() -> Self {
        Self::default()
    }

    /// Does the user grant *cover* the manifest declaration? Used to
    /// decide whether to re-prompt when a plugin updates and asks for
    /// a broader capability than was previously granted.
    ///
    /// Returns `true` when every declared capability in `caps` is at
    /// most as wide as the grant. If the plugin escalates, returns
    /// `false` and the caller must reprompt.
    pub fn covers(&self, caps: &Capabilities) -> bool {
        fs_covers(self.fs, caps.fs)
            && net_covers(self.net, caps.net)
            && ui_covers(self.ui, caps.ui)
            && beacon_covers(self.beacon, caps.beacon)
            && (caps.net != NetCap::Specific
                || caps
                    .net_allow_hosts
                    .iter()
                    .all(|h| self.net_allow_hosts.contains(h)))
            && (caps.fs == FsCap::None
                || caps
                    .fs_allow_paths
                    .iter()
                    .all(|p| self.fs_allow_paths.contains(p)))
    }
}

fn fs_covers(grant: FsCap, want: FsCap) -> bool {
    fs_rank(grant) >= fs_rank(want)
}

fn fs_rank(c: FsCap) -> u8 {
    match c {
        FsCap::None => 0,
        FsCap::Read => 1,
        FsCap::ReadWrite => 2,
    }
}

fn net_covers(grant: NetCap, want: NetCap) -> bool {
    net_rank(grant) >= net_rank(want)
}

fn net_rank(c: NetCap) -> u8 {
    match c {
        NetCap::None => 0,
        NetCap::Specific => 1,
        NetCap::Any => 2,
    }
}

fn ui_covers(grant: UiCap, want: UiCap) -> bool {
    matches!(
        (grant, want),
        (_, UiCap::None)
            | (UiCap::Both, _)
            | (UiCap::Panel, UiCap::Panel)
            | (UiCap::Tool, UiCap::Tool)
    )
}

fn beacon_covers(grant: BeaconCap, want: BeaconCap) -> bool {
    matches!(
        (grant, want),
        (_, BeaconCap::None)
            | (BeaconCap::Both, _)
            | (BeaconCap::ToolProvider, BeaconCap::ToolProvider)
            | (BeaconCap::AiProvider, BeaconCap::AiProvider)
    )
}

/// Persisted map of `plugin_id -> PluginGrants`. Stored at
/// `~/.slab/plugin-grants.toml`.
///
/// We deliberately use a *flattened* outer map so the on-disk format
/// reads as `[plugin.id]` per plugin — easy to hand-edit when
/// debugging, and easy for users to audit.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct GrantStore {
    #[serde(flatten)]
    pub grants: HashMap<String, PluginGrants>,
}

impl GrantStore {
    /// Lookup grants for a plugin. Returns the default ("deny all")
    /// when the plugin has no entry. Callers should pattern on the
    /// returned ref against [`PluginGrants::default()`] if they want
    /// to know whether grants are explicit or implicit.
    pub fn get(&self, plugin_id: &str) -> PluginGrants {
        self.grants.get(plugin_id).cloned().unwrap_or_default()
    }

    /// Has the user ever made an explicit grant decision for this
    /// plugin? `false` means we should show the prompt.
    pub fn has_decision(&self, plugin_id: &str) -> bool {
        self.grants.contains_key(plugin_id)
    }

    /// Record a grant decision. Overwrites any previous decision.
    pub fn set(&mut self, plugin_id: impl Into<String>, grants: PluginGrants) {
        self.grants.insert(plugin_id.into(), grants);
    }

    /// Remove a plugin's grants (e.g. on uninstall). No-op when the
    /// plugin has no entry.
    pub fn remove(&mut self, plugin_id: &str) {
        self.grants.remove(plugin_id);
    }
}

/// Default location for the grant file. `None` when `HOME` is unset.
pub fn default_grants_path() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".slab").join("plugin-grants.toml"))
}

/// Load the grant store from disk. Missing file is normal on first
/// boot and returns an empty store rather than an error.
pub fn read_grants(path: &Path) -> GrantStore {
    match fs::read_to_string(path) {
        Ok(src) => toml::from_str(&src).unwrap_or_default(),
        Err(_) => GrantStore::default(),
    }
}

/// Persist the grant store to disk. Creates parent dirs as needed.
pub fn write_grants(path: &Path, store: &GrantStore) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let src = toml::to_string_pretty(store).unwrap_or_default();
    fs::write(path, src)
}

/// Reasons [`enforce`] may decline a capability call. Mapped 1:1 to
/// user-facing error messages by the runtime host shim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DenyReason {
    /// Plugin manifest doesn't declare this capability — even with a
    /// grant, the runtime refuses (manifest is contract; grant must
    /// be a subset of contract).
    NotDeclared,
    /// User has not granted (or has explicitly denied) this capability.
    NotGranted,
    /// User granted a narrower variant than the request needs
    /// (e.g. plugin declared `fs = "read-write"` and the user only
    /// granted `fs = "read"`, plugin tried to write).
    GrantTooNarrow,
    /// `net = "specific"` was granted but the requested host isn't in
    /// `net_allow_hosts`.
    HostNotAllowed,
}

/// The thing a plugin is trying to do. The runtime host shim builds
/// one of these per capability-gated call (Slice 4+ work).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapabilityRequest<'a> {
    FsRead,
    FsWrite,
    NetFetch { host: &'a str },
    UiRegisterPanel,
    UiRegisterTool,
    BeaconRegisterTool,
    BeaconRegisterAiProvider,
}

/// Decide whether a [`CapabilityRequest`] is permitted given the
/// plugin's declared upper bound and the user's actual grant.
///
/// **Both must permit** — declared but not granted = NotGranted;
/// granted but not declared = NotDeclared.
pub fn enforce(
    declared: &Capabilities,
    granted: &PluginGrants,
    req: &CapabilityRequest<'_>,
) -> Result<(), DenyReason> {
    match req {
        CapabilityRequest::FsRead => {
            if declared.fs == FsCap::None {
                return Err(DenyReason::NotDeclared);
            }
            match granted.fs {
                FsCap::None => Err(DenyReason::NotGranted),
                FsCap::Read | FsCap::ReadWrite => Ok(()),
            }
        }
        CapabilityRequest::FsWrite => {
            if declared.fs != FsCap::ReadWrite {
                return Err(DenyReason::NotDeclared);
            }
            match granted.fs {
                FsCap::None => Err(DenyReason::NotGranted),
                FsCap::Read => Err(DenyReason::GrantTooNarrow),
                FsCap::ReadWrite => Ok(()),
            }
        }
        CapabilityRequest::NetFetch { host } => {
            if declared.net == NetCap::None {
                return Err(DenyReason::NotDeclared);
            }
            match granted.net {
                NetCap::None => Err(DenyReason::NotGranted),
                NetCap::Any => Ok(()),
                NetCap::Specific => {
                    if granted.net_allow_hosts.iter().any(|h| h == host) {
                        Ok(())
                    } else {
                        Err(DenyReason::HostNotAllowed)
                    }
                }
            }
        }
        CapabilityRequest::UiRegisterPanel => {
            if !matches!(declared.ui, UiCap::Panel | UiCap::Both) {
                return Err(DenyReason::NotDeclared);
            }
            if matches!(granted.ui, UiCap::Panel | UiCap::Both) {
                Ok(())
            } else {
                Err(DenyReason::NotGranted)
            }
        }
        CapabilityRequest::UiRegisterTool => {
            if !matches!(declared.ui, UiCap::Tool | UiCap::Both) {
                return Err(DenyReason::NotDeclared);
            }
            if matches!(granted.ui, UiCap::Tool | UiCap::Both) {
                Ok(())
            } else {
                Err(DenyReason::NotGranted)
            }
        }
        CapabilityRequest::BeaconRegisterTool => {
            if !matches!(declared.beacon, BeaconCap::ToolProvider | BeaconCap::Both) {
                return Err(DenyReason::NotDeclared);
            }
            if matches!(granted.beacon, BeaconCap::ToolProvider | BeaconCap::Both) {
                Ok(())
            } else {
                Err(DenyReason::NotGranted)
            }
        }
        CapabilityRequest::BeaconRegisterAiProvider => {
            if !matches!(declared.beacon, BeaconCap::AiProvider | BeaconCap::Both) {
                return Err(DenyReason::NotDeclared);
            }
            if matches!(granted.beacon, BeaconCap::AiProvider | BeaconCap::Both) {
                Ok(())
            } else {
                Err(DenyReason::NotGranted)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn caps(fs: FsCap, net: NetCap, ui: UiCap, beacon: BeaconCap) -> Capabilities {
        Capabilities {
            fs,
            net,
            ui,
            beacon,
            net_allow_hosts: vec![],
            fs_allow_paths: vec![],
        }
    }

    // ---- PluginGrants behaviour ----

    #[test]
    fn deny_all_grants_nothing() {
        let g = PluginGrants::deny_all();
        assert_eq!(g.fs, FsCap::None);
        assert_eq!(g.net, NetCap::None);
        assert_eq!(g.ui, UiCap::None);
        assert_eq!(g.beacon, BeaconCap::None);
    }

    #[test]
    fn grant_all_mirrors_capabilities() {
        let c = Capabilities {
            fs: FsCap::ReadWrite,
            net: NetCap::Specific,
            ui: UiCap::Both,
            beacon: BeaconCap::AiProvider,
            net_allow_hosts: vec!["api.openai.com".into()],
            fs_allow_paths: vec!["~/Documents/**".into()],
        };
        let g = PluginGrants::grant_all(&c);
        assert_eq!(g.fs, c.fs);
        assert_eq!(g.net, c.net);
        assert_eq!(g.ui, c.ui);
        assert_eq!(g.beacon, c.beacon);
        assert_eq!(g.net_allow_hosts, c.net_allow_hosts);
        assert_eq!(g.fs_allow_paths, c.fs_allow_paths);
    }

    #[test]
    fn covers_returns_true_when_grant_matches_declaration() {
        let c = caps(FsCap::Read, NetCap::None, UiCap::Panel, BeaconCap::None);
        let g = PluginGrants::grant_all(&c);
        assert!(g.covers(&c));
    }

    #[test]
    fn covers_returns_false_when_plugin_escalates() {
        let old = caps(FsCap::Read, NetCap::None, UiCap::None, BeaconCap::None);
        let g = PluginGrants::grant_all(&old);
        // plugin now wants read-write — grant should NOT cover
        let new = caps(FsCap::ReadWrite, NetCap::None, UiCap::None, BeaconCap::None);
        assert!(!g.covers(&new));
    }

    #[test]
    fn covers_checks_each_axis_independently() {
        let g = PluginGrants {
            fs: FsCap::ReadWrite,
            net: NetCap::Any,
            ui: UiCap::Both,
            beacon: BeaconCap::Both,
            net_allow_hosts: vec![],
            fs_allow_paths: vec![],
        };
        // Plugin wants nothing → covered.
        assert!(g.covers(&caps(
            FsCap::None,
            NetCap::None,
            UiCap::None,
            BeaconCap::None
        )));
        // Plugin wants everything that fits → covered.
        assert!(g.covers(&caps(
            FsCap::Read,
            NetCap::Any,
            UiCap::Panel,
            BeaconCap::ToolProvider
        )));
    }

    #[test]
    fn covers_requires_net_allow_hosts_to_be_subset() {
        let c = Capabilities {
            fs: FsCap::None,
            net: NetCap::Specific,
            ui: UiCap::None,
            beacon: BeaconCap::None,
            net_allow_hosts: vec!["api.openai.com".into(), "huggingface.co".into()],
            fs_allow_paths: vec![],
        };
        let mut g = PluginGrants::grant_all(&c);
        assert!(g.covers(&c));
        // Drop huggingface from grants → plugin now requires more.
        g.net_allow_hosts.retain(|h| h != "huggingface.co");
        assert!(!g.covers(&c));
    }

    // ---- GrantStore behaviour ----

    #[test]
    fn store_returns_default_for_unknown_plugin() {
        let s = GrantStore::default();
        assert_eq!(s.get("never.heard.of.you"), PluginGrants::default());
        assert!(!s.has_decision("never.heard.of.you"));
    }

    #[test]
    fn store_remembers_explicit_decision() {
        let mut s = GrantStore::default();
        let g = PluginGrants {
            fs: FsCap::Read,
            ..PluginGrants::default()
        };
        s.set("com.x.y", g.clone());
        assert!(s.has_decision("com.x.y"));
        assert_eq!(s.get("com.x.y"), g);
    }

    #[test]
    fn store_remove_clears_decision() {
        let mut s = GrantStore::default();
        s.set("com.x.y", PluginGrants::deny_all());
        s.remove("com.x.y");
        assert!(!s.has_decision("com.x.y"));
    }

    #[test]
    fn read_grants_returns_default_when_file_missing() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("nope.toml");
        assert_eq!(read_grants(&path), GrantStore::default());
    }

    #[test]
    fn read_write_grants_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("grants.toml");
        let mut s = GrantStore::default();
        s.set(
            "com.x.y",
            PluginGrants {
                fs: FsCap::Read,
                net: NetCap::Specific,
                ui: UiCap::Panel,
                beacon: BeaconCap::ToolProvider,
                net_allow_hosts: vec!["api.openai.com".into()],
                fs_allow_paths: vec!["~/Documents/**".into()],
            },
        );
        write_grants(&path, &s).unwrap();
        let loaded = read_grants(&path);
        assert_eq!(loaded, s);
    }

    #[test]
    fn read_grants_treats_garbage_file_as_empty() {
        // Better behaviour than panic: a corrupt file silently
        // reverts to "no grants" rather than locking the user out.
        // The user will re-grant on next prompt.
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("grants.toml");
        fs::write(&path, "this is not toml ===").unwrap();
        assert_eq!(read_grants(&path), GrantStore::default());
    }

    // ---- enforce() — the runtime gate ----

    #[test]
    fn enforce_denies_when_capability_not_declared() {
        let declared = caps(FsCap::None, NetCap::None, UiCap::None, BeaconCap::None);
        let granted = PluginGrants {
            fs: FsCap::ReadWrite, // user somehow granted more than declared
            ..PluginGrants::default()
        };
        let err = enforce(&declared, &granted, &CapabilityRequest::FsRead).unwrap_err();
        assert_eq!(err, DenyReason::NotDeclared);
    }

    #[test]
    fn enforce_denies_when_capability_not_granted() {
        let declared = caps(FsCap::Read, NetCap::None, UiCap::None, BeaconCap::None);
        let granted = PluginGrants::deny_all();
        let err = enforce(&declared, &granted, &CapabilityRequest::FsRead).unwrap_err();
        assert_eq!(err, DenyReason::NotGranted);
    }

    #[test]
    fn enforce_allows_when_both_declared_and_granted() {
        let declared = caps(FsCap::Read, NetCap::None, UiCap::None, BeaconCap::None);
        let granted = PluginGrants::grant_all(&declared);
        enforce(&declared, &granted, &CapabilityRequest::FsRead).unwrap();
    }

    #[test]
    fn enforce_fs_write_needs_read_write_grant() {
        let declared = caps(FsCap::ReadWrite, NetCap::None, UiCap::None, BeaconCap::None);
        // User granted only Read.
        let granted = PluginGrants {
            fs: FsCap::Read,
            ..PluginGrants::default()
        };
        let err = enforce(&declared, &granted, &CapabilityRequest::FsWrite).unwrap_err();
        assert_eq!(err, DenyReason::GrantTooNarrow);

        // Bumping grant to ReadWrite allows the write.
        let granted = PluginGrants {
            fs: FsCap::ReadWrite,
            ..PluginGrants::default()
        };
        enforce(&declared, &granted, &CapabilityRequest::FsWrite).unwrap();
    }

    #[test]
    fn enforce_net_fetch_respects_allow_list() {
        let declared = Capabilities {
            fs: FsCap::None,
            net: NetCap::Specific,
            ui: UiCap::None,
            beacon: BeaconCap::None,
            net_allow_hosts: vec!["api.openai.com".into()],
            fs_allow_paths: vec![],
        };
        let granted = PluginGrants {
            net: NetCap::Specific,
            net_allow_hosts: vec!["api.openai.com".into()],
            ..PluginGrants::default()
        };
        // Allowed host
        enforce(
            &declared,
            &granted,
            &CapabilityRequest::NetFetch {
                host: "api.openai.com",
            },
        )
        .unwrap();
        // Blocked host
        let err = enforce(
            &declared,
            &granted,
            &CapabilityRequest::NetFetch {
                host: "evil.example.com",
            },
        )
        .unwrap_err();
        assert_eq!(err, DenyReason::HostNotAllowed);
    }

    #[test]
    fn enforce_net_fetch_any_allows_all_hosts() {
        let declared = caps(FsCap::None, NetCap::Any, UiCap::None, BeaconCap::None);
        let granted = PluginGrants {
            net: NetCap::Any,
            ..PluginGrants::default()
        };
        enforce(
            &declared,
            &granted,
            &CapabilityRequest::NetFetch {
                host: "anything.example",
            },
        )
        .unwrap();
    }

    #[test]
    fn enforce_ui_panel_vs_tool_distinct() {
        let declared = caps(FsCap::None, NetCap::None, UiCap::Panel, BeaconCap::None);
        let granted = PluginGrants {
            ui: UiCap::Panel,
            ..PluginGrants::default()
        };
        // Panel granted — register panel works.
        enforce(&declared, &granted, &CapabilityRequest::UiRegisterPanel).unwrap();
        // Manifest never declared Tool — even if user "granted" Tool,
        // declaring-vs-using is the contract, NotDeclared wins.
        let err = enforce(&declared, &granted, &CapabilityRequest::UiRegisterTool).unwrap_err();
        assert_eq!(err, DenyReason::NotDeclared);
    }

    #[test]
    fn enforce_beacon_ai_provider_distinct_from_tool() {
        let declared = caps(
            FsCap::None,
            NetCap::None,
            UiCap::None,
            BeaconCap::AiProvider,
        );
        let granted = PluginGrants {
            beacon: BeaconCap::AiProvider,
            ..PluginGrants::default()
        };
        enforce(
            &declared,
            &granted,
            &CapabilityRequest::BeaconRegisterAiProvider,
        )
        .unwrap();
        let err = enforce(&declared, &granted, &CapabilityRequest::BeaconRegisterTool).unwrap_err();
        assert_eq!(err, DenyReason::NotDeclared);
    }

    #[test]
    fn enforce_beacon_both_satisfies_either_request() {
        let declared = caps(FsCap::None, NetCap::None, UiCap::None, BeaconCap::Both);
        let granted = PluginGrants {
            beacon: BeaconCap::Both,
            ..PluginGrants::default()
        };
        enforce(&declared, &granted, &CapabilityRequest::BeaconRegisterTool).unwrap();
        enforce(
            &declared,
            &granted,
            &CapabilityRequest::BeaconRegisterAiProvider,
        )
        .unwrap();
    }
}
