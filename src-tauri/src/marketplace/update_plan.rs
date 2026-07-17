//! Marketplace update planning — pure data.
//!
//! Given the currently-installed plugins (carrying their on-disk
//! versions via [`InstalledSnapshot`]) and a freshly-fetched
//! marketplace [`Index`], compute the set of plugins for which the
//! index has a strictly newer version. The result is a deterministic
//! [`UpdatePlan`] that the bulk-update command [`crate::marketplace`]
//! / the "Updates available" banner UI both read from.
//!
//! This module is pure — no I/O, no networking, no clock. The
//! semver-aware comparison reuses the same logic the v1.4.0 Browse
//! tab uses for the "↑ vX.Y.Z — update available" badge so a plugin
//! showing the badge in the UI is the EXACT same plugin that lands in
//! the bulk-update plan. One source of truth for "is there an
//! update".
//!
//! ## Why a dedicated planner (not just a UI derived value)
//!
//! The per-card badge in [`super::index::IndexEntry`] is derived in
//! the Svelte layer today. A backend planner ports that derivation
//! into Rust so:
//!
//! 1. The bulk-update command can resolve targets server-side without
//!    trusting a client-supplied id list (the client could pass a
//!    stale list; the server re-plans on every call).
//! 2. The plan is unit-testable in pure Rust — semver edge cases
//!    (pre-release ordering, missing components, equal versions) are
//!    pinned in tests rather than implicit in Svelte derived chains.
//! 3. A future CLI / scriptable surface ("slab plugins update") can
//!    reuse the same plan computation without dragging in the UI
//!    layer.

use serde::Serialize;

use super::index::IndexEntry;

/// Snapshot of one installed plugin's identity, with enough info to
/// compute whether the index has a newer version. Mirrors the slim
/// subset of `crate::plugins::registry::Plugin` the planner needs;
/// the planner takes a slim type so its unit tests don't need to
/// mock the whole PluginRegistry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledPlugin {
    /// Plugin id (reverse-DNS, e.g. `com.example.hello`).
    pub id: String,
    /// On-disk version string (from the manifest). Compared
    /// semver-aware against the index version.
    pub version: String,
}

/// One planned update — the installed plugin + the index entry that
/// supersedes it. The full entry is carried so downstream consumers
/// (banner UI, install command) can render the new name / installs /
/// size without a second lookup.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UpdateTarget {
    /// Plugin id (same as `installed_version`'s id and `entry.id`).
    pub id: String,
    /// Currently-installed version on disk.
    pub installed_version: String,
    /// Newer version available in the marketplace index.
    pub available_version: String,
    /// Sum of bytes that will be pulled if this target updates.
    /// Convenience accessor — equals `entry.size_bytes`.
    pub size_bytes: u64,
    /// The full index entry. The bulk-update command feeds this
    /// straight into [`super::install::install_from_entry`].
    pub entry: IndexEntry,
}

/// A deterministic plan computed from installed + index state.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UpdatePlan {
    /// One target per installed plugin with an available update,
    /// sorted by `id` ascending so the plan is stable across
    /// re-computations (UI lists / test assertions don't need to
    /// sort defensively).
    pub targets: Vec<UpdateTarget>,
    /// Sum of `size_bytes` across `targets`. Useful for the banner
    /// "Update all (3 plugins · 4.2 MB)" affordance.
    pub total_bytes: u64,
}

impl UpdatePlan {
    /// Number of planned updates. Convenience for the banner header.
    pub fn count(&self) -> usize {
        self.targets.len()
    }

    /// True when there's nothing to update. The banner UI hides
    /// itself when this is the case.
    pub fn is_empty(&self) -> bool {
        self.targets.is_empty()
    }

    /// Return the slice of target ids in plan order. The UI uses
    /// this to render a compact preview list ("Acme · Beta · Gamma");
    /// the bulk-update command uses it as the default id set when
    /// the user clicked "Update all" rather than picking individual
    /// rows.
    pub fn target_ids(&self) -> Vec<String> {
        self.targets.iter().map(|t| t.id.clone()).collect()
    }
}

