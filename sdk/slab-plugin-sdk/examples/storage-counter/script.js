// script.ts
import { definePlugin } from "@slab/plugin-sdk";
var COUNT_KEY = "open_count";
var script_default = definePlugin({
  id: "com.slab.examples.storage-counter",
  onLoad(slab) {
    slab.document.onOpen(async (doc) => {
      const prev = Number(await slab.storage.get(COUNT_KEY) ?? "0");
      const next = prev + 1;
      await slab.storage.set(COUNT_KEY, String(next));
      slab.ui.notify(`Opened "${doc.name}" \u2014 total opens: ${next}`, "info");
    });
    slab.ui.registerTool({
      id: "show-stats",
      label: "Show Open Count",
      shortcut: "Ctrl+Shift+O",
      async invoke() {
        const count = Number(await slab.storage.get(COUNT_KEY) ?? "0");
        const usage = await slab.storage.usage();
        slab.ui.notify(
          `You've opened ${count} PDF(s). Storage used: ${usage.bytes} bytes across ${usage.keys} keys.`,
          "info"
        );
      }
    });
    slab.ui.registerTool({
      id: "reset-stats",
      label: "Reset Open Count",
      async invoke() {
        await slab.storage.clear();
        slab.ui.notify("Counter reset to 0.", "warn");
      }
    });
  }
});
export {
  script_default as default
};
