# @slab/plugin-sdk

> Type-safe authoring kit for [Slab](https://github.com/Sanjays2402/slab) plugins.

`@slab/plugin-sdk` gives plugin authors:

1. **Strict, accurate TypeScript types** for the entire `slab.*` runtime
   global — manifests, UI, document events, persistent storage, host
   fetch, and Beacon AI hooks.
2. **Runtime helpers** — `definePlugin()`, `assertSlab()`, `trySlab()` —
   that catch shape mistakes at compile time and runtime.
3. **An ambient declaration** so a single `import` of the package wires
   up IntelliSense for the entire `globalThis.slab` surface.

Slab plugins run inside a sandboxed JS host (QuickJS) embedded in the
Slab desktop app. The host exposes a small, capability-gated API
surface. Without this SDK, every plugin author has to read Rust source
to find out what's available. With it, you get autocompletion in
2 keystrokes.

---

## Install

```bash
npm install --save-dev @slab/plugin-sdk
# or
pnpm add -D @slab/plugin-sdk
# or
yarn add -D @slab/plugin-sdk
```

Slab plugins ship as a **single bundled ESM file** plus a manifest. The
SDK is a build-time-only dependency: it provides types and tiny
helpers (no heavy runtime). At ship time you bundle with
`esbuild`/`rollup`/`vite` and the SDK helpers tree-shake or inline.

---

## 60-second quickstart

```ts
// script.ts
import { definePlugin } from "@slab/plugin-sdk";

export default definePlugin({
  id: "my-first-plugin",

  onLoad(slab) {
    slab.ui.registerTool({
      id: "say-hi",
      label: "Say Hi",
      shortcut: "Ctrl+Shift+H",
      invoke: () => slab.ui.notify("Hello from my plugin!", "info"),
    });
  },

  onUnload(slab) {
    slab.ui.notify("Goodbye!", "info");
  },
});
```

```toml
# manifest.toml
schema_version = 1
id = "my-first-plugin"
name = "My First Plugin"
version = "0.1.0"
authors = ["You <you@example.com>"]
description = "A demo plugin."
license = "MIT"

[script]
entry = "script.js"
sha256 = "REPLACE_AFTER_BUILD"  # see Build section below

[runtime.capabilities]
fs = "none"
net = "none"
ui = "tool"
beacon = "none"
```

```bash
# Build
npx esbuild script.ts --bundle --format=esm --platform=neutral \
  --target=es2022 --outfile=script.js --external:@slab/plugin-sdk

# Compute SHA256 → paste it into manifest.toml as script.sha256
shasum -a 256 script.js

# Bundle script.js + manifest.toml into a .slab-plugin archive
zip my-first-plugin.slab-plugin script.js manifest.toml
```

Drag the `.slab-plugin` file into Slab's plugin panel. Done.

---

## API tour

All surfaces are accessed through the ambient `slab` global. Import
anything from `@slab/plugin-sdk` and the types light up automatically.

### `slab.ui` — user interface

```ts
slab.ui.registerTool({
  id: "highlight-all",
  label: "Highlight all matches",
  shortcut: "Cmd+Shift+H",
  invoke: () => { /* … */ },
});

slab.ui.registerPanel({
  id: "my-panel",
  title: "My Panel",
  render: (root) => {
    root.innerHTML = "<h1>Hi</h1>";
  },
});

slab.ui.notify("Saved!", "info");  // "info" | "warn" | "error"
```

### `slab.document` — current document + lifecycle

```ts
slab.document.onOpen((doc) => {
  console.log("opened", doc.uri, "pages:", doc.pageCount);
});

const sel = slab.document.getSelection();
if (sel) console.log("selected:", sel.text);

const pages = await slab.document.extractText({ pages: [0, 1, 2] });
```

### `slab.storage` — per-plugin persistent KV

Per-plugin namespaced storage. Hard quotas enforced by the host.

```ts
await slab.storage.set("opens", "42");
const opens = await slab.storage.get("opens");
const { keys, totalBytes } = await slab.storage.usage();
await slab.storage.clear();
```

| Quota | Limit |
|---|---|
| Total bytes per plugin | 8 MiB |
| Bytes per value | 1 MiB |
| Bytes per key | 64 KiB |

### `slab.fetch` — host-mediated HTTP

Web-Fetch-shaped HTTP. Gated by `net_allow_hosts` in manifest.

```ts
const r = await slab.fetch("https://example.com", {
  method: "GET",
  timeoutMs: 5_000,
});
if (r.ok) console.log(r.body);
```

Only **network** failures reject — `4xx`/`5xx` resolve with `r.ok === false`.

### `slab.beacon` — local-first AI

Hook into Slab's local LLM (Ollama) — chat with PDFs, summarise,
register your own AI tools.

```ts
slab.beacon.registerTool({
  id: "summarise-page",
  label: "Summarise current page",
  invoke: async () => {
    const page = (await slab.document.extractText({ pages: [0] })).text;
    return slab.beacon.complete({
      prompt: `Summarise in 3 bullets:\n\n${page}`,
    });
  },
});
```

---

## Capability lattice

Every plugin declares its capabilities in `manifest.toml`. The host
**enforces** each one at every API call — declaring `fs = "read"` and
then trying `slab.storage.set` won't escape sandboxing, but trying to
hit `slab.fetch` without `net != "none"` will throw synchronously.

| Axis | Values | What it gates |
|---|---|---|
| `fs` | `none` \| `read` \| `read-write` | Future: direct file system (today, only via `slab.document`) |
| `net` | `none` \| `specific` \| `any` | `slab.fetch`. `"specific"` requires `net_allow_hosts`. |
| `ui` | `none` \| `panel` \| `tool` \| `all` | UI surface registration |
| `beacon` | `none` \| `tool` \| `provider` \| `all` | AI tools and custom model providers |

The user sees this lattice on the **plugin consent screen** the first
time they enable your plugin, and can dial each one down.

---

## Examples

Three reference plugins live under `examples/`. Each is < 100 LOC and
ships with build recipe.

| Example | Caps | Shows off |
|---|---|---|
| [`hello-workshop`](./examples/hello-workshop) | `ui=tool` | Minimal "Hello World" — register a tool with a shortcut |
| [`storage-counter`](./examples/storage-counter) | `ui=tool` | Persistent state + `slab.document.onOpen` lifecycle |
| [`url-fetch`](./examples/url-fetch) | `net=specific, ui=tool` | Host-mediated HTTP + per-host allow-list |

---

## Security model

Slab plugins run in a sandboxed QuickJS runtime, isolated from:

- The host process's memory
- The user's filesystem (except via explicit `slab.document` calls)
- The network (except via `slab.fetch` with declared allow-listed hosts)
- Other plugins (each gets its own KV namespace)

The **user is the final authority**. On first enable, they see your
declared capabilities and can:

- Approve as-declared
- Restrict (e.g. dial `net = "specific"` down to an empty allow-list)
- Deny entirely

Your plugin must handle the "restricted" case gracefully — `slab.fetch`
to a denied host throws synchronously; `slab.storage` over quota
rejects; etc.

The SDK types reflect these failure modes so you can't ignore them.

---

## Versioning

`@slab/plugin-sdk` follows semver. Major bumps mean breaking type or
runtime changes; minor bumps add new surfaces; patches fix bugs and
docs.

The package version tracks the **Slab plugin schema version**, not the
Slab app version. Plugin schema is currently at `schema_version = 1`.

See [`CHANGELOG.md`](./CHANGELOG.md).

---

## Building locally

```bash
git clone https://github.com/Sanjays2402/slab.git
cd slab/sdk/slab-plugin-sdk
# Uses parent repo's TypeScript installation
../../node_modules/.bin/tsc --noEmit -p tsconfig.tests.json
node scripts/rename-cjs.mjs  # post-build CJS extension fix
```

The build emits:
- `dist/esm/` — ESM `.js` + `.d.ts` + sourcemaps
- `dist/cjs/` — CommonJS `.cjs` + sourcemaps
- `dist/types/` — ambient global declaration

---

## Contributing

Slab is open source (GPL-3.0). The SDK package itself ships as MIT to
keep plugin author friction low. Bug reports and PRs welcome at
[github.com/Sanjays2402/slab](https://github.com/Sanjays2402/slab).

---

## License

MIT © Sanjay Sridhar
