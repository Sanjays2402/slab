// Slab marketplace runtime — v1.4.0 "Bench" Slice 6.
//
// TypeScript layer over `slab_marketplace_*` Tauri commands plus a
// Svelte store that drives the Browse tab in PluginsPanel.svelte.
// Every UI access goes through this module — no scattered
// `invoke("slab_marketplace_*")` calls in components.
//
// Design mirrors `$lib/plugins`: small surface, browser-mode safe
// (no-ops outside Tauri so `pnpm dev` keeps working), per-id busy
// flags so the UI can disable individual cards while their action
// is in flight.

import { invoke } from "@tauri-apps/api/core";
import { writable, get } from "svelte/store";
import { isInTauri } from "$lib/tauri";

// ---------- type mirrors (must match Rust serde output) ----------

/** One entry from the curated marketplace index. */
export interface IndexEntry {
  id: string;
  name: string;
  version: string;
  description: string;
  author: string;
  download_url: string;
  /** Hex-encoded SHA-256 of the tarball bytes. */
  sha256: string;
  /** Tarball size in bytes (UI surfaces this on cards). */
  size_bytes: number;
  /** SemVer requirement against host Slab version, e.g. ">=1.4.0". */
  slab_compat: string;
  /** Base64-encoded Ed25519 signature over the canonical unsigned form. */
  signature: string;
}

/** Top-level index envelope. */
export interface Index {
  schema_version: number;
  signing_key_id: string;
  plugins: IndexEntry[];
}

/** Result of a successful install — mirrors `marketplace::InstallReport`. */
export interface InstallReport {
  id: string;
  version: string;
  installed_at: string;
  bytes_written: number;
  files_extracted: number;
  replaced_existing: boolean;
}

/**
 * Flat outcome shape returned by `slab_marketplace_index` — easier to
 * consume from Svelte than the Rust tagged FetchOutcome enum.
 */
export interface MarketplaceFetchResult {
  is_fresh: boolean;
  is_stale: boolean;
  index: Index | null;
  error: string | null;
}

// ---------- store ----------

/**
 * Top-level marketplace state. The Browse tab subscribes to this and
 * derives its render decisions from `state`. Single-store keeps the
 * mental model simple — no separate "loading flag", "current index",
 * "error" stores to keep in sync.
 */
export type MarketplacePhase = "idle" | "loading" | "ready" | "error";

export interface MarketplaceState {
  /** Lifecycle. `idle` = never tried; `loading` = refresh in flight;
   *  `ready` = `index` is populated (possibly stale); `error` = no
   *  usable index. */
  phase: MarketplacePhase;
  /** The fetched (or cached) index when `phase === 'ready'`. */
  index: Index | null;
  /** True when the index came from the offline cache because the
   *  network failed this call. UI shows a "showing cached results"
   *  banner with a Refresh button. */
  isStale: boolean;
  /** Human-readable error string when `phase === 'error'` (or the
   *  underlying network error when `isStale`). */
  error: string | null;
  /** ms-epoch of the last successful refresh attempt. 0 = never. */
  loadedAt: number;
  /** Per-id "action in flight" set so the UI can disable a card while
   *  its install / uninstall runs. */
  busy: Record<string, true>;
}

const EMPTY: MarketplaceState = {
  phase: "idle",
  index: null,
  isStale: false,
  error: null,
  loadedAt: 0,
  busy: {},
};

export const marketplaceStore = writable<MarketplaceState>({ ...EMPTY });

// ---------- commands ----------

/** True iff the marketplace is reachable (we're inside Tauri). */
export function marketplaceAvailable(): boolean {
  return isInTauri();
}

/**
 * Refresh the index from the network with offline-cache fallback. Safe
 * to call repeatedly; idempotent. Outside Tauri this is a no-op that
 * leaves the store in `idle` so the panel can render its
 * "marketplace not available in browser" empty state.
 *
 * Never throws — failures land in the `error` / `isStale` fields so
 * callers can render them without wrapping in try/catch.
 */
