/**
 * @slab/plugin-sdk — runtime helpers.
 *
 * Tiny non-type surface for authoring ergonomics. Everything else
 * in this package is types-only; these two helpers are the entire
 * runtime footprint.
 */

import type { SlabGlobal } from "./types/global";

/**
 * Plugin definition object accepted by {@link definePlugin}.
 *
 * The `id` field MUST match the plugin's `manifest.id` — the host
 * cross-checks at enable time and surfaces a clear error on
 * mismatch.
 */
export interface PluginDefinition {
  /** Stable plugin id. Must match `manifest.id` exactly. */
  id: string;
  /**
   * Optional setup hook invoked synchronously at plugin enable
   * time with the host-supplied `slab` global.
   *
   * Throwing here fails the plugin's enable transaction cleanly —
   * the user sees the throw message in the Cabinet plugin panel
   * and the plugin is marked as "enabled-but-errored".
   */
  onLoad?(slab: SlabGlobal): void;
}

/**
 * Author a Slab plugin with full IntelliSense and runtime
 * bootstrapping.
 *
 * @example
 * ```ts
 * import { definePlugin } from "@slab/plugin-sdk";
 *
 * export default definePlugin({
 *   id: "my-plugin",
 *   onLoad(slab) {
 *     slab.ui.notify(`Hello from ${slab.pluginId}!`);
 *     slab.ui.registerTool({
 *       id: "say-hi",
 *       label: "Say Hi",
 *       invoke: () => slab.ui.notify("Hi!"),
 *     });
 *   },
 * });
 * ```
 *
 * Behaviour:
 *   - Validates `id` is a non-empty string (throws otherwise).
 *   - If `globalThis.slab` is present (i.e. we're running inside
 *     the Slab QuickJS runtime) and `onLoad` is defined, invokes
 *     `onLoad(slab)` synchronously.
 *   - If `globalThis.slab` is absent (e.g. the plugin is being
 *     imported in a node unit test), the call is a no-op — the
 *     returned definition object can still be inspected.
 *
 * The returned `PluginDefinition` is the same object passed in
 * (no clone), so future host versions can extend the shape without
 * breaking callers.
 */
export function definePlugin(def: PluginDefinition): PluginDefinition {
  if (typeof def !== "object" || def === null) {
    throw new TypeError("definePlugin: argument must be a plugin definition object");
  }
  if (typeof def.id !== "string" || def.id.length === 0) {
    throw new Error("definePlugin: `id` must be a non-empty string");
  }
  // Auto-bootstrap when running inside the Slab runtime — the host
  // expects top-level side effects on script eval.
  const g = (globalThis as { slab?: SlabGlobal }).slab;
  if (g && typeof def.onLoad === "function") {
    def.onLoad(g);
  }
  return def;
}

/**
 * Runtime narrowing — returns the `slab` global if present, else
 * throws a TypeError with a clear message.
 *
 * Useful in plugin code that's also imported in node-side unit
 * tests, where `globalThis.slab` is absent and you want to fail
 * fast rather than chain `?.` operators forever.
 *
 * @example
 * ```ts
 * import { assertSlab } from "@slab/plugin-sdk";
 *
 * export function notifyAll(message: string) {
 *   const slab = assertSlab();
 *   slab.ui.notify(message);
 * }
 * ```
 */
export function assertSlab(): SlabGlobal {
  const g = (globalThis as { slab?: SlabGlobal }).slab;
  if (!g) {
    throw new TypeError(
      "slab global not found — this code only runs inside the Slab plugin runtime",
    );
  }
  return g;
}

/**
 * Soft accessor — returns the `slab` global if present, else
 * `undefined`. Mirrors `assertSlab()` but for hot paths that
 * shouldn't throw.
 */
export function trySlab(): SlabGlobal | undefined {
  return (globalThis as { slab?: SlabGlobal }).slab;
}
