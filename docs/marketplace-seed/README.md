# slab-plugins

> The curated plugin index for [Slab](https://github.com/Sanjays2402/slab).

This repo is **read-only data** consumed by Slab v1.4.0 "Bench" and
later. The Slab app fetches `index.json` from this repo at boot,
verifies each entry against a maintainer-held Ed25519 key, and offers
one-click install of signed plugins inside **Settings → Plugins →
Browse**.

If you're an end-user, you don't interact with this repo directly —
open Slab, click around the Browse tab, install plugins, that's it.

If you're a plugin author or contributor, this README explains how
the index works and how to submit a plugin. The canonical doc lives
in the Slab repo at
[`docs/MARKETPLACE.md`](https://github.com/Sanjays2402/slab/blob/main/docs/MARKETPLACE.md)
— this README is a pointer plus a quick reference.

---

## Repo layout

```
slab-plugins/
  index.json          # The thing Slab fetches. Public, signed entries.
  README.md           # This file.
```

That's it. No build, no CI required, no tooling. Hand-edit JSON, PR,
merge. The signing tool that produces entries (`slab-sign-plugin`)
lives in the main Slab repo because it has to share the canonical
signing payload definition with the verifier.

---

## How to submit a plugin

1. Write your plugin. Follow the author guide:
   [Slab plugins guide](https://github.com/Sanjays2402/slab/blob/main/docs/PLUGINS.md).
2. Build a release tarball (`.tar.gz`), upload it somewhere with a
   stable HTTPS URL (GitHub releases work great).
3. Open a PR on this repo adding an entry to `index.json` with
   `"signature": "PENDING_MAINTAINER_SIGNATURE"`.
4. Include in the PR description:
   - Link to your plugin's source code.
   - Link to the release tarball.
   - A summary of what the plugin does and which permissions
     (`fs` / `net` / `spawn`) it declares.
5. The maintainer reviews the code, re-checks `sha256`, signs the
   entry with `slab-sign-plugin`, and merges.

Full schema + step-by-step walkthrough:
[`MARKETPLACE.md`](https://github.com/Sanjays2402/slab/blob/main/docs/MARKETPLACE.md).

---

## Trust model

The maintainer signs **index entries**, not tarballs. Slab verifies
each entry's `signature` against a baked-in Ed25519 public key
(`MAINTAINER_PUBLIC_KEY` in
[`src-tauri/src/marketplace/verify.rs`](https://github.com/Sanjays2402/slab/blob/main/src-tauri/src/marketplace/verify.rs)).
If verification fails, Slab refuses to install.

Plugins are **not** sandboxed. The maintainer manually reviews each
submission before signing. End users trust the maintainer's judgement.

If you want to skip this trust chain entirely, hand-author a plugin
under `~/.slab/plugins/` — Slab loads those with no signature check.

---

## Index format

```json
{
  "schema_version": 1,
  "signing_key_id": "slab-maintainer-2026",
  "plugins": [
    { "id": "...", "name": "...", /* ...full entry... */ "signature": "..." }
  ]
}
```

Full field reference in `MARKETPLACE.md`. The short version:

- `id` — reverse-DNS plugin id (matches plugin.toml).
- `name`, `version`, `description`, `author` — display metadata.
- `download_url` — HTTPS URL of the tarball.
- `sha256` — hex SHA-256 of the tarball bytes.
- `size_bytes` — tarball size (≤ 5 MiB).
- `slab_compat` — SemVer requirement against Slab host version.
- `signature` — base64 Ed25519 sig over the entry minus this field.

**Field order is significant** — the signing payload is
`serde_json::to_vec` of the entry with `signature` removed, in
struct declaration order. Do not reorder.

---

## Removing a plugin from the index

Open a PR removing the entry. Already-installed copies on user
machines are left alone (Slab never auto-uninstalls), but the entry
disappears from Browse.

---

## License

The `index.json` and this README are released under the same license
as Slab itself (MIT). Plugins listed here are licensed by their own
authors — check each plugin's source repo.