/// Build a plan by intersecting `installed` with `index`, keeping
/// only the rows where the index entry is strictly newer.
///
/// "Strictly newer" uses the same semver-aware comparison the v1.4.0
/// Browse tab's "update available" badge uses (see
/// [`semver_compare`]). Equal-version pairs are NOT included (no
/// re-install affordance — that's a separate surface).
///
/// Duplicate-id rows in `installed` are tolerated: the first
/// occurrence wins (matches the registry's natural insertion order;
/// duplicates can't happen in practice because the registry keys
/// plugins by id, but the planner is defensive). Duplicate-id rows
/// in `index` are also tolerated: the first occurrence wins for the
/// same reason.
pub fn plan_updates(installed: &[InstalledPlugin], index_entries: &[IndexEntry]) -> UpdatePlan {
    let mut targets: Vec<UpdateTarget> = Vec::new();
    let mut seen_installed: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for inst in installed {
        if !seen_installed.insert(inst.id.as_str()) {
            continue;
        }
        let Some(entry) = index_entries.iter().find(|e| e.id == inst.id) else {
            continue;
        };
        if semver_compare(&entry.version, &inst.version) > 0 {
            targets.push(UpdateTarget {
                id: inst.id.clone(),
                installed_version: inst.version.clone(),
                available_version: entry.version.clone(),
                size_bytes: entry.size_bytes,
                entry: entry.clone(),
            });
        }
    }
    targets.sort_by(|a, b| a.id.cmp(&b.id));
    let total_bytes = targets.iter().map(|t| t.size_bytes).sum();
    UpdatePlan {
        targets,
        total_bytes,
    }
}