export async function refreshMarketplace(): Promise<void> {
  if (!isInTauri()) {
    marketplaceStore.set({ ...EMPTY });
    return;
  }
  marketplaceStore.update((s) => ({ ...s, phase: "loading", error: null }));
  try {
    const result = await invoke<MarketplaceFetchResult>("slab_marketplace_index");
    if (result.index) {
      marketplaceStore.update((s) => ({
        ...s,
        phase: "ready",
        index: result.index,
        isStale: result.is_stale,
        error: result.is_stale ? result.error : null,
        loadedAt: Date.now(),
      }));
    } else {
      marketplaceStore.update((s) => ({
        ...s,
        phase: "error",
        index: null,
        isStale: false,
        error: result.error ?? "Failed to fetch marketplace index",
        loadedAt: Date.now(),
      }));
    }
  } catch (e) {
    marketplaceStore.update((s) => ({
      ...s,
      phase: "error",
      error: e instanceof Error ? e.message : String(e),
      loadedAt: Date.now(),
    }));
  }
}

/**
 * Install (or update) the plugin described by `entry`. Backend
 * re-verifies the signature against the maintainer key as
 * defence-in-depth before touching the network — the frontend doesn't
 * need to pre-filter.
 *
 * Sets a per-id busy flag while in flight so the card can show a
 * spinner and disable double-clicks. On success, the backend has
 * already re-discovered the plugin registry, so callers should
 * follow up with `refreshPlugins()` from `$lib/plugins` to repopulate
 * the Installed tab.
 */
export async function installPlugin(entry: IndexEntry): Promise<InstallReport> {
  if (!isInTauri()) {
    throw new Error("Marketplace is only available in the Slab desktop app");
  }
  marketplaceStore.update((s) => ({ ...s, busy: { ...s.busy, [entry.id]: true } }));
  try {
    return await invoke<InstallReport>("slab_marketplace_install", { entry });
  } finally {
    marketplaceStore.update((s) => {
      const { [entry.id]: _removed, ...rest } = s.busy;
      return { ...s, busy: rest };
    });
  }
}

/**
 * Uninstall a plugin by id. Returns `true` if a plugin was removed,
 * `false` if no install was found (idempotent — safe to call on
 * already-uninstalled ids).
 */
export async function uninstallPluginById(id: string): Promise<boolean> {
  if (!isInTauri()) return false;
  marketplaceStore.update((s) => ({ ...s, busy: { ...s.busy, [id]: true } }));
  try {
    return await invoke<boolean>("slab_marketplace_uninstall", { id });
  } finally {
    marketplaceStore.update((s) => {
      const { [id]: _removed, ...rest } = s.busy;
      return { ...s, busy: rest };
    });
  }
}

/** Sync accessor — read the current state without subscribing. */
export function currentMarketplace(): MarketplaceState {
  return get(marketplaceStore);
}

/**
 * Format a byte count for card display. Mirrors the human-friendly
 * format the rest of Slab uses (e.g. file size in the Library panel).
 */
export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / (1024 * 1024)).toFixed(2)} MB`;
}

/**
 * Compare two semver-ish strings (`major.minor.patch[-prerelease]`).
 * Returns:
 * - negative when `a < b`
 * - 0 when equal
 * - positive when `a > b`
 *
 * Tolerant: missing components are treated as 0; non-numeric segments
 * sort lexicographically. Pre-release tags sort lower than the same
 * version without one (so `1.0.0-rc1` < `1.0.0`).
 *
 * Used to decide whether an installed plugin has an update available.
 */
export function compareSemver(a: string, b: string): number {
  const split = (s: string): { core: number[]; pre: string | null } => {
    const dash = s.indexOf("-");
    const core = dash >= 0 ? s.slice(0, dash) : s;
    const pre = dash >= 0 ? s.slice(dash + 1) : null;
    return { core: core.split(".").map((n) => parseInt(n, 10) || 0), pre };
  };
  const pa = split(a);
  const pb = split(b);
  const len = Math.max(pa.core.length, pb.core.length);
  for (let i = 0; i < len; i++) {
    const x = pa.core[i] ?? 0;
    const y = pb.core[i] ?? 0;
    if (x !== y) return x - y;
  }
  // Same core → handle pre-release rules.
  if (pa.pre === null && pb.pre === null) return 0;
  if (pa.pre === null) return 1; // release > prerelease
  if (pb.pre === null) return -1;
  return pa.pre < pb.pre ? -1 : pa.pre > pb.pre ? 1 : 0;
}
