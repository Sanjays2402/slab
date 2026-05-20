# url-fetch — host-mediated HTTP demo

Exercises `slab.fetch` with capability declaration and per-host allow-list.

## What it shows off

- **`slab.fetch(url, init)`** — web-Fetch-shaped HTTP, including:
  - GET with `timeoutMs`
  - POST with `Content-Type` + JSON body
  - `r.ok` semantics (only network failures reject; 4xx/5xx resolve)
- **`NetCap === "specific"`** — declare allowed hosts in
  `[runtime.capabilities].net_allow_hosts`. The host enforces the
  list at every call site (sync throw on mismatch).
- **Consent flow** — first enable surfaces the allow-list in the
  consent modal; the user can deny, approve as-is, or dial down to
  `net = "none"` to disable network entirely.

## Files

```
url-fetch/
├── manifest.toml
└── script.ts
```

## Build recipe

Same as the other examples.

## Capabilities used

| Axis | Declared | Why |
|---|---|---|
| `fs` | `none` | No file access |
| `net` | `specific` | Only `example.com` + `httpbin.org` |
| `ui` | `tool` | Two toolbar tools |
| `beacon` | `none` | No AI hooks |

## Notes on the security model

Even though the manifest declares the allow-list, the *user* can dial it
down at consent time. If they approve with the list empty, every fetch
throws — your plugin must handle that gracefully (this example does, via
the `try/catch` around each call). Plan for `slab.fetch` to fail
synchronously when the user has restricted you.

## License

MIT.
