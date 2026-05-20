# Changelog

All notable changes to **Slab** are tracked here. Format follows
[Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/), and the
project adheres to [Semantic Versioning 2.0.0](https://semver.org/).

Older release notes (pre-2.0.1) live as full prose on the
[GitHub Releases page](https://github.com/Sanjays2402/slab/releases) and
are not duplicated here. From v2.0.1 onward every release gets an entry
in this file too.

---

## [Unreleased]

Nothing yet.

## [2.0.1] — 2026-05-20 — Bundled Hello Workshop 🧩

### Added

- **Three example plugins now ship in the binary itself.** Open a brand-new
  Slab v2.0.1 and you immediately see
  `com.slab.examples.hello-workshop`,
  `com.slab.examples.storage-counter`, and `com.slab.examples.url-fetch`
  in Cabinet → Plugins — fully working, sandboxed, source-readable.
  No marketplace round-trip required to try out the plugin system.
- **"Bundled" pill in the Installed-Plugins panel** so users know the
  three pre-installed plugins came from Slab (not from a marketplace install)
  and are safe to disable or uninstall (they'll come back on a fresh install).
- **Onboarding tour now has a "🧩 Plugins, included" step** between
  Beacon AI and the Command Palette walk-through. Drives discovery of the
  bundled plugins, the SDK, and the marketplace in one screen.
- New unit tests pin the bundled-plugin roster and the manifest-id ⇄ roster
  parity. Drift between the rust roster and the SDK example manifests now
  fails the build with an actionable error pointing at the esbuild + shasum
  recipe.

### Changed

- Example plugin ids were renamed from short forms (`hello-workshop`) to
  reverse-DNS (`com.slab.examples.hello-workshop`) so they actually validate
  against the host's `id` rule. The published examples never loaded as-is
  before this release. Versions bumped to 0.2.0 and pinned sha256 hashes
  refreshed.

### Why it matters

The v2.0.0 "Workshop" release shipped the entire plugin platform — the
typed SDK, the capability-gated sandbox, the consent UI, the marketplace
client, the Cabinet integration. But a brand-new v2.0.0 user opening the
Plugins panel saw an empty list and no on-ramp. v2.0.1 closes that "0 → 1"
gap: the moment Slab starts, the platform has something to show.

---

## [2.0.0] — 2026-05-20 — Workshop 🔧

The first release with **plugins** — see the
[GitHub release notes](https://github.com/Sanjays2402/slab/releases/tag/v2.0.0)
for the full prose.

Headline:

- `@slab/plugin-sdk` typed TypeScript SDK with `definePlugin`, lifecycle
  hooks, `slab.ui.notify`, `slab.ui.registerTool`, `slab.storage`,
  `slab.fetch`.
- Permission-gated capability system. No plugin can silently read your
  filesystem or hit the network — the user grants each capability per-plugin.
- Cmd-K command palette + keyboard shortcuts wired through Mira's command
  layer; plugins register the same way the host does.
- Foundry plugin marketplace (browse / install / update / uninstall).

## Older releases

For releases v1.9.2 and earlier, see the
[GitHub Releases page](https://github.com/Sanjays2402/slab/releases).

[Unreleased]: https://github.com/Sanjays2402/slab/compare/v2.0.1...HEAD
[2.0.1]: https://github.com/Sanjays2402/slab/releases/tag/v2.0.1
[2.0.0]: https://github.com/Sanjays2402/slab/releases/tag/v2.0.0
