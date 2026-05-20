// Throwaway typecheck-only test — verifies that the public surface of
// @slab/plugin-sdk gives plugin authors the right IntelliSense and
// catches the right errors.
//
// Run with: tsc --noEmit tests/typecheck-smoke.ts
//
// This file lives outside the published package (it's excluded from
// the package.json `files` field) so users never see it.

import {
  definePlugin,
  assertSlab,
  trySlab,
  type BeaconTool,
  type UiPanel,
  type UiTool,
  type StorageUsage,
  type SlabFetchInit,
  type SlabFetchResponse,
  type DocumentHandler,
  type SlabGlobal,
  type ManifestCapabilities,
  type PluginDefinition,
} from "../src/index";

// --- definePlugin happy path ---
const _hello: PluginDefinition = definePlugin({
  id: "hello-test",
  onLoad(slab) {
    // slab parameter inferred as SlabGlobal — every surface accessible.
    slab.ui.notify("loaded");
    slab.ui.notify("loaded with level", "warn");
    slab.ui.notify("err", "error");

    // beacon surface
    const tool: BeaconTool = {
      id: "echo",
      name: "Echo",
      description: "Returns the input as-is",
      parameters: { type: "object", properties: { msg: { type: "string" } } },
      async run(input) {
        return input;
      },
    };
    slab.beacon.registerTool(tool);

    slab.beacon.registerAiProvider({
      id: "my-llama",
      label: "My Llama",
      kind: "ollama",
      base_url: "http://localhost:11434",
      default_model: "llama3",
    });

    // ui surface
    const panel: UiPanel = {
      id: "stats",
      label: "Stats",
      render(root: HTMLElement) {
        root.textContent = `pluginId=${slab.pluginId}`;
        return () => {
          root.textContent = "";
        };
      },
    };
    slab.ui.registerPanel(panel);

    const uiTool: UiTool = {
      id: "say-hi",
      label: "Say Hi",
      shortcut: "Ctrl+Shift+H",
      invoke: () => slab.ui.notify("Hi!"),
    };
    slab.ui.registerTool(uiTool);

    // document surface
    const onOpen: DocumentHandler = async (doc) => {
      slab.ui.notify(`Opened: ${doc.path}`);
      // doc.path : string, doc.name : string
      const _len: number = doc.name.length;
      void _len;
    };
    slab.document.onOpen(onOpen);
    slab.document.onClose((doc) => {
      slab.ui.notify(`Closed: ${doc.name}`);
    });
    const active = slab.document.getActive();
    if (active) {
      slab.ui.notify(`Active: ${active.name}`);
    }

    // storage surface
    slab.storage
      .set("greet", "hi")
      .then(() => slab.storage.get("greet"))
      .then((v) => {
        // v : string | null
        if (v !== null) slab.ui.notify(`stored ${v}`);
      });
    slab.storage.list().then((keys: string[]) => {
      slab.ui.notify(`have ${keys.length} keys`);
    });
    slab.storage.usage().then((u: StorageUsage) => {
      slab.ui.notify(`using ${u.bytes} bytes across ${u.keys} keys`);
    });
    slab.storage.remove("greet").then((existed: boolean) => {
      slab.ui.notify(`existed: ${existed}`);
    });
    slab.storage.clear();

    // fetch surface
    const init: SlabFetchInit = {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ hello: "world" }),
      timeoutMs: 5_000,
    };
    slab.fetch("https://example.com/api", init).then((r: SlabFetchResponse) => {
      if (r.ok) slab.ui.notify(`fetched ${r.url} → ${r.status}`);
    });
  },
});
void _hello;

// --- assertSlab / trySlab ---
function useGlobal() {
  const g = assertSlab();
  g.ui.notify("via assertSlab");
}
void useGlobal;

function maybeUseGlobal() {
  const g = trySlab();
  if (g) g.ui.notify("via trySlab");
}
void maybeUseGlobal;

// --- type-level: ManifestCapabilities lattice is correct ---
const _caps: ManifestCapabilities = {
  fs: "read",
  net: "specific",
  ui: "both",
  beacon: "tool-provider",
  net_allow_hosts: ["example.com"],
  fs_allow_paths: ["~/.slab"],
};
void _caps;

// --- type-level: globalThis.slab ambient is reachable ---
function useAmbient() {
  // No import needed — `slab` is ambient from index.ts's declare global.
  // We cast through `globalThis` in this test file because we're outside
  // src/ and the test's own scope doesn't inherit the side-effect import,
  // but in real plugin code that imports from "@slab/plugin-sdk", the
  // ambient propagates.
  const g: SlabGlobal = (globalThis as { slab: SlabGlobal }).slab;
  g.ui.notify("ambient");
}
void useAmbient;

// --- negative tests: these should fail to typecheck (proves the
// types are actually constraining). Each line is preceded by an
// expect-error directive.
const _negTests = () => {
  // @ts-expect-error invalid NotifyLevel
  assertSlab().ui.notify("bad", "verbose");

  assertSlab().beacon.registerAiProvider({
    id: "x",
    label: "x",
    // @ts-expect-error invalid kind
    kind: "magic",
    base_url: "x",
    default_model: "x",
  });
};
void _negTests;
