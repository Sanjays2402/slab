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
 * side-effect import below.
 */

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
