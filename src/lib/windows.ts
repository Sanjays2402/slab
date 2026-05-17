// Cabinet (v1.1.0) — typed wrapper around `slab_window_*` Tauri commands.
//
// The frontend never invokes `slab_window_open/close/list` directly; this
// module is the single entry point. It transparently:
//   - falls back to no-op when running outside Tauri (dev `pnpm dev` shell,
//     SSR, vitest) so consumers don't have to guard each call site;
//   - converts the Rust snake_case (`panel_id`, `target_doc`) to camelCase
//     on the way out so the rest of the Svelte tree gets a clean API.
//
// Public surface:
//   - openPanelWindow(panelId, targetDoc?): spawn a detached window for a
//     panel. Returns the label assigned by the registry, or null on failure.
//   - closePanelWindow(label): graceful close + drop from registry.
//   - listPanelWindows(): snapshot of all currently-open detached windows.
//   - focusPanelWindow(label): bring a detached window forward (used by
//     the upcoming Windows menu in Slice 7).

import { invoke } from "@tauri-apps/api/core";
import { isInTauri } from "./tauri";

export type Geometry = {
  x: number;
  y: number;
  width: number;
  height: number;
};

export type WindowState = {
  label: string;
  panelId: string;
  geometry: Geometry;
  targetDoc?: string | null;
};

/** Backend shape — snake_case fields straight from serde. */
type RawWindowState = {
  label: string;
  panel_id: string;
  geometry: Geometry;
  target_doc?: string | null;
};

function fromRaw(r: RawWindowState): WindowState {
  return {
    label: r.label,
    panelId: r.panel_id,
    geometry: r.geometry,
    targetDoc: r.target_doc ?? null,
  };
}

/**
 * Open a panel in its own native window. Pass `targetDoc` to seed the
 * detached panel with a specific file path (used by Reader and Library).
 *
 * Returns the assigned window label (e.g. `panel-beacon-2`), or `null`
 * if we're outside Tauri or the spawn failed. Never throws.
 */
export async function openPanelWindow(
  panelId: string,
  targetDoc?: string | null,
): Promise<string | null> {
  if (!isInTauri()) {
    console.warn(
      `[cabinet] openPanelWindow("${panelId}") ignored — not running in Tauri`,
    );
    return null;
  }
  try {
    const label = await invoke<string>("slab_window_open", {
      panelId,
      targetDoc: targetDoc ?? null,
    });
    return label;
  } catch (e) {
    console.error(`[cabinet] openPanelWindow("${panelId}") failed:`, e);
    return null;
  }
}

/** Close a detached window by label. Idempotent — closing an unknown
 *  label is a no-op. */
export async function closePanelWindow(label: string): Promise<void> {
  if (!isInTauri()) return;
  try {
    await invoke("slab_window_close", { label });
  } catch (e) {
    console.error(`[cabinet] closePanelWindow("${label}") failed:`, e);
  }
}

/** List all currently-open detached panel windows, sorted by label. */
export async function listPanelWindows(): Promise<WindowState[]> {
  if (!isInTauri()) return [];
  try {
    const raw = await invoke<RawWindowState[]>("slab_window_list");
    return raw.map(fromRaw);
  } catch (e) {
    console.error("[cabinet] listPanelWindows failed:", e);
    return [];
  }
}

/**
 * Bring a detached window to the front. Uses the `@tauri-apps/api/webviewWindow`
 * API to set focus by label. Lazy-imports the module so non-Tauri builds
 * never pay the cost.
 */
export async function focusPanelWindow(label: string): Promise<void> {
  if (!isInTauri()) return;
  try {
    const mod = await import("@tauri-apps/api/webviewWindow");
    const w = await mod.WebviewWindow.getByLabel(label);
    await w?.setFocus();
  } catch (e) {
    console.error(`[cabinet] focusPanelWindow("${label}") failed:`, e);
  }
}
