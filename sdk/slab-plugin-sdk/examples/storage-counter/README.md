# storage-counter — per-plugin persistent state demo

Counts how many PDFs you've opened, persisting across sessions in this
plugin's own slice of `~/.slab/plugin-storage.sqlite`.

## What it shows off

- **`slab.document.onOpen`** — lifecycle event hook fires every time a PDF
  loads in the reader.
- **`slab.storage.{get,set}`** — per-plugin sqlite-backed KV. Your data
  is invisible to every other plugin (and you cannot see theirs).
- **`slab.storage.{clear,usage}`** — reset + quota introspection.
- **Three tools** — open count display, reset, and the automatic increment.

## Files

```
storage-counter/
├── manifest.toml
└── script.ts
```

## Build recipe

Same as `hello-workshop` — bundle to `script.js`, sha256, drop into
`~/.slab/plugins/`.

## Capabilities used

| Axis | Declared | Why |
|---|---|---|
| `fs` | `none` | Storage is sandboxed; no manifest cap needed |
| `net` | `none` | No HTTP |
| `ui` | `tool` | Two toolbar tools |
| `beacon` | `none` | No AI hooks |

Note: `slab.storage.*` has no manifest capability gate — scoping is the
security boundary. See the [security model][1] for why.

[1]: ../../README.md#security-model

## License

MIT.
