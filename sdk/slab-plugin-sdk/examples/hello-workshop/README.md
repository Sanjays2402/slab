# hello-workshop — smallest Slab v2.0.0 plugin

The "it works" demo plugin. Registers a single Cabinet tool, "Say Hi", that
emits a toast notification when invoked.

## What it shows off

- **`definePlugin`** — typed plugin authoring with one-line bootstrap.
- **`slab.ui.notify(...)`** — toast notifications, no capability required.
- **`slab.ui.registerTool(...)`** — Cabinet quick-action + command palette
  entry, with a default keyboard shortcut.

Total: 18 lines of script. This is the floor for a useful Slab plugin.

## Files

```
hello-workshop/
├── manifest.toml   # plugin metadata + capability declaration
└── script.ts       # the plugin code, authored in TS
```

## Build recipe

Slab's runtime evaluates plain ES modules, not TypeScript. Bundle `script.ts`
to `script.js`, compute the sha256, and update `manifest.toml`:

```bash
# from this directory
npx esbuild script.ts \
  --bundle --format=esm --platform=neutral --target=es2022 \
  --outfile=script.js --external:@slab/plugin-sdk

# compute and pin the hash
HASH=$(shasum -a 256 script.js | awk '{print $1}')
sed -i.bak "s/^sha256 = .*/sha256 = \"$HASH\"/" manifest.toml
rm manifest.toml.bak
```

Then drop the directory into `~/.slab/plugins/` and reload Slab. The Cabinet
permissions modal will ask you to confirm the declared capabilities; the
`hello-workshop` plugin only asks for `ui = "tool"` so the modal is one
click.

## Capabilities used

| Axis | Declared | Why |
|---|---|---|
| `fs` | `none` | No file access |
| `net` | `none` | No HTTP |
| `ui` | `tool` | Registers a toolbar tool |
| `beacon` | `none` | No AI hooks |

## License

MIT (same as the SDK).
