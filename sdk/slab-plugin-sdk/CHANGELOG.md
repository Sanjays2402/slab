# Changelog

All notable changes to `@slab/plugin-sdk` will be documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] — 2026-05-19

Initial public release. Matches Slab plugin `schema_version = 1`.

### Added

- **Type modules** mirroring the entire runtime surface:
  - `slab.manifest` — schema version, capability lattice (`FsCap`,
    `NetCap`, `UiCap`, `BeaconCap`), `PluginManifest` shape.
  - `slab.ui` — `registerTool`, `registerPanel`, `notify`,
    `NotificationLevel`.
  - `slab.document` — `getSelection`, `extractText`, `onOpen`/`onClose`,
    `Document`, `Selection`, `ExtractedText`.
  - `slab.storage` — `get`, `set`, `delete`, `clear`, `usage` with
    documented 8 MiB / 1 MiB / 64 KiB quotas.
  - `slab.fetch` — web-Fetch-shaped HTTP with `SlabRequestInit` /
    `SlabFetchResponse` and `r.ok` semantics for `4xx`/`5xx`.
  - `slab.beacon` — `complete`, `registerTool`, model picker
    integration types.
  - `SlabGlobal` — root composite type and ambient `declare global`
    declaration so a single `import` lights up IntelliSense for the
    entire surface.
- **Runtime helpers** (~0.5 KB gzipped):
  - `definePlugin(spec)` — identity helper that enforces the plugin
    shape at compile time and returns it unchanged at runtime.
  - `assertSlab(slab)` — exhaustively validates the host injected the
    expected surfaces (call sites: plugin entry points).
  - `trySlab(slab)` — `Result`-shaped variant that returns
    `{ ok: true, slab }` or `{ ok: false, missing: string[] }`.
- **Three reference example plugins** under `examples/`:
  - `hello-workshop` — minimal UI tool with keyboard shortcut.
  - `storage-counter` — persistent state + document lifecycle hooks.
  - `url-fetch` — host-mediated HTTP with per-host allow-list.
- **Build infrastructure**:
  - Dual ESM (`.js`) + CJS (`.cjs`) + `.d.ts` emit.
  - Post-build `scripts/rename-cjs.mjs` works around TypeScript
    [issue #54573](https://github.com/microsoft/TypeScript/issues/54573)
    (no native `outFileExtension`); rewrites internal `require()` and
    `sourceMappingURL` comments to point at the renamed `.cjs` files.
  - Strict TypeScript: `exactOptionalPropertyTypes`, `isolatedModules`,
    `noUnusedLocals`, `noUnusedParameters`, `noImplicitOverride`,
    `noImplicitReturns`, `noFallthroughCasesInSwitch`,
    `noUncheckedIndexedAccess`, `noPropertyAccessFromIndexSignature`.
- **Consumer smoke test** at `tests/typecheck-smoke.ts` exercising every
  public type with positive cases and `@ts-expect-error` negative
  cases. Runs as part of `pnpm typecheck`.

### Notes

- Package licensed MIT (parent Slab repo is GPL-3.0) — chosen to keep
  plugin-author friction low and avoid copyleft contamination of
  third-party plugin source.
- Ground-truth Rust source is referenced in JSDoc on each type module
  (`src-tauri/src/plugins/runtime/*.rs`, `src-tauri/src/plugins/*.rs`)
  so type drift can be caught by `grep`.
- Not yet published to npm (requires `@slab` org ownership). Tracked as
  a Slice 9 follow-up.

[Unreleased]: https://github.com/Sanjays2402/slab/compare/v2.0.0...HEAD
[0.1.0]: https://github.com/Sanjays2402/slab/releases/tag/sdk-v0.1.0
