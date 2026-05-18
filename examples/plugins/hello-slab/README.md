# Hello Slab — example plugin

Demonstrates every contribution kind in the v1.3.0 "Foundry" plugin
system. Copy-paste-edit this directory as the starting point for your
own plugin.

## What this plugin contributes

| Kind | ID | Where it appears |
|---|---|---|
| Theme | `midnight` | Cmd-K → Appearance ("Midnight") |
| Locale | `jp` | Merged into i18n at boot — switch via Settings → Language |
| Command (URL) | `open-github` | Cmd-K → Plugin commands |
| Command (shell) | `hello-shell` | Cmd-K → Plugin commands |
| AI provider | `local-ollama` | Cmd-K → Plugin AI providers |
| PDF action | `qpdf-linearize` | Reader → ✦ Plugin dropdown |

## Install

```bash
mkdir -p ~/.slab/plugins
cp -r examples/plugins/hello-slab ~/.slab/plugins/
```

Then open Slab → **Settings → Plugins** → click **↻ Reload**.

You should see "Hello Slab" appear in the list with five contribution
chips ("1 theme · 1 locale · 2 commands · 1 AI provider · 1 PDF action").

## Try it out

- **Theme:** Press `Cmd-K` and search for "Midnight". The whole UI
  should switch to a deep-blue palette.
- **Commands:** Press `Cmd-K` and search for "hello". You should see
  "Say hello from a shell" — running it shows a toast with the
  shell's stdout. "Open Slab on GitHub" opens the repo.
- **AI provider:** Press `Cmd-K` and search for "Ollama". Running the
  action copies the base URL to your clipboard. (Full Beacon hook-up
  for plugin providers ships in a v1.3.x patch.)
- **PDF action:** Open any PDF, click the **✦ Plugin** button in the
  Reader toolbar, pick "Linearize with qpdf", pick an output path.

## Requirements

- The PDF action requires [`qpdf`](https://github.com/qpdf/qpdf) in
  your `$PATH`. (`brew install qpdf` on macOS,
  `apt install qpdf` on Debian/Ubuntu.)
- The AI provider expects [Ollama](https://ollama.ai) running at
  `127.0.0.1:11434`. The pull request to ship `default_model` =
  `llama3.2` assumes you've already run `ollama pull llama3.2`.

## Plugin layout

```
hello-slab/
  plugin.toml           # manifest — see docs/PLUGINS.md
  themes/
    midnight.css        # CSS variable overrides
  locales/
    jp.json             # flat key→translation map
```

See **[../../../docs/PLUGINS.md](../../../docs/PLUGINS.md)** for the
full manifest reference and contribution kind details.
