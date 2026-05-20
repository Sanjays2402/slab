// script.ts
import { definePlugin } from "@slab/plugin-sdk";
var script_default = definePlugin({
  id: "com.slab.examples.url-fetch",
  onLoad(slab) {
    slab.ui.registerTool({
      id: "ping-example",
      label: "Ping example.com",
      shortcut: "Ctrl+Shift+E",
      async invoke() {
        try {
          const r = await slab.fetch("https://example.com", {
            method: "GET",
            timeoutMs: 5e3
          });
          const level = r.ok ? "info" : "warn";
          slab.ui.notify(
            `example.com \u2192 ${r.status} ${r.statusText} (${r.body.length} bytes)`,
            level
          );
        } catch (e) {
          slab.ui.notify(`Fetch failed: ${String(e)}`, "error");
        }
      }
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
            timeoutMs: 1e4
          });
          if (r.ok) {
            const echo = JSON.parse(r.body);
            slab.ui.notify(
              `httpbin echoed: hello="${echo.json.hello}" pluginId="${echo.json.pluginId}"`,
              "info"
            );
          } else {
            slab.ui.notify(`httpbin returned ${r.status}`, "warn");
          }
        } catch (e) {
          slab.ui.notify(`POST failed: ${String(e)}`, "error");
        }
      }
    });
  }
});
export {
  script_default as default
};
