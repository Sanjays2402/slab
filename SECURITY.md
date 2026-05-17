# Security

Slab is a local-first desktop PDF tool. No content ever leaves your machine
unless you explicitly point Beacon at a remote AI provider (OpenAI / Claude /
OpenAI-compatible endpoint); the default is Ollama running on `127.0.0.1`.

## Reporting a vulnerability

If you find a security issue, please email **51058514+Sanjays2402@users.noreply.github.com**
or open a private security advisory at
<https://github.com/Sanjays2402/slab/security/advisories/new>.

Please do **not** open a public issue for security problems.

We aim to acknowledge reports within 72 hours and ship a fix within 14 days
for critical / high severity, 30 days for medium, best-effort for low.

## Supported versions

Only the latest minor release is supported. Slab is pre-1.0; users on older
versions should upgrade before reporting issues.

| Version | Supported          |
|---------|--------------------|
| 0.10.x  | ✅ (current dev)   |
| 0.9.x   | ✅ (latest stable) |
| < 0.9   | ❌                 |

## Accepted risks / suppressed alerts

The following Dependabot alerts have been triaged and accepted as **not
exploitable in Slab's runtime context**. They are not auto-dismissed in
Dependabot — instead they are documented here so future maintainers (and
future-Cake on cron duty) don't churn on them. Each entry must be re-evaluated
when its upstream root cause moves.

### 🟡 GHSA-wrw7-89jp-8q8g — `glib` 0.18.5 unsoundness in `VariantStrIter`

- **Ecosystem:** Rust (`src-tauri/Cargo.lock`)
- **Severity:** Medium
- **Vulnerable range:** `>= 0.15.0, < 0.20.0`
- **First patched:** 0.20.0
- **Why we're not patching today:**
  Slab ships **macOS-only** (Tauri 2.11 + `webkit2gtk` on Linux is in the dep
  graph but never compiled for our shipped targets). `glib` lives on the
  Linux/GTK side of Tauri — it is *not* loaded on macOS or Windows builds.
  The bug is in iterator unsoundness for `VariantStrIter`, a code path Slab
  never invokes.
- **Why we can't bump it ourselves:**
  `glib 0.18` is pinned transitively by `gtk 0.18.2 → tauri 2.11.1`. Forking
  Tauri to override one transitive crate is far more risk than the
  zero-practical-impact alert.
- **Re-evaluate when:** Tauri 2.12+ ships with `gtk-rs` 0.20+. Track upstream:
  <https://github.com/tauri-apps/tauri/issues?q=is%3Aissue+gtk-rs+0.20>.

### 🟢 GHSA-pxg6-pf52-xh8x — `cookie` 0.6.0 out-of-bounds chars

- **Ecosystem:** npm (`pnpm-lock.yaml`)
- **Severity:** Low
- **Vulnerable range:** `< 0.7.0`
- **First patched:** 0.7.0
- **Why we're not patching today:**
  Slab is a desktop Tauri app. It **does not run an HTTP server, does not
  issue cookies, and does not accept inbound cookies from any source**.
  `cookie` is a transitive dev dependency of `@sveltejs/kit 2.59.1` (already
  the latest SvelteKit) — used only in the SSR/adapter codepath, which Slab
  doesn't use (we use `@sveltejs/adapter-static`).
- **Why we can't bump it ourselves:**
  SvelteKit 2.59.1 (current latest) still pins `cookie ^0.6.0`. Override
  requires patching `pnpm-lock.yaml` with `pnpm.overrides`, which can desync
  with SvelteKit's expected behavior on minor bumps.
- **Re-evaluate when:** SvelteKit publishes a release pinning `cookie ^0.7`.
  Track <https://github.com/sveltejs/kit/blob/main/packages/kit/package.json>.

## Already fixed

| GHSA | Package | Fixed in | Slab commit/release |
|------|---------|----------|---------------------|
| GHSA-rcqx-6q8c-2c42 | svelte (DOM clobbering XSS) | 5.55.7 | merged PR #2, v0.10.0+ |
| GHSA-pr6f-5x2q-rwfp | svelte (SSR spread XSS) | 5.55.7 | merged PR #2, v0.10.0+ |
| GHSA-f3cj-j4f6-wq85 | svelte (SSR promise hydration XSS) | 5.55.7 | merged PR #2, v0.10.0+ |

## Beacon AI threat model

Slab v0.10.0+ ships **Beacon**, a local-first AI sidekick. Important security
properties:

- **Default provider is Ollama on `127.0.0.1:11434`.** No PDF content leaves the
  machine on a default install.
- **Remote providers are opt-in.** Setting `provider = "openai"` (or any
  OpenAI-compatible endpoint) in `~/.slab/beacon.toml` sends prompt + chunked
  PDF context to that endpoint. Beacon shows a one-time warning on first remote
  call.
- **API keys live in the system keychain**, not in plaintext config files.
- **Model output is rendered as plain text + safe markdown.** Beacon does not
  use `{@html}` on model responses; citations are parsed into structured chips,
  not raw HTML. This is why the svelte XSS bumps still mattered — defense in
  depth even though our own code is XSS-clean.
- **No telemetry, no analytics, no auto-update phone-home.** Update checks (when
  added) will require explicit user opt-in.
