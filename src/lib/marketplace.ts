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
  // -------- v2 fields (Workshop Marketplace, schema_version=2) --------
  // All four are optional on the wire: a v1 entry deserialises as a v2
  // entry with empty arrays / installs=0. UI code should always default
  // these defensively (use `?? []` / `?? 0`) so it works on either schema.
  /** Browsable taxonomy (e.g. ["Editing", "Productivity"]). */
  categories?: string[];
  /** Free-form discovery keywords (e.g. ["redact", "pii", "privacy"]). */
  tags?: string[];
  /** Optional preview image URLs. Up to 5 typically. */
  screenshots?: string[];
  /** Aggregate install counter (server-curated; 0 = no data). */
  installs?: number;
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
  /** v2.0.2 Workshop Marketplace — network+cache both unavailable;
   *  the binary's embedded seed index is being served. Optional on the
   *  wire so older backends that don't emit it still parse cleanly. */
  is_embedded_seed?: boolean;
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
  /** v2.0.2 Workshop Marketplace — true when the bundled seed-index
   *  baked into the binary is being shown (network + cache both
   *  unavailable). UI shows a "showing built-in plugins — connect to
   *  see more" banner. */
  isEmbeddedSeed: boolean;
  /** Human-readable error string when `phase === 'error'` (or the
   *  underlying network error when `isStale` / `isEmbeddedSeed`). */
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
  isEmbeddedSeed: false,
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
        isEmbeddedSeed: !!result.is_embedded_seed,
        error: result.is_stale || result.is_embedded_seed ? result.error : null,
        loadedAt: Date.now(),
      }));
    } else {
      marketplaceStore.update((s) => ({
        ...s,
        phase: "error",
        index: null,
        isStale: false,
        isEmbeddedSeed: false,
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

// ─── Install log read surface (v3.39 Slice 55) ──────────────────────

/**
 * One persisted row in the marketplace install-log table. Mirrors
 * `marketplace::install_log::InstallEvent` on the Rust side.
 *
 * The NULL-able fields are populated only on the row kinds that need
 * them: `bytes_written` / `files_extracted` / `source` /
 * `replaced_existing` arrive on install/update rows; `error_msg`
 * arrives on failed rows; `prior_version` arrives on update rows.
 */
export interface InstallEvent {
  id: number;
  plugin_id: string;
  version: string;
  action: "install" | "update" | "uninstall" | "failed";
  /** Unix seconds (UTC). */
  occurred_at: number;
  source: string | null;
  bytes_written: number | null;
  files_extracted: number | null;
  replaced_existing: boolean | null;
  prior_version: string | null;
  error_msg: string | null;
}

/**
 * Per-plugin counts of each install-log action kind. Mirrors
 * `marketplace::install_log::InstallStats`.
 */
export interface InstallStats {
  installs: number;
  updates: number;
  uninstalls: number;
  failures: number;
}

const EMPTY_INSTALL_STATS: InstallStats = {
  installs: 0,
  updates: 0,
  uninstalls: 0,
  failures: 0,
};

/**
 * Per-plugin timeline of install/update/uninstall/failure events,
 * newest first, capped at `limit` (default 50). Returns an empty
 * array in browser mode and for unknown plugin ids.
 *
 * Used by PluginDetailDrawer's Activity section.
 */
export async function listInstallEvents(
  pluginId: string,
  limit?: number,
): Promise<InstallEvent[]> {
  if (!isInTauri()) return [];
  return invoke<InstallEvent[]>("slab_marketplace_install_events", {
    pluginId,
    limit: limit ?? null,
  });
}

/**
 * Corpus-wide recent install events, newest first, capped at `limit`
 * (default 50). Drives the PluginsPanel toolbar "Recent installs"
 * drawer. Returns an empty array in browser mode.
 */
export async function listRecentInstallEvents(limit?: number): Promise<InstallEvent[]> {
  if (!isInTauri()) return [];
  return invoke<InstallEvent[]>("slab_marketplace_install_history_recent", {
    limit: limit ?? null,
  });
}

/**
 * Per-plugin counts of each install-log action kind. Returns an
 * all-zeroes payload in browser mode or for an unknown plugin id.
 *
 * Powers the slim header pill on PluginDetailDrawer's Activity
 * section ("Installed 3 · 1 update · 0 failures") in one round-trip.
 */
export async function pluginInstallStats(pluginId: string): Promise<InstallStats> {
  if (!isInTauri()) return { ...EMPTY_INSTALL_STATS };
  return invoke<InstallStats>("slab_marketplace_plugin_install_stats", { pluginId });
}

/**
 * Format an `InstallEvent.occurred_at` (unix seconds) as a compact
 * human-friendly relative timestamp ("just now", "3m ago", "2h ago",
 * "5d ago"), falling back to ISO yyyy-mm-dd for events older than 30
 * days. UTC arithmetic so the result is timezone-stable.
 *
 * Used by both the Activity rows and the Recent installs drawer so
 * they share one timestamp vocabulary.
 */
export function formatInstallEventTime(occurredAt: number, now?: number): string {
  const nowSec = Math.floor((now ?? Date.now()) / 1000);
  const delta = Math.max(0, nowSec - occurredAt);
  if (delta < 60) return "just now";
  if (delta < 60 * 60) return `${Math.floor(delta / 60)}m ago`;
  if (delta < 60 * 60 * 24) return `${Math.floor(delta / 3600)}h ago`;
  if (delta < 60 * 60 * 24 * 30) return `${Math.floor(delta / 86400)}d ago`;
  // ≥30 days — fall back to ISO date (UTC) so the cell is stable
  // across timezone changes.
  const iso = new Date(occurredAt * 1000).toISOString();
  return iso.slice(0, 10);
}

/**
 * Glyph hint for an install-log action, matching Slab's monochrome
 * chrome vocabulary (no emoji in app surfaces — but unicode glyphs
 * like ✓ / ✕ / ↻ / ⌫ are app-chrome and are fine).
 */
export function installEventGlyph(action: InstallEvent["action"]): string {
  switch (action) {
    case "install":
      return "✓";
    case "update":
      return "↻";
    case "uninstall":
      return "⌫";
    case "failed":
      return "✕";
  }
}

// ─── Install log retention surface (v3.39 Slice 56) ─────────────────

/**
 * Slim summary of the install log as a whole. Mirrors
 * `InstallLogSummary` on the Rust side. Drives the Recent installs
 * drawer's header "N events across X days" subtitle.
 */
export interface InstallLogSummary {
  total_events: number;
  distinct_plugins: number;
  /** Unix seconds of the oldest row, or null if the log is empty. */
  oldest_occurred_at: number | null;
}

const EMPTY_INSTALL_LOG_SUMMARY: InstallLogSummary = {
  total_events: 0,
  distinct_plugins: 0,
  oldest_occurred_at: null,
};

/**
 * Fetch a one-shot summary of the install log. Cheap (three small
 * queries) and safe to call on every drawer open. Returns the empty
 * summary in browser mode so the UI renders consistently.
 */
export async function installLogSummary(): Promise<InstallLogSummary> {
  if (!isInTauri()) return { ...EMPTY_INSTALL_LOG_SUMMARY };
  return invoke<InstallLogSummary>("slab_marketplace_install_log_summary");
}

/**
 * Trim the install log to events newer than `retainDays` days
 * before now. Returns the number of rows pruned.
 *
 * `retainDays` is clamped on the backend to a minimum of 1, so a
 * caller can't accidentally wipe the whole log via `prune(0)`.
 * Returns 0 in browser mode (no-op).
 */
export async function pruneInstallLog(retainDays: number): Promise<number> {
  if (!isInTauri()) return 0;
  return invoke<number>("slab_marketplace_install_log_prune", { retainDays });
}

/**
 * Human-friendly "log spans X days" subtitle. Returns the literal
 * "no events yet" when the summary is empty so the UI can render the
 * subtitle unconditionally without an extra empty-state branch.
 *
 * Uses ceiling-day arithmetic so a log opened 5 minutes ago still
 * reads "1 day" rather than the awkward "0 days".
 */
export function formatLogSpan(summary: InstallLogSummary, now?: number): string {
  if (summary.total_events === 0 || summary.oldest_occurred_at === null) {
    return "no events yet";
  }
  const nowSec = Math.floor((now ?? Date.now()) / 1000);
  const span = Math.max(1, Math.ceil((nowSec - summary.oldest_occurred_at) / 86_400));
  const events = `${summary.total_events} event${summary.total_events === 1 ? "" : "s"}`;
  const days = `${span} day${span === 1 ? "" : "s"}`;
  return `${events} across ${days}`;
}
