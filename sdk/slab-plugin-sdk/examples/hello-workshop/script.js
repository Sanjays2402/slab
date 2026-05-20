// script.ts
import { definePlugin } from "@slab/plugin-sdk";
var script_default = definePlugin({
  id: "com.slab.examples.hello-workshop",
  onLoad(slab) {
    slab.ui.notify(`Hello from ${slab.pluginId}!`);
    slab.ui.registerTool({
      id: "say-hi",
      label: "Say Hi",
      shortcut: "Ctrl+Shift+H",
      invoke() {
        slab.ui.notify("\u{1F44B} Hi from a plugin!", "info");
      }
    });
  }
});
export {
  script_default as default
};