/// Compare two semver-ish strings. Returns negative when `a < b`,
/// zero when equal, positive when `a > b`.
///
/// Tolerant: missing components count as 0; non-numeric components
/// fall back to lexicographic ordering. Pre-release tags sort lower
/// than the same version without one (so `1.0.0-rc1` < `1.0.0`).
///
/// MUST match the TS `compareSemver` in `src/lib/marketplace.ts`
/// byte-for-byte semantically — the "update available" badge in the
/// Browse tab uses the TS version, the planner uses this Rust
/// version, and a plugin showing the badge MUST land in the plan
/// (and vice versa). Both implementations are direct ports of each
/// other; the test corpus below pins the parity.
pub fn semver_compare(a: &str, b: &str) -> i32 {
    let split = |s: &str| -> (Vec<i64>, Option<String>) {
        if let Some(dash) = s.find('-') {
            let core = &s[..dash];
            let pre = &s[dash + 1..];
            (
                core.split('.')
                    .map(|n| n.parse::<i64>().unwrap_or(0))
                    .collect(),
                Some(pre.to_string()),
            )
        } else {
            (
                s.split('.')
                    .map(|n| n.parse::<i64>().unwrap_or(0))
                    .collect(),
                None,
            )
        }
    };
    let (ac, ap) = split(a);
    let (bc, bp) = split(b);
    let len = ac.len().max(bc.len());
    for i in 0..len {
        let x = ac.get(i).copied().unwrap_or(0);
        let y = bc.get(i).copied().unwrap_or(0);
        if x != y {
            return if x < y { -1 } else { 1 };
        }
    }
    match (ap, bp) {
        (None, None) => 0,
        (None, Some(_)) => 1, // release > prerelease
        (Some(_), None) => -1,
        (Some(ax), Some(bx)) => ax.cmp(&bx) as i32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, version: &str, size: u64) -> IndexEntry {
        IndexEntry {
            id: id.into(),
            name: format!("Plugin {id}"),
            version: version.into(),
            description: "demo".into(),
            author: "test".into(),
            download_url: format!("https://example.test/{id}-{version}.tar.gz"),
            sha256: "0".repeat(64),
            size_bytes: size,
            slab_compat: ">=1.4.0".into(),
            signature: "AAAA".into(),
            categories: Vec::new(),
            tags: Vec::new(),
            screenshots: Vec::new(),
            installs: 0,
        }
    }

    fn installed(id: &str, version: &str) -> InstalledPlugin {
        InstalledPlugin {
            id: id.into(),
            version: version.into(),
        }
    }

    // ─── semver_compare parity with the TS implementation ────────────

    #[test]
    fn semver_compare_basic_ordering() {
        assert!(semver_compare("1.0.0", "2.0.0") < 0);
        assert!(semver_compare("2.0.0", "1.0.0") > 0);
        assert_eq!(semver_compare("1.0.0", "1.0.0"), 0);
    }

    #[test]
    fn semver_compare_minor_patch() {
        assert!(semver_compare("1.2.3", "1.2.4") < 0);
        assert!(semver_compare("1.2.4", "1.2.3") > 0);
        assert!(semver_compare("1.9.0", "1.10.0") < 0);
    }

    #[test]
    fn semver_compare_missing_components_default_to_zero() {
        assert_eq!(semver_compare("1", "1.0.0"), 0);
        assert_eq!(semver_compare("1.2", "1.2.0"), 0);
        assert!(semver_compare("1", "1.0.1") < 0);
    }

    #[test]
    fn semver_compare_non_numeric_components_treated_as_zero() {
        // matches TS parseInt-or-0 fallback
        assert_eq!(semver_compare("a.b.c", "0.0.0"), 0);
        assert!(semver_compare("a.b.c", "0.0.1") < 0);
    }

    #[test]
    fn semver_compare_release_beats_prerelease() {
        assert!(semver_compare("1.0.0-rc1", "1.0.0") < 0);
        assert!(semver_compare("1.0.0", "1.0.0-rc1") > 0);
    }

    #[test]
    fn semver_compare_prerelease_lexicographic() {
        assert!(semver_compare("1.0.0-alpha", "1.0.0-beta") < 0);
        assert!(semver_compare("1.0.0-beta", "1.0.0-alpha") > 0);
        assert_eq!(semver_compare("1.0.0-rc1", "1.0.0-rc1"), 0);
    }

    // ─── plan_updates: the planner itself ────────────────────────────

    #[test]
    fn plan_is_empty_when_no_plugins_installed() {
        let plan = plan_updates(&[], &[entry("a", "1.0.0", 100)]);
        assert!(plan.is_empty());
        assert_eq!(plan.count(), 0);
        assert_eq!(plan.total_bytes, 0);
        assert!(plan.target_ids().is_empty());
    }

    #[test]
    fn plan_is_empty_when_index_is_empty() {
        let plan = plan_updates(&[installed("a", "1.0.0")], &[]);
        assert!(plan.is_empty());
    }

    #[test]
    fn plan_is_empty_when_installed_already_current() {
        let plan = plan_updates(&[installed("a", "1.0.0")], &[entry("a", "1.0.0", 100)]);
        assert!(plan.is_empty());
    }

    #[test]
    fn plan_is_empty_when_installed_ahead_of_index() {
        // User sideloaded a newer build than the index advertises —
        // not an update target.
        let plan = plan_updates(&[installed("a", "2.0.0")], &[entry("a", "1.0.0", 100)]);
        assert!(plan.is_empty());
    }

    #[test]
    fn plan_includes_strict_newer_only() {
        let plan = plan_updates(
            &[installed("a", "1.0.0"), installed("b", "1.0.0")],
            &[entry("a", "1.0.1", 200), entry("b", "1.0.0", 200)],
        );
        assert_eq!(plan.count(), 1);
        assert_eq!(plan.targets[0].id, "a");
        assert_eq!(plan.targets[0].installed_version, "1.0.0");
        assert_eq!(plan.targets[0].available_version, "1.0.1");
        assert_eq!(plan.targets[0].size_bytes, 200);
        assert_eq!(plan.total_bytes, 200);
    }

    #[test]
    fn plan_ignores_index_only_entries_not_installed() {
        // Plugin "c" exists in the index but isn't installed locally —
        // not an update target (the user has to install it first via
        // the Browse tab).
        let plan = plan_updates(
            &[installed("a", "1.0.0")],
            &[entry("a", "1.0.1", 100), entry("c", "5.0.0", 100)],
        );
        assert_eq!(plan.count(), 1);
        assert_eq!(plan.targets[0].id, "a");
    }

    #[test]
    fn plan_sorts_targets_by_id_ascending() {
        let plan = plan_updates(
            &[
                installed("zeta", "1.0.0"),
                installed("alpha", "1.0.0"),
                installed("mike", "1.0.0"),
            ],
            &[
                entry("zeta", "1.0.1", 100),
                entry("alpha", "1.0.1", 100),
                entry("mike", "1.0.1", 100),
            ],
        );
        let ids = plan.target_ids();
        assert_eq!(ids, vec!["alpha", "mike", "zeta"]);
    }

    #[test]
    fn plan_total_bytes_sums_correctly() {
        let plan = plan_updates(
            &[
                installed("a", "1.0.0"),
                installed("b", "1.0.0"),
                installed("c", "1.0.0"),
            ],
            &[
                entry("a", "1.0.1", 100),
                entry("b", "1.0.1", 250),
                entry("c", "1.0.1", 1000),
            ],
        );
        assert_eq!(plan.count(), 3);
        assert_eq!(plan.total_bytes, 1350);
    }

    #[test]
    fn plan_carries_full_index_entry_for_each_target() {
        let plan = plan_updates(&[installed("a", "1.0.0")], &[entry("a", "1.0.1", 200)]);
        assert_eq!(plan.targets[0].entry.id, "a");
        assert_eq!(plan.targets[0].entry.version, "1.0.1");
        assert_eq!(plan.targets[0].entry.name, "Plugin a");
        // download_url roundtrips so the bulk-updater can hand it
        // straight to install_from_entry.
        assert!(plan.targets[0].entry.download_url.contains("a-1.0.1"));
    }

    #[test]
    fn plan_dedupes_installed_duplicates_first_wins() {
        // Pathological: same id twice in the installed list. Registry
        // can't actually produce this, but the planner defends so a
        // future caller mutating the input vec doesn't break it.
        let plan = plan_updates(
            &[installed("a", "1.0.0"), installed("a", "0.5.0")],
            &[entry("a", "1.0.1", 100)],
        );
        assert_eq!(plan.count(), 1);
        // First-wins: installed_version is "1.0.0", not "0.5.0".
        assert_eq!(plan.targets[0].installed_version, "1.0.0");
    }

    #[test]
    fn plan_dedupes_index_duplicates_first_wins() {
        // Pathological: same id twice in the index. find() returns
        // first match — pinning this behaviour.
        let plan = plan_updates(
            &[installed("a", "1.0.0")],
            &[entry("a", "1.0.1", 100), entry("a", "9.9.9", 999)],
        );
        assert_eq!(plan.count(), 1);
        assert_eq!(plan.targets[0].available_version, "1.0.1");
        assert_eq!(plan.targets[0].size_bytes, 100);
    }

    #[test]
    fn plan_handles_prerelease_correctly() {
        // installed=1.0.0-rc1, available=1.0.0 → update
        let plan = plan_updates(&[installed("a", "1.0.0-rc1")], &[entry("a", "1.0.0", 100)]);
        assert_eq!(plan.count(), 1);
        assert_eq!(plan.targets[0].available_version, "1.0.0");

        // installed=1.0.0, available=1.0.0-rc1 → no update (release > rc)
        let plan = plan_updates(&[installed("a", "1.0.0")], &[entry("a", "1.0.0-rc1", 100)]);
        assert!(plan.is_empty());
    }

    #[test]
    fn plan_serializes_as_camel_compatible_json() {
        // Wire smoke: the plan must serialize cleanly so the Tauri
        // command can return it.
        let plan = plan_updates(&[installed("a", "1.0.0")], &[entry("a", "1.0.1", 100)]);
        let json = serde_json::to_string(&plan).unwrap();
        assert!(json.contains("\"targets\""));
        assert!(json.contains("\"total_bytes\""));
        assert!(json.contains("\"installed_version\":\"1.0.0\""));
        assert!(json.contains("\"available_version\":\"1.0.1\""));
        assert!(json.contains("\"size_bytes\":100"));
    }
}
