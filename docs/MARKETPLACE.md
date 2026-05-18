# Slab marketplace 🪑

> Slab v1.4.0 "Bench" adds a curated plugin marketplace. The Slab app
> fetches a signed `index.json` at boot, verifies each entry against a
> hard-coded Ed25519 public key, and lets users one-click install
> plugins from inside **Settings → Plugins → Browse**.

## Table of contents

1. [What this is, in plain English](#what-this-is-in-plain-english)
2. [Trust model](#trust-model)
3. [User flow](#user-flow)
4. [For plugin authors — submitting your plugin](#for-plugin-authors--submitting-your-plugin)
5. [Index schema reference](#index-schema-reference)
6. [The `slab-sign-plugin` tool](#the-slab-sign-plugin-tool-maintainer-only)
7. [Updating an existing plugin](#updating-an-existing-plugin)
8. [Removing a plugin from the index](#removing-a-plugin-from-the-index)
9. [Key rotation](#key-rotation-future)

---

## What this is, in plain English

The marketplace is a single JSON file hosted in a public GitHub repo:

```
https://raw.githubusercontent.com/Sanjays2402/slab-plugins/main/index.json
```

The file lists plugins — name, version, description, download URL,
size, SHA-256, and a base64 Ed25519 signature. Slab pulls this file
when the Browse tab opens, caches a copy to `~/.slab/marketplace-cache.json`
so the user always has *something* even if offline, and renders one
card per entry.

When the user clicks **Install**, Slab:

1. Downloads the tarball from `download_url`.
2. Verifies the bytes against `sha256` (refuses if it doesn't match).
3. Verifies the index entry's `signature` against a public key compiled
   into the Slab binary (refuses if the signature is bad).
4. Atomically extracts the tarball into `~/.slab/plugins/<id>/`
   (stages to a temp dir first, then renames — never a half-written
   plugin folder on disk).
5. Re-discovers plugins so the new one appears in the Installed tab
   without a restart.

Uninstall removes `~/.slab/plugins/<id>/` and re-discovers. Both ops
are pure filesystem; no network is touched on uninstall.

---

## Trust model

Slab uses **maintainer-signed index entries**, not author-signed
tarballs. Each entry in `index.json` is signed by the Slab maintainer
(see `MAINTAINER_PUBLIC_KEY` in `src-tauri/src/marketplace/verify.rs`,
key id `slab-maintainer-2026`).

Concretely this means:

- **What you can trust:** the tarball you download is the exact one
  the maintainer signed. Sha256 + signature catch any swap-in attack
  (compromised CDN, malicious mirror, accidental file replacement).
- **What you cannot trust:** the *behaviour* of the plugin itself.
  Plugins are not sandboxed in v1.4 — a plugin can read files, spawn
  shell commands, or hit the network if the manifest declares those
  permissions. The maintainer signs entries after a manual code review,
  but you're still trusting one person's judgement.

If you don't want that trust assumption, don't enable Browse plugins.
The local `~/.slab/plugins/` path is still there — you can hand-author
plugins (see [`PLUGINS.md`](PLUGINS.md)) and Slab will load them with
no signature check at all.

The public key is **hard-coded** into the Slab binary at build time.
You can verify it matches what the maintainer holds:

```bash
# Maintainer side
cargo run --bin slab-sign-plugin -- --print-public-key

# Slab binary side (built-in constant)
grep -A2 'MAINTAINER_PUBLIC_KEY' src-tauri/src/marketplace/verify.rs
```

Both should print the same 32 hex bytes.

---

## User flow

1. Open Slab → **Settings → Plugins** → click the **Browse** tab.
2. Slab fetches `index.json` (or uses the cached copy if offline).
3. Click any plugin card to see the detail drawer (description, size,
   SHA-256 prefix, signature prefix, download URL).
4. Click **Install**. Watch the progress modal — verifying signature →
   downloading → extracting → done.
5. The plugin now appears in the **Installed** tab.
6. To remove: open the Installed tab, click **Uninstall**, confirm in
   the destructive-action modal.

Offline behaviour: Slab keeps the last successful fetch in
`~/.slab/marketplace-cache.json`. If the network is down, Browse shows
that cached list with a "stale cache" indicator and an option to retry.

---

## For plugin authors — submitting your plugin

The marketplace is curated — there's no self-service publish API yet.
To get your plugin into the index, open a PR on the
`Sanjays2402/slab-plugins` repo. The flow:

### 1. Build a release tarball

```bash
cd ~/.slab/plugins/my-plugin
tar czf my-plugin-1.0.0.tar.gz \
  plugin.toml themes/ locales/ README.md
sha256sum my-plugin-1.0.0.tar.gz
```

The tarball must:
- Contain a top-level `plugin.toml` (no extra prefix directory).
- Be ≤ 5 MiB compressed.
- Uncompress to ≤ 50 MiB.
- Only contain regular files, directories, and symlinks pointing
  inside the tarball (no `../`, no absolute paths, no devices). Slab's
  install pipeline enforces this — see
  `src-tauri/src/marketplace/install.rs::sanitize_entry_path`.

### 2. Host the tarball somewhere stable

GitHub releases are the easiest path:

```bash
gh release create my-plugin-v1.0.0 my-plugin-1.0.0.tar.gz
```

The `download_url` will be:
`https://github.com/<owner>/<repo>/releases/download/my-plugin-v1.0.0/my-plugin-1.0.0.tar.gz`

Any HTTPS URL works as long as it serves the exact bytes you hashed.

### 3. Open a PR on `Sanjays2402/slab-plugins`

Add a draft entry to `index.json` (without `signature` — the maintainer
fills that in):

```json
{
  "id": "com.example.my-plugin",
  "name": "My Plugin",
  "version": "1.0.0",
  "description": "What it does, in 1–2 sentences.",
  "author": "Your Name",
  "download_url": "https://github.com/.../my-plugin-1.0.0.tar.gz",
  "sha256": "abc123…",
  "size_bytes": 12345,
  "slab_compat": ">=1.4.0",
  "signature": "PENDING_MAINTAINER_SIGNATURE"
}
```

Include in the PR description:
- Link to the plugin's source repo (so reviewers can read the code).
- Link to the release tarball you uploaded.
- A one-paragraph summary of what the plugin does and which
  permissions it needs.

### 4. Wait for review

The Slab maintainer:
- Reads the plugin source for malice / quality (review is manual; we
  don't have a sandbox).
- Re-downloads the tarball and re-checks `sha256`.
- Runs `slab-sign-plugin` to produce a real signature, replaces the
  `PENDING_*` placeholder, and merges the PR.

After merge, the new entry shows up in users' Browse tab within
~5 minutes (the cache TTL).

---

## Index schema reference

The full schema lives in `src-tauri/src/marketplace/index.rs`. Quick
reference:

### Top-level `index.json`

| Field | Type | Notes |
|---|---|---|
| `schema_version` | u32 | Currently `1`. Slab refuses to load higher values. |
| `signing_key_id` | string | Currently `"slab-maintainer-2026"`. Maps to a baked-in public key. |
| `plugins` | array | Zero or more `IndexEntry` objects. |

### `IndexEntry`

| Field | Type | Notes |
|---|---|---|
| `id` | string | Reverse-DNS plugin id (`com.example.x`). Must contain `.`. Matches the `id` in the plugin's `plugin.toml`. |
| `name` | string | Display name. |
| `version` | string | SemVer release version. Should match `plugin.toml`'s `version`. |
| `description` | string | One-line summary shown on the card. |
| `author` | string | Display author. |
| `download_url` | string | HTTPS URL of the tarball (`.tar.gz`). |
| `sha256` | string | Lowercase hex SHA-256 of the tarball bytes. |
| `size_bytes` | u64 | Tarball size in bytes. Must be ≤ 5 MiB. |
| `slab_compat` | string | SemVer requirement against the Slab host version, e.g. `">=1.4.0"`. |
| `signature` | string | Base64-encoded Ed25519 signature over the entry minus this field. Filled in by the maintainer's signing tool. |

Field order is significant — the signature is computed over
`serde_json::to_vec` of `IndexEntryUnsigned`, which has a stable field
declaration order matching the table above (minus `signature`). Do
not reorder fields when hand-editing.

---

## The `slab-sign-plugin` tool (maintainer only)

`slab-sign-plugin` is a second binary in the Slab repo
(`src-tauri/Cargo.toml`, `[[bin]] name = "slab-sign-plugin"`). It reads
the maintainer's private key from `~/.slab-maintainer-key`, computes
the tarball's SHA-256, builds the canonical signing payload, and
prints a signed `IndexEntry` JSON to stdout.

```bash
cargo run --bin slab-sign-plugin -- \
  --tarball ./my-plugin-1.0.0.tar.gz \
  --id com.example.my-plugin \
  --name "My Plugin" \
  --version 1.0.0 \
  --description "What it does." \
  --author "Author Name" \
  --download-url https://github.com/.../my-plugin-1.0.0.tar.gz \
  --slab-compat ">=1.4.0"
```

Output is JSON ready to paste into `index.json`. Pipe through `jq` if
you want it pretty-printed.

Aux commands:

```bash
# Print the public key derived from the private key on disk.
# Compare against MAINTAINER_PUBLIC_KEY in verify.rs — they must match.
cargo run --bin slab-sign-plugin -- --print-public-key

# Re-sign the verifier's regression test fixture. Use this if you
# rotate the key — the printed signature replaces the one in
# verify.rs's tests.
cargo run --bin slab-sign-plugin -- --print-fixture-signature
```

The private key file `~/.slab-maintainer-key` is base64-encoded raw
Ed25519 seed (32 bytes). It is **never committed**. Keep `chmod 600`.

---

## Updating an existing plugin

To ship a new version:

1. Bump `version` in your `plugin.toml`.
2. Build a new tarball, hash it, upload it.
3. Open a PR on `slab-plugins` replacing the entry's `version`,
   `download_url`, `sha256`, and `size_bytes`. Reset the `signature`
   to `PENDING_MAINTAINER_SIGNATURE`.
4. Maintainer re-signs + merges.

Slab compares semver versions client-side. If `index.json` lists a
strictly higher version than what's installed, the Installed tab
shows an "Update available — vX.Y.Z" pill and the Browse tab card's
button changes to "Update to vX.Y.Z".

Major version bumps are handled the same as minor — the user sees an
update offer either way. If you need to ship a breaking change that
shouldn't auto-update, bump the `id` instead (e.g.
`com.example.plugin-v2`).

---

## Removing a plugin from the index

Open a PR removing the entry from `index.json`. After merge, the
plugin disappears from Browse, but:

- **Already-installed copies are not touched.** Users keep using the
  version they have. The Installed tab no longer offers updates.
- **The download URL doesn't need to stay live.** Once removed, Slab
  never re-downloads.

If you need to actively notify users (security advisory, etc.), open
an issue on `Sanjays2402/slab` — there's no in-app messaging yet.

---

## Key rotation (future)

If the maintainer private key is ever compromised:

1. Generate a new keypair.
2. Bake the new public key into the Slab binary under a new
   `signing_key_id` (e.g. `slab-maintainer-2027`).
3. Update `signing_key_id` in `index.json` to the new id.
4. Re-sign all entries with the new key.
5. Ship a Slab patch release with the new key + a deprecation note
   for the old one.

Old Slab clients (built before the rotation) will fail signature
verification on the new index — they'll fall back to the cached
old-key index and stop seeing updates until upgraded. This is
intentional: it forces a Slab update along with the key rotation.

v1.4 hard-codes one key. Multi-key support (so a transition window
works) is parked for v1.5+.
