# Changelog

All notable changes to **Slab** are tracked here. Format follows
[Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/), and the
project adheres to [Semantic Versioning 2.0.0](https://semver.org/).

Older release notes (pre-2.0.1) live as full prose on the
[GitHub Releases page](https://github.com/Sanjays2402/slab/releases) and
are not duplicated here. From v2.0.1 onward every release gets an entry
in this file too.

---

## [3.24.0] — 2026-05-24 — Stack Pro

### Added

- **Three-way PDF compare (Stack Pro)** — pick a Base, Mine, and Theirs
  revision, get a side-by-side classified diff (both agree / mine only /
  theirs only / conflict), resolve conflicts, and materialize a merged
  PDF. The Litera Compare ($400/seat/yr) killer. Free + offline.
- **Export redline PDF** — bake the three-way diff to a self-contained
  colour-coded PDF (Base | Mine | Theirs columns) using standard-14
  fonts. Recipient does not need Slab installed.
- New `pdf::stack_diff3_export` module + six unit tests.
- New Tauri command `slab_diff3_export_pdf` and "Export redline PDF"
  action in the Diff3 panel.

## [Unreleased]

### v3.4.0 "Discovery" (preview — backend + UI on `feature/v3.4.0-discovery-slice-1-3`)

The release litigation paralegals have been asking for. Bates numbering is
the legal-discovery standard for stamping a sequential identifier on every
page of every document in a production set — and until today it was a
$239/yr Adobe Acrobat Pro DC feature. Now it's free, offline, and faster
than Adobe.

#### Added

- **Bates numbering panel** — prefix, zero-padded digits (1–12), six
  positions, custom font size and gray. `Cmd/Ctrl + Shift + B`.
- **Batch mode** — drop a folder of PDFs, get a numbered production set
  with a single monotonic counter chained across every page of every
  file, in seconds. Optional Relativity / Concordance / Everlaw–compatible
  CSV or JSON load file written next to the output.
- **Legal stamp panel** — four canonical preset chips
  (CONFIDENTIAL, ATTORNEY EYES ONLY, PRIVILEGED & CONFIDENTIAL, DRAFT)
  plus custom-text diagonal stamps with opacity, font-size, rotation, and
  page-subset controls. `Cmd/Ctrl + Shift + S`.
- **Live preview** — both panels re-render the stamped label as you type,
  drag a slider, or click a position. Paralegals can see exactly what
  page 1 will look like before they touch a document.
- **Command palette + sidebar entries + ShortcutsOverlay entries** so the
  two panels are discoverable from every existing entry point.
- **7-language UI** — feature labels in English, German, French, Spanish,
  Hindi, Tamil, Telugu (plus Arabic for the Slab feature index).

### Earlier on this branch (Slices 1–4, 2026-05-23)

- `bates_label_for()` pure helper extracted from the original `apply_bates`
  primitive, with unit-test coverage.
- `bates_batch` module — counter-chaining driver across an ordered list of
  PDFs, output-directory writer, CSV/JSON load-file emission.
- `legal_stamp` module — diagonal stamp engine with 4 canonical presets
  + custom-text preset, rotation, opacity, RGB color, optional page subset.
- Tauri IPC commands `slab_bates_apply`, `slab_bates_batch`,
  `slab_legal_stamp_apply` wired into the handler registry.

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
