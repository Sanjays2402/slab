# Slab plugins — author guide

> Slab v1.3.0 "Foundry" introduces a declarative plugin system. Drop a folder into `~/.slab/plugins/`, contribute themes / locales / commands / AI providers / PDF actions via a single `plugin.toml` manifest, and reload — no compilation, no native code.

## TL;DR

1. Plugins live at `~/.slab/plugins/<your-plugin>/plugin.toml`.
2. Every plugin is a folder. The manifest lists which assets you contribute, the asset files (CSS / JSON) live next to the manifest.
3. Open Slab → **Settings → Plugins** → click **↻ Reload** to discover new plugins, or enable/disable existing ones.
4. There's a working example at [`examples/plugins/hello-slab/`](../examples/plugins/hello-slab/) — copy that folder and edit.

---

## Directory layout

```
~/.slab/plugins/
  my-plugin/
    plugin.toml         # required — manifest
    themes/
      cool.css          # optional, referenced from manifest
    locales/
      pt.json           # optional, referenced from manifest
    README.md           # optional — your docs
```

Slab scans direct children of `~/.slab/plugins/` at boot. Any sub-folder with a `plugin.toml` is loaded; folders without one are silently ignored.

---

## Manifest reference (`plugin.toml`)

```toml
# Required top-level fields.
id = "com.example.my-plugin"   # reverse-DNS, must contain at least one '.'
name = "My Plugin"              # display name
version = "0.1.0"               # your plugin version (SemVer)
slab_compat = ">=1.3.0"         # which Slab versions you support (SemVer req)

# Optional top-level fields.
description = "What this plugin does."
author = "Your Name"
homepage = "https://example.com"
permissions = ["fs", "net", "spawn"]   # declarative — granted by user via Settings → Plugins
```

### Required field rules

| Field | Rule |
|---|---|
| `id` | Reverse-DNS-style identifier. Must contain at least one `.` to discourage collisions. Used as the registry key. |
| `name` | Human-readable name shown in Settings → Plugins. |
| `version` | SemVer string. Slab does not currently check this but it's reserved for future update flows. |
| `slab_compat` | SemVer requirement string (e.g. `">=1.3.0"`, `"^1.3"`). Slab refuses to load plugins whose `slab_compat` doesn't satisfy the running version. |

### Permissions

Permissions are declarative metadata — Slab uses them to surface "this plugin will be able to…" copy in the UI but does *not* sandbox the contributed assets. Be honest:

- `fs` — your plugin reads/writes the user's filesystem (most plugins).
- `net` — your plugin makes network calls (typically AI providers).
- `spawn` — your plugin spawns subprocesses (any plugin with a `pdf_actions` or `shell` command).

---

## Contribution kinds

A plugin can contribute zero or more of each kind. All sections are optional. Use the `[[contributions.<kind>]]` TOML array-of-tables syntax — Slab merges multiple entries from one plugin and multiple plugins into one global pool.

### Themes (`[[contributions.themes]]`)

Custom UI themes injected as `<style id="slab-plugin-theme">` at runtime. Selectable via Cmd-K → Appearance.

```toml
[[contributions.themes]]
id = "midnight"          # unique within your plugin
label = "Midnight"       # shown in the picker
css = "themes/midnight.css"   # relative path from plugin dir
dark = true              # hint for system-prefers-dark matching
```

The CSS file should override Slab's CSS variables (`--bg`, `--text`, `--accent`, etc.). See `src/lib/theme.ts` in the Slab repo for the canonical variable list, and `examples/plugins/hello-slab/themes/midnight.css` for a working starting point.

### Locales (`[[contributions.locales]]`)

Add a new UI language, or override strings for an existing one.

```toml
[[contributions.locales]]
locale = "jp"            # ISO-style locale code; "jp" creates a new language
bundle = "locales/jp.json"
```

The bundle is a flat JSON map of dotted-key → translation:

```json
{
  "settings.title": "設定",
  "features.reader": "リーダー"
}
```

Missing keys fall back to English (the canonical bundle). Partial coverage is fine — you don't have to translate every key.

### Commands (`[[contributions.commands]]`)

Shell or URL commands runnable from Cmd-K. Exactly *one* of `shell` or `url` must be set.

**URL command** (opens in default browser):

```toml
[[contributions.commands]]
id = "open-github"
label = "Open Slab on GitHub"
url = "https://github.com/Sanjays2402/slab"
```

**Shell command** (spawns subprocess, captures stdout/stderr, surfaces in toast):

```toml
[[contributions.commands]]
id = "hello"
label = "Say hello from a shell"
shell = "echo 'Hello from Slab plugin'"
```

Shell commands run with a 30-second timeout by default. Use them for one-shot diagnostics, not long-running daemons.

### AI providers (`[[contributions.ai_providers]]`)

Plug Slab's Beacon AI into your own LLM endpoint. Currently supports OpenAI-compatible APIs (Ollama, llama.cpp, vLLM, LM Studio, OpenRouter, Together, etc.).

