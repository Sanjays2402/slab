/**
 * @slab/plugin-sdk — public surface.
 *
 * Plugin authors typically only need:
 *
 * ```ts
 * import { definePlugin } from "@slab/plugin-sdk";
 *
 * export default definePlugin({
 *   id: "my-plugin",
 *   onLoad(slab) {
 *     slab.ui.notify("Hello world");
 *   },
 * });
 * ```
 *
 * The full type catalogue is also re-exported per domain. The
 * `globalThis.slab` ambient declaration is installed by the
 * `declare global` block at the bottom of this file — importing
 * *anything* from this package pulls it into scope.
 */

// Runtime helpers — the only non-type exports.
export { definePlugin, assertSlab, trySlab } from "./define";
export type { PluginDefinition } from "./define";

// Re-export every domain type module by name so authors can do
// `import type { BeaconTool } from "@slab/plugin-sdk";` without
// reaching into deep paths.
export type * from "./types/manifest";
export type * from "./types/beacon";
export type * from "./types/ui";
export type * from "./types/document";
export type * from "./types/storage";
export type * from "./types/fetch";
export type { SlabGlobal } from "./types/global";

import type { SlabGlobal as _SlabGlobalForAmbient } from "./types/global";

declare global {
  /**
   * The Slab plugin runtime global. Available at top level inside
   * any plugin `script.js` (or `.ts` that compiles to it).
   *
   * Outside the Slab runtime (e.g. in a node-side unit test) the
   * declaration still typechecks but the value is `undefined` — use
   * {@link assertSlab} or {@link trySlab} for runtime narrowing.
   */
  // eslint-disable-next-line no-var
  var slab: _SlabGlobalForAmbient;
}
