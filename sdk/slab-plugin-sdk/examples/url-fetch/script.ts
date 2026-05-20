// url-fetch — exercises slab.fetch + the per-host capability allow-list.
//
// Two tools:
//   - "Ping example.com": GET request, shows the HTTP status.
//   - "POST httpbin": shows headers + body roundtripping.
//
// The manifest declares net = "specific" with two allow-listed hosts,
// so calls to anything else throw synchronously. Users see the
// allow-list in the consent modal at first enable and can dial it
// down or deny entirely.

import { definePlugin } from "@slab/plugin-sdk";

export default definePlugin({
  id: "url-fetch",
  onLoad(slab) {
    slab.ui.registerTool({
      id: "ping-example",
      label: "Ping example.com",
      shortcut: "Ctrl+Shift+E",
      async invoke() {
        try {
          const r = await slab.fetch("https://example.com", {
            method: "GET",
            timeoutMs: 5_000,
          });
          const level = r.ok ? "info" : "warn";
          slab.ui.notify(
            `example.com → ${r.status} ${r.statusText} (${r.body.length} bytes)`,
            level,
          );
        } catch (e) {
          slab.ui.notify(`Fetch failed: ${String(e)}`, "error");
        }
      },
    });

    slab.ui.registerTool({
      id: "post-httpbin",
      label: "POST httpbin",
      async invoke() {
        try {
          const payload = { hello: "from slab", pluginId: slab.pluginId };
          const r = await slab.fetch("https://httpbin.org/post", {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify(payload),
            timeoutMs: 10_000,
          });
          if (r.ok) {
            // httpbin echoes the JSON back under "json"
            const echo = JSON.parse(r.body) as {
              json: { hello: string; pluginId: string };
            };
            slab.ui.notify(
              `httpbin echoed: hello="${echo.json.hello}" pluginId="${echo.json.pluginId}"`,
              "info",
            );
          } else {
            slab.ui.notify(`httpbin returned ${r.status}`, "warn");
          }
        } catch (e) {
          slab.ui.notify(`POST failed: ${String(e)}`, "error");
        }
      },
    });
  },
});
