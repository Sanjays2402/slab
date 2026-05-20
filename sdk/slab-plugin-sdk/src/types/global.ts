/**
 * @slab/plugin-sdk — the `slab` global.
 *
 * The host injects this object as `globalThis.slab` into every
 * plugin script's QuickJS context. Importing `SlabGlobal` from this
 * module lets you author functions that take `slab` as a parameter
 * (the recommended testable pattern) while still getting full
 * IntelliSense.
 *
 * For the `globalThis.slab` ambient declaration (so you can use
 * `slab.ui.notify(...)` at top level without importing anything),
 * see `src/ambient.d.ts`.
 */

import type { BeaconSurface } from "./beacon";
import type { DocumentSurface } from "./document";
import type { SlabFetch } from "./fetch";
import type { StorageSurface } from "./storage";
import type { UiSurface } from "./ui";

/**
 * The `slab` global injected by the host into every plugin script.
 *
 * The shape is stable across slices — surfaces gain capabilities
 * but the top-level keys are forward-compatible. Plugins target a
 * specific `slab_compat` semver range in their manifest.
 */
export interface SlabGlobal {
  /** AI / Beacon surface — tool + AI provider registration. */
  beacon: BeaconSurface;
  /** UI surface — panels, toolbar tools, notifications. */
  ui: UiSurface;
  /** Document lifecycle — open/close events, getActive. */
  document: DocumentSurface;
  /** Per-plugin sqlite-backed key/value store. */
  storage: StorageSurface;
  /** Host-mediated HTTP fetch. */
  fetch: SlabFetch;
  /**
   * The plugin's own id (matches `manifest.id`). Useful for log
   * prefixes and constructing unambiguous identifiers within the
   * plugin's own slice of the key namespace.
   */
  readonly pluginId: string;
}
