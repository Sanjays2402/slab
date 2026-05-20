// hello-workshop — smallest possible Slab v2.0.0 plugin.
//
// Registers a single UI tool, "Say Hi", that emits a toast notification
// when the user clicks it or hits the assigned shortcut.
//
// This file is authored in TypeScript for IntelliSense, then bundled to
// a plain ES module (script.js) for the host runtime. The host doesn't
// execute TypeScript directly — see the README for the build recipe.

import { definePlugin } from "@slab/plugin-sdk";

export default definePlugin({
  id: "hello-workshop",
  onLoad(slab) {
    slab.ui.notify(`Hello from ${slab.pluginId}!`);
    slab.ui.registerTool({
      id: "say-hi",
      label: "Say Hi",
      shortcut: "Ctrl+Shift+H",
      invoke() {
        slab.ui.notify("👋 Hi from a plugin!", "info");
      },
    });
  },
});