```toml
[[contributions.ai_providers]]
id = "local-ollama"
label = "Local Ollama"
kind = "openai_compat"        # currently the only supported kind
base_url = "http://127.0.0.1:11434/v1"
default_model = "llama3.2"

# Optional: custom headers. `$VAR` syntax interpolates env vars at
# runtime so you don't have to ship API keys in plain text.
[contributions.ai_providers.headers]
Authorization = "Bearer $OPENROUTER_API_KEY"
```

Plugin providers surface in Cmd-K → Plugin AI providers. Full hook-up to Beacon's chat / search runtime ships in a v1.3.x patch — for now running the action copies the `base_url` to your clipboard so you can sanity-check the endpoint.

### PDF actions (`[[contributions.pdf_actions]]`)

External-CLI-backed PDF transformations. Slab passes the current document's path as `{in}` and the user-chosen output as `{out}`.

```toml
[[contributions.pdf_actions]]
id = "qpdf-linearize"
label = "Linearize with qpdf"
cli = "qpdf"                  # must be in $PATH
args = ["--linearize", "{in}", "{out}"]
timeout_ms = 60000            # default 30000
```

Surfaces in the Reader toolbar's **✦ Plugin** dropdown when a PDF is open. Slab prompts the user for an output path via the system save dialog, then runs `cli` with `args` (after `{in}`/`{out}` substitution), respecting the timeout. Stdout/stderr surface in a toast on completion.

---

## Validation rules (cheat-sheet)

Slab refuses to load manifests that violate these rules and shows a red "Manifest error" chip in Settings → Plugins (the user can expand it to see the exact message):

- `id` must contain at least one `.`.
- `slab_compat` must be a valid SemVer requirement string.
- Each `command` must have exactly one of `shell` or `url` (not both, not neither).
- AI provider `kind` must be `"openai_compat"`.
- AI provider `base_url` must use `http://` or `https://`.
- No two contributions within the same plugin may share an `id`.

---

## Enabled-state persistence

User toggles in Settings → Plugins persist to `~/.slab/plugins/plugin-state.toml`. Plugins default to **enabled** on first load. The persistence file looks like:

```toml
[plugins]
"com.example.my-plugin" = true
"com.other.flaky" = false
```

You generally shouldn't touch this file — Slab manages it. It's documented here only so power users know what's happening.

---

## Security model (honest version)

Slab does **not** sandbox plugin assets. A plugin that contributes a `shell` command or `pdf_actions` `cli` can execute arbitrary subprocesses on the user's machine; a plugin theme can override any CSS variable.

This is a deliberate tradeoff: plugins are local files the user explicitly drops into `~/.slab/plugins/`. The trust boundary is *user authorisation*, not runtime isolation. The Settings → Plugins panel shows the plugin's `id`, `author`, declared `permissions`, and absolute on-disk path so users can audit before enabling.

Don't install plugins you wouldn't `cp -r` from the same source onto your `~/`.

---

## Distribution

There's no plugin marketplace yet. For now: ship your plugin as a git repo, tar/zip archive, or just a folder. Users install by extracting into `~/.slab/plugins/<your-plugin>/` and clicking Reload.

A first-party gallery + signed-manifest install flow are roadmap items for post-v1.3.

---

## Troubleshooting

| Symptom | Likely cause |
|---|---|
| Plugin doesn't appear in Settings → Plugins | The folder has no `plugin.toml`, or it's nested too deep. Manifests must be at `~/.slab/plugins/<folder>/plugin.toml` — not deeper. |
| Red "Manifest error" chip | Invalid TOML, or a validation rule above failed. Click the chip to expand the parse/validation error. |
| Theme doesn't show in Cmd-K | The plugin is disabled, or `css` path is wrong. Check Settings → Plugins → expand contributions. |
| Locale doesn't appear in Settings → Language | The locale code conflicts with an existing one *and* your bundle has zero keys. Add at least one key. |
| PDF action says "spawn failed" | The `cli` binary isn't in `$PATH`. Add the full path, or fix your `$PATH`. |
| Shell command silently exits | The command timed out (default 30s). Move heavy work elsewhere. |

---

## Example plugin

The Slab repo ships a runnable reference at [`examples/plugins/hello-slab/`](../examples/plugins/hello-slab/). It exercises every contribution kind in one manifest and is the recommended starting point for new plugin authors.

```bash
mkdir -p ~/.slab/plugins
cp -r examples/plugins/hello-slab ~/.slab/plugins/
# Open Slab → Settings → Plugins → Reload
```

Then explore the source of `examples/plugins/hello-slab/plugin.toml` alongside this doc.

---

## Schema stability

Slab follows SemVer for the manifest schema:
- **patch** version bumps add optional fields that older Slabs ignore safely.
- **minor** bumps add new contribution kinds.
- **major** bumps break older manifests.

Pin your `slab_compat` accordingly. If your plugin only uses themes + locales, `">=1.3.0"` is enough; if you rely on a v1.3.5-added field, write `">=1.3.5"`.

---

## Where to go next

- Read [`examples/plugins/hello-slab/plugin.toml`](../examples/plugins/hello-slab/plugin.toml).
- Open Settings → Plugins in Slab and click "Open plugins directory" to see your real plugins root.
- File issues or feature requests at <https://github.com/Sanjays2402/slab/issues>.
