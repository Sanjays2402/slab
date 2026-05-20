// storage-counter — counts how many PDFs you've opened across sessions.
//
// Demonstrates:
//   - slab.storage.{get,set,usage,clear} — per-plugin persistent KV
//   - slab.document.onOpen — lifecycle event hook
//   - slab.ui.registerTool — a tool to reset the counter
//
// The counter increments every time you open a PDF and persists to
// ~/.slab/plugin-storage.sqlite (scoped to this plugin's slice — no
// other plugin can read or modify the value).

import { definePlugin } from "@slab/plugin-sdk";

const COUNT_KEY = "open_count";

export default definePlugin({
  id: "storage-counter",
  onLoad(slab) {
    slab.document.onOpen(async (doc) => {
      const prev = Number((await slab.storage.get(COUNT_KEY)) ?? "0");
      const next = prev + 1;
      await slab.storage.set(COUNT_KEY, String(next));
      slab.ui.notify(`Opened "${doc.name}" — total opens: ${next}`, "info");
    });

    slab.ui.registerTool({
      id: "show-stats",
      label: "Show Open Count",
      shortcut: "Ctrl+Shift+O",
      async invoke() {
        const count = Number((await slab.storage.get(COUNT_KEY)) ?? "0");
        const usage = await slab.storage.usage();
        slab.ui.notify(
          `You've opened ${count} PDF(s). Storage used: ${usage.bytes} bytes across ${usage.keys} keys.`,
          "info",
        );
      },
    });

    slab.ui.registerTool({
      id: "reset-stats",
      label: "Reset Open Count",
      async invoke() {
        await slab.storage.clear();
        slab.ui.notify("Counter reset to 0.", "warn");
      },
    });
  },
});
