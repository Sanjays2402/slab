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
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
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

// ─── Install log filter surface (v3.39 Slice 75) ────────────────────

/**
 * The complete InstallAction vocabulary as a readonly tuple. Useful for
 * exhaustive UI iteration (filter chips, action legends) — order is the
 * "natural" reading order matching how the Recent installs drawer
 * lists action filter chips (success-shaped first, failure-shaped last).
 */
export const ALL_INSTALL_ACTIONS: readonly InstallEvent["action"][] = [
  "install",
  "update",
  "uninstall",
  "failed",
] as const;

/**
 * Four-axis filter for the Recent installs drawer. Mirrors the
 * Rust-side `slab_marketplace_install_log_list_filtered` command:
 *
 * - `since_unix` / `until_unix`: optional inclusive time-window
 *   boundaries. `null`/missing == no bound on that side.
 * - `actions`: optional subset of `ALL_INSTALL_ACTIONS`. An empty
 *   array OR `null`/missing both mean "no action filter". Unknown
 *   tokens are silently dropped server-side so a typo can't widen
 *   the result.
 * - `plugin_id_substr`: optional case-insensitive substring against
 *   plugin_id. Whitespace-only strings are treated as "no filter"
 *   (the backend trims+normalises before matching).
 * - `limit`: row cap. Defaults to 500 server-side.
 *
 * All four axes compose via AND so a "Last 7d failures for
 * com.acme.\*" query reads naturally.
 */
export interface InstallEventQuery {
  since_unix?: number | null;
  until_unix?: number | null;
  actions?: readonly InstallEvent["action"][] | null;
  plugin_id_substr?: string | null;
  limit?: number | null;
}

/**
 * Wire payload returned by [`listInstallEventsFiltered`].
 * `total_returned` is the row count actually delivered; `limit_used`
 * echoes the effective limit so the UI can render a "Limit reached
 * (500) — narrow the filter to see more" hint when the two are equal.
 */
export interface InstallEventFilteredResult {
  events: InstallEvent[];
  total_returned: number;
  limit_used: number;
}

const EMPTY_QUERY: InstallEventQuery = {};
const EMPTY_FILTERED_RESULT: InstallEventFilteredResult = {
  events: [],
  total_returned: 0,
  limit_used: 0,
};

/**
 * Read the install log with the four-axis filter. Returns an empty
 * result in browser mode (no Tauri context) so the drawer's
 * `filteredEvents` reducer doesn't have to special-case the dev
 * environment.
 */
export async function listInstallEventsFiltered(
  query: InstallEventQuery = EMPTY_QUERY,
): Promise<InstallEventFilteredResult> {
  if (!isInTauri()) return { ...EMPTY_FILTERED_RESULT };
  return invoke<InstallEventFilteredResult>(
    "slab_marketplace_install_log_list_filtered",
    {
      sinceUnix: query.since_unix ?? null,
      untilUnix: query.until_unix ?? null,
      actions: query.actions && query.actions.length > 0 ? [...query.actions] : null,
      pluginIdSubstr: query.plugin_id_substr ?? null,
      limit: query.limit ?? null,
    },
  );
}

/**
 * Return the most-recently-active distinct plugin_ids in the install
 * log, newest activity first. Powers the filter bar's plugin
 * autocomplete in the Recent installs drawer. Returns an empty array
 * in browser mode.
 */
export async function recentInstallPluginIds(limit?: number): Promise<string[]> {
  if (!isInTauri()) return [];
  return invoke<string[]>("slab_marketplace_install_log_recent_plugin_ids", {
    limit: limit ?? null,
  });
}

// ─── Per-plugin histogram aggregate (v3.40 Slice 87) ────────────────

/**
 * One row of the per-plugin histogram aggregate. Mirrors
 * `marketplace::PluginHistogramRow`. `total` is precomputed so the UI's
 * bar-width and sort don't have to re-add four columns per row;
 * `last_occurred_at` is the unix-seconds of this plugin's most recent
 * event within the queried window.
 */
export interface PluginHistogramRow {
  plugin_id: string;
  installs: number;
  updates: number;
  uninstalls: number;
  failures: number;
  total: number;
  last_occurred_at: number;
}

/**
 * Wire payload returned by [`getPluginInstallHistogram`]. Echoes the
 * effective window + limit so the UI can render "Showing top 25 plugins
 * in the last 30d" without remembering which defaults ran when the
 * caller left the args unset. `grand_total` is the sum of every row's
 * `total` — the corpus-wide event count within the window across all
 * returned plugins.
 */
export interface PluginHistogramResult {
  rows: PluginHistogramRow[];
  since_unix: number | null;
  until_unix: number | null;
  limit_used: number;
  grand_total: number;
}

const EMPTY_HISTOGRAM_RESULT: PluginHistogramResult = {
  rows: [],
  since_unix: null,
  until_unix: null,
  limit_used: 0,
  grand_total: 0,
};

/**
 * Aggregate the install log by plugin_id within an optional time
 * window. Returns the per-plugin counts ordered by total activity
 * descending, capped at `limit` (default 25). Powers the Recent
 * installs drawer's "Top plugins" panel — the answer to "which
 * plugins did I install the most this month?". Returns an empty
 * result in browser mode.
 */
export async function getPluginInstallHistogram(opts: {
  sinceUnix?: number | null;
  untilUnix?: number | null;
  limit?: number | null;
} = {}): Promise<PluginHistogramResult> {
  if (!isInTauri()) return { ...EMPTY_HISTOGRAM_RESULT };
  return invoke<PluginHistogramResult>(
    "slab_marketplace_install_log_plugin_histogram",
    {
      sinceUnix: opts.sinceUnix ?? null,
      untilUnix: opts.untilUnix ?? null,
      limit: opts.limit ?? null,
    },
  );
}

/**
 * Compose a one-line summary copy for the histogram header. Returns
 * a friendly empty-state string when no events were aggregated.
 */
export function summarizeHistogram(result: PluginHistogramResult): string {
  const n = result.rows.length;
  if (n === 0) return "No plugins active in this window";
  const events = result.grand_total;
  return `${events} event${events === 1 ? "" : "s"} across ${n} plugin${n === 1 ? "" : "s"}`;
}

// ─── Histogram export surface (v3.40 round-21 Slice 101) ────────────

/**
 * Optional time-window + row-cap filter for a histogram export.
 * Mirrors the backend `plugin_histogram` boundaries. Either or both
 * of `since_unix` / `until_unix` may be omitted for "no lower / no
 * upper bound"; both omitted exports across the whole log.
 *
 * `limit` clamps the number of plugin rows written (defaults to 25
 * on the backend — matches the read endpoint's default so "export
 * what I'm looking at" is the natural reading without remembering
 * a custom export limit).
 */
export interface HistogramExportFilter {
  since_unix?: number | null;
  until_unix?: number | null;
  limit?: number | null;
}

const EMPTY_HISTOGRAM_FILTER: HistogramExportFilter = {};

/**
 * Write the top-plugins histogram to `path` as RFC-4180 CSV. `path`
 * is an absolute filesystem path the caller usually obtains from
 * `@tauri-apps/plugin-dialog` `save()` so it bypasses the default
 * plugin-fs scope.
 *
 * Returns the byte count actually written so the UI toast can say
 * "Exported 12 plugins (1.8 KB)" without re-reading the file.
 * Returns 0 in browser mode (no-op) so the drawer's export flow
 * doesn't have to special-case the dev environment.
 */
export async function exportInstallLogHistogramCsv(
  path: string,
  filter: HistogramExportFilter = EMPTY_HISTOGRAM_FILTER,
): Promise<number> {
  if (!isInTauri()) return 0;
  return invoke<number>("slab_marketplace_install_log_export_histogram_csv", {
    path,
    sinceUnix: filter.since_unix ?? null,
    untilUnix: filter.until_unix ?? null,
    limit: filter.limit ?? null,
  });
}

/**
 * Write the top-plugins histogram to `path` as a pretty-printed JSON
 * envelope (`PluginHistogramExportEnvelope` shape: schema_version +
 * generated_at_iso + window-bounds + row_count + grand_total + rows).
 * Mirrors `exportInstallLogHistogramCsv` — same filter, same window
 * semantics, same return.
 */
export async function exportInstallLogHistogramJson(
  path: string,
  filter: HistogramExportFilter = EMPTY_HISTOGRAM_FILTER,
): Promise<number> {
  if (!isInTauri()) return 0;
  return invoke<number>("slab_marketplace_install_log_export_histogram_json", {
    path,
    sinceUnix: filter.since_unix ?? null,
    untilUnix: filter.until_unix ?? null,
    limit: filter.limit ?? null,
  });
}

/**
 * Suggest a default filename for a histogram export. Mirrors the
 * install-log export filename convention (slice 61) so paralegals
 * see one consistent naming pattern across the audit-export
 * surfaces.
 *
 * Format: `marketplace-top-plugins_<window>_<YYYY-MM-DD>.<ext>`.
 * The `window` slot reads "all" when both bounds are unset,
 * "from-YYYYMMDD" when only `since` is set, "to-YYYYMMDD" when
 * only `until` is set, and "YYYYMMDD-YYYYMMDD" when both are set —
 * exactly the same shape as `suggestInstallLogExportFilename` so
 * the two exports sort next to each other when a user collects
 * audit files in a directory.
 *
 * Pure helper — no I/O, no Tauri context required. `now` is
 * injectable for deterministic tests; production callers leave it
 * unset and it reads `Date.now()`.
 */
export function suggestHistogramExportFilename(
  filter: HistogramExportFilter,
  ext: "csv" | "json",
  now?: number,
): string {
  const iso = (unixSec: number): string => {
    const d = new Date(unixSec * 1000);
    const y = d.getUTCFullYear();
    const m = String(d.getUTCMonth() + 1).padStart(2, "0");
    const day = String(d.getUTCDate()).padStart(2, "0");
    return `${y}${m}${day}`;
  };
  const since = filter.since_unix ?? null;
  const until = filter.until_unix ?? null;
  let window = "all";
  if (since !== null && until !== null) window = `${iso(since)}-${iso(until)}`;
  else if (since !== null) window = `from-${iso(since)}`;
  else if (until !== null) window = `to-${iso(until)}`;
  const todaySec = Math.floor((now ?? Date.now()) / 1000);
  const today = iso(todaySec);
  return `marketplace-top-plugins_${window}_${today}.${ext}`;
}

// ─── Histogram sort axis (v3.40 Slice 97) ───────────────────────────

/**
 * Sort keys the Top plugins histogram can be reordered by. The
 * server emits rows sorted by `total` DESC (matching the default
 * "most active first" mental model); the UI offers four extra axes
 * so a paralegal can pivot the same row set:
 *
 *   - `"total"`     — sum across actions, server's default
 *   - `"installs"`  — fresh-install volume only
 *   - `"updates"`   — version-bump volume only
 *   - `"failures"`  — failed install/update count (the bug hunt)
 *   - `"recent"`    — most-recent last_occurred_at first
 *
 * Uninstalls is deliberately NOT a sort axis: uninstall-heavy
 * plugins are an antipattern the user typically wants to spot via
 * the bar segment, not as a sort default. Keeping the menu lean
 * (5 axes, not 7) reads better and matches the four-action stacked
 * bar segments minus the uninstall outlier.
 */
export type HistogramSortKey =
  | "total"
  | "installs"
  | "updates"
  | "failures"
  | "recent";

/**
 * Stable in-place-equivalent sort for histogram rows. Always sorts
 * descending on the requested axis (most-of-X-first reads more
 * naturally than ascending for every count axis); secondary sort is
 * ASC on `plugin_id` so ties are reproducible across calls and the
 * row order doesn't reshuffle on a refresh.
 *
 * Pure function — returns a NEW array; the caller's input is not
 * mutated. This matters because Svelte 5's `$state` proxies don't
 * play nicely with in-place sorts, and the server payload should
 * stay untouched in case a different sort gets re-applied later.
 *
 * The `recent` key sorts by `last_occurred_at` DESC; everything
 * else is a count column.
 */
export function sortHistogramRows(
  rows: readonly PluginHistogramRow[],
  key: HistogramSortKey,
): PluginHistogramRow[] {
  const out = [...rows];
  const valueOf = (row: PluginHistogramRow): number => {
    switch (key) {
      case "total":
        return row.total;
      case "installs":
        return row.installs;
      case "updates":
        return row.updates;
      case "failures":
        return row.failures;
      case "recent":
        return row.last_occurred_at;
    }
  };
  out.sort((a, b) => {
    const diff = valueOf(b) - valueOf(a);
    if (diff !== 0) return diff;
    // Deterministic tiebreak so a refresh doesn't reshuffle.
    return a.plugin_id.localeCompare(b.plugin_id);
  });
  return out;
}

/**
 * Human label for a sort key — used in the toggle dropdown and the
 * "Sorted by X" subtitle. Singular nouns match the rest of the
 * drawer's voice; "Most recent" reads naturally for the timestamp
 * axis (vs the count axes which already imply "most-of-X first").
 */
export function histogramSortLabel(key: HistogramSortKey): string {
  switch (key) {
    case "total":
      return "Most active";
    case "installs":
      return "Most installs";
    case "updates":
      return "Most updates";
    case "failures":
      return "Most failures";
    case "recent":
      return "Most recent";
  }
}

/**
 * All sort keys in display order. Keep `total` first so it reads as
 * the default; `recent` last because it's a timestamp axis and feels
 * like a "switch the mental model" rather than a count-pivot.
 */
export const HISTOGRAM_SORT_KEYS: readonly HistogramSortKey[] = [
  "total",
  "installs",
  "updates",
  "failures",
  "recent",
] as const;

/**
 * Human-friendly label for a set of action filters. Used in the
 * "Showing X of Y events" subtitle and the filter-bar empty-state.
 * The canonical render:
 *
 *   ∅                              → "all actions"
 *   ALL_INSTALL_ACTIONS            → "all actions"
 *   { "failed" }                   → "failures only"
 *   { "install", "update" }        → "installs and updates"
 *   { "install", "update", ... }   → "installs, updates and uninstalls"
 *
 * Single-action plurals use the action's natural plural (install →
 * installs); "failed" pluralises to "failures" for cleaner copy. The
 * three-or-more form uses Oxford-style "X, Y and Z" without a comma
 * before the final "and" (matches the surrounding app voice — see
 * formatUpdateSummary in slice 70).
 */
export function describeActionSet(
  actions: readonly InstallEvent["action"][] | null | undefined,
): string {
  // Treat missing / empty / full-set as "all actions".
  if (!actions || actions.length === 0) return "all actions";
  if (actions.length === ALL_INSTALL_ACTIONS.length) {
    // De-dupe + size check — a caller might pass duplicates.
    const set = new Set(actions);
    if (ALL_INSTALL_ACTIONS.every((a) => set.has(a))) return "all actions";
  }
  // Stable plural forms. "failed" → "failures" for readable copy.
  const plurals: Record<InstallEvent["action"], string> = {
    install: "installs",
    update: "updates",
    uninstall: "uninstalls",
    failed: "failures",
  };
  // Single-action specialisation: "failures only" reads better than
  // "failures" as a chip subtitle.
  if (actions.length === 1) {
    const only = plurals[actions[0]];
    return `${only} only`;
  }
  // Deterministic order — match the canonical ALL_INSTALL_ACTIONS
  // sequence so two callers with the same set always render the same
  // string regardless of input order.
  const ordered = ALL_INSTALL_ACTIONS.filter((a) =>
    actions.includes(a),
  ).map((a) => plurals[a]);
  if (ordered.length === 2) return `${ordered[0]} and ${ordered[1]}`;
  // 3+: Oxford-style "X, Y and Z" (no Oxford comma — matches the app's
  // existing copy in formatUpdateSummary).
  const head = ordered.slice(0, -1).join(", ");
  return `${head} and ${ordered[ordered.length - 1]}`;
}

/**
 * Compact label for the active filter state, suitable for a footer
 * subtitle in the drawer ("3 filters active") or a chip count badge.
 * Counts the number of axes (out of four) that are narrowing the result:
 * window, action set, plugin substring. The window axis counts as ONE
 * narrowing axis even when both since+until are set (a single semantic
 * choice from the user). Returns `null` when no axis narrows — the
 * caller can hide the subtitle entirely on a clean filter.
 */
export function pluginQueryActiveLabel(
  query: InstallEventQuery,
): string | null {
  let n = 0;
  if (query.since_unix != null || query.until_unix != null) n++;
  if (query.actions && query.actions.length > 0) {
    // A full-set explicit pick reads as "no action filter" too.
    const set = new Set(query.actions);
    const isFullSet =
      query.actions.length === ALL_INSTALL_ACTIONS.length &&
      ALL_INSTALL_ACTIONS.every((a) => set.has(a));
    if (!isFullSet) n++;
  }
  if (query.plugin_id_substr && query.plugin_id_substr.trim() !== "") n++;
  if (n === 0) return null;
  return `${n} filter${n === 1 ? "" : "s"} active`;
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

// ─── Install log export surface (v3.39 Slice 61) ────────────────────

/**
 * Optional time-window + row-cap filter for an install-log export.
 * Mirrors the backend `list_events_between` boundaries. Either or
 * both of `since_unix` / `until_unix` may be omitted for "no
 * lower / no upper bound"; both omitted exports the whole log.
 *
 * `limit` clamps the number of rows written (defaults to 100_000 on
 * the backend); the install log is small in practice but a defensive
 * cap protects against a runaway log eating a user's disk.
 */
export interface InstallLogExportFilter {
  since_unix?: number | null;
  until_unix?: number | null;
  limit?: number | null;
}

const EMPTY_FILTER: InstallLogExportFilter = {};

/**
 * Write the install log to `path` as RFC-4180 CSV. The path is an
 * absolute filesystem path that the caller usually obtains from
 * `@tauri-apps/plugin-dialog` `save()` so it bypasses the default
 * plugin-fs scope.
 *
 * Returns the byte count actually written so the UI toast can say
 * "Exported N events (X.X KB)" without re-reading the file.
 * Returns 0 in browser mode (no-op).
 */
export async function exportInstallLogCsv(
  path: string,
  filter: InstallLogExportFilter = EMPTY_FILTER,
): Promise<number> {
  if (!isInTauri()) return 0;
  return invoke<number>("slab_marketplace_install_log_export_csv", {
    path,
    sinceUnix: filter.since_unix ?? null,
    untilUnix: filter.until_unix ?? null,
    limit: filter.limit ?? null,
  });
}

/**
 * Write the install log to `path` as a pretty-printed JSON envelope.
 * Mirrors `exportInstallLogCsv` but emits the `InstallLogExportEnvelope`
 * shape (schema_version + generated_at_iso + window-bounds + events).
 */
export async function exportInstallLogJson(
  path: string,
  filter: InstallLogExportFilter = EMPTY_FILTER,
): Promise<number> {
  if (!isInTauri()) return 0;
  return invoke<number>("slab_marketplace_install_log_export_json", {
    path,
    sinceUnix: filter.since_unix ?? null,
    untilUnix: filter.until_unix ?? null,
    limit: filter.limit ?? null,
  });
}

/**
 * Suggest a default filename for an install-log export. Mirrors the
 * hopper backfill CSV convention so paralegals see one consistent
 * naming pattern across the audit-export surfaces.
 *
 * Format: `marketplace-history_<window>_<YYYY-MM-DD>.<ext>`. The
 * window slot reads "all" when both bounds are unset, "from-YYYYMMDD"
 * when only `since` is set, "to-YYYYMMDD" when only `until` is set,
 * and "YYYYMMDD-YYYYMMDD" when both are set. Pure helper — no I/O,
 * no Tauri.
 */
export function suggestInstallLogExportFilename(
  filter: InstallLogExportFilter,
  ext: "csv" | "json",
  now?: number,
): string {
  const iso = (unixSec: number): string => {
    const d = new Date(unixSec * 1000);
    const y = d.getUTCFullYear();
    const m = String(d.getUTCMonth() + 1).padStart(2, "0");
    const day = String(d.getUTCDate()).padStart(2, "0");
    return `${y}${m}${day}`;
  };
  const since = filter.since_unix ?? null;
  const until = filter.until_unix ?? null;
  let window = "all";
  if (since !== null && until !== null) window = `${iso(since)}-${iso(until)}`;
  else if (since !== null) window = `from-${iso(since)}`;
  else if (until !== null) window = `to-${iso(until)}`;
  const todaySec = Math.floor((now ?? Date.now()) / 1000);
  const today = iso(todaySec);
  return `marketplace-history_${window}_${today}.${ext}`;
}

// ─── Install log retention policy (v3.40 Slice 65) ──────────────────

/**
 * Effective retention policy for the install log.
 *
 * `retain_days` is the user-modifiable window (defaults to 365 when
 * never set). `last_auto_prune_at` is the unix-seconds stamp from the
 * most recent auto-prune execution, or `null` if it has never run on
 * this install — the UI uses it to render a "Last auto-prune:
 * <relative>" line and to compute "Next auto-prune in <duration>".
 *
 * The three `*_*` capability fields surface the backend's policy
 * constants (default, floor, debounce interval) so the UI doesn't
 * have to hard-code them — bumping `DEFAULT_RETAIN_DAYS` in Rust
 * flows through here transparently.
 */
export interface InstallLogRetentionPolicy {
  retain_days: number;
  last_auto_prune_at: number | null;
  default_retain_days: number;
  min_retain_days: number;
  auto_prune_interval_secs: number;
}

/**
 * Discriminated union returned by `runInstallLogAutoPrune`. Matches the
 * Rust `AutoPruneOutcome` shape (snake_case tagged enum):
 *
 * - `{ outcome: "pruned", rows_removed, retain_days, cutoff_unix }` —
 *   the prune ran (either because it had never run before, the
 *   debounce window had elapsed, or `force` was passed).
 * - `{ outcome: "skipped", next_due_unix }` — the debounce window had
 *   not yet elapsed; no rows were touched. `next_due_unix` is when the
 *   next unforced call will actually prune.
 */
export type InstallLogAutoPruneOutcome =
  | {
      outcome: "pruned";
      rows_removed: number;
      retain_days: number;
      cutoff_unix: number;
    }
  | { outcome: "skipped"; next_due_unix: number };

const BROWSER_FALLBACK_POLICY: InstallLogRetentionPolicy = {
  retain_days: 365,
  last_auto_prune_at: null,
  default_retain_days: 365,
  min_retain_days: 1,
  auto_prune_interval_secs: 86_400,
};

/**
 * Read the current retention policy. Cheap (two key-value queries on
 * the backend); safe to call on every drawer mount. In browser mode
 * returns the fallback policy that mirrors the Rust constants so the
 * UI renders consistently for dev / preview builds.
 */
export async function getInstallLogRetentionPolicy(): Promise<InstallLogRetentionPolicy> {
  if (!isInTauri()) return { ...BROWSER_FALLBACK_POLICY };
  return invoke<InstallLogRetentionPolicy>(
    "slab_marketplace_install_log_retention_policy",
  );
}

/**
 * Persist a new retention window in days. Returns the value actually
 * stored after the backend's `MIN_RETAIN_DAYS` clamp — when the user
 * types 0 the backend stores 1 and we surface 1 here, so the input
 * field can correct itself inline without an extra round-trip.
 *
 * Does NOT immediately run a prune — that is a separate user action
 * via `runInstallLogAutoPrune`. Changing the policy and applying it
 * are independent so the user can edit + cancel without altering the
 * log.
 *
 * In browser mode returns the requested value (clamped at >= 1) so
 * the UI's optimistic update reads consistently.
 */
export async function setInstallLogRetentionDays(days: number): Promise<number> {
  if (!isInTauri()) return Math.max(1, Math.trunc(days));
  return invoke<number>("slab_marketplace_install_log_set_retention_days", {
    days,
  });
}

/**
 * Run the retention auto-prune if the 24-hour debounce window has
 * elapsed (or unconditionally if `force` is true). Returns the
 * outcome discriminator so the UI can either surface
 * "Auto-pruned N events" or "Next auto-prune due in X" depending on
 * which branch fired.
 *
 * The force path is for the user-clicked "Run auto-prune now" button;
 * the natural-debounce path is for the startup wiring. Both paths
 * re-stamp `last_auto_prune_at` after a prune, so a forced run still
 * resets the 24h debounce.
 *
 * In browser mode returns a synthetic "skipped" outcome dated 1 day
 * out so the UI's "Next auto-prune" copy reads sensibly without a
 * real backend.
 */
export async function runInstallLogAutoPrune(
  force = false,
): Promise<InstallLogAutoPruneOutcome> {
  if (!isInTauri()) {
    return {
      outcome: "skipped",
      next_due_unix: Math.floor(Date.now() / 1000) + 86_400,
    };
  }
  return invoke<InstallLogAutoPruneOutcome>(
    "slab_marketplace_install_log_auto_prune",
    { force },
  );
}

/**
 * Human-friendly subtitle for the Retention section. Renders one of:
 *
 * - "Never auto-pruned"                 — last_auto_prune_at is null
 * - "Last auto-prune: 2h ago"           — pruned recently
 * - "Last auto-prune: yesterday"        — within 7 days
 * - "Last auto-prune: 2026-06-15"       — older than 7 days
 *
 * Accepts an injectable `now` (unix seconds) for deterministic unit
 * tests. Pure helper — no I/O, no Tauri.
 */
export function formatLastAutoPrune(
  lastUnix: number | null,
  now?: number,
): string {
  if (lastUnix === null) return "Never auto-pruned";
  const nowSec = Math.floor((now ?? Date.now()) / 1000);
  const delta = Math.max(0, nowSec - lastUnix);
  if (delta < 90) return "Last auto-prune: just now";
  if (delta < 3_600) {
    const m = Math.floor(delta / 60);
    return `Last auto-prune: ${m}m ago`;
  }
  if (delta < 86_400) {
    const h = Math.floor(delta / 3_600);
    return `Last auto-prune: ${h}h ago`;
  }
  if (delta < 86_400 * 2) return "Last auto-prune: yesterday";
  if (delta < 86_400 * 7) {
    const d = Math.floor(delta / 86_400);
    return `Last auto-prune: ${d}d ago`;
  }
  const date = new Date(lastUnix * 1000);
  const y = date.getUTCFullYear();
  const m = String(date.getUTCMonth() + 1).padStart(2, "0");
  const day = String(date.getUTCDate()).padStart(2, "0");
  return `Last auto-prune: ${y}-${m}-${day}`;
}

/**
 * Human-friendly "Next auto-prune in X" subtitle for the Retention
 * section's debounce indicator. Returns:
 *
 * - "Due now"                          — at or past the due time
 * - "Next auto-prune in 4h 12m"         — same-day, hours+minutes
 * - "Next auto-prune in 23m"            — same-hour, minutes only
 * - "Next auto-prune in 1d 3h"          — across day boundary
 *
 * Accepts an injectable `now` for deterministic unit tests.
 */
export function formatNextAutoPrune(
  nextDueUnix: number,
  now?: number,
): string {
  const nowSec = Math.floor((now ?? Date.now()) / 1000);
  const delta = nextDueUnix - nowSec;
  if (delta <= 0) return "Due now";
  if (delta < 60) return "Next auto-prune in <1m";
  if (delta < 3_600) {
    const m = Math.floor(delta / 60);
    return `Next auto-prune in ${m}m`;
  }
  if (delta < 86_400) {
    const h = Math.floor(delta / 3_600);
    const m = Math.floor((delta % 3_600) / 60);
    return m > 0
      ? `Next auto-prune in ${h}h ${m}m`
      : `Next auto-prune in ${h}h`;
  }
  const d = Math.floor(delta / 86_400);
  const h = Math.floor((delta % 86_400) / 3_600);
  return h > 0
    ? `Next auto-prune in ${d}d ${h}h`
    : `Next auto-prune in ${d}d`;
}

// ─── Bulk update surface (v3.39 round-15 slices 68-72) ──────────────

/**
 * One planned update — installed plugin id paired with the index
 * entry that supersedes it. Mirrors `marketplace::UpdateTarget`. The
 * `entry` field is the full IndexEntry so the banner UI can render
 * the new name / size / installs without a second lookup.
 */
export interface UpdateTarget {
  id: string;
  installed_version: string;
  available_version: string;
  size_bytes: number;
  entry: IndexEntry;
}

/**
 * Deterministic plan of updates to apply. Mirrors
 * `marketplace::UpdatePlan`. Targets are sorted by id ascending so
 * the banner UI doesn't need to sort defensively.
 */
export interface UpdatePlan {
  targets: UpdateTarget[];
  total_bytes: number;
}

/**
 * One step of progress emitted on `marketplace://update-progress`
 * while `slab_marketplace_update_all` is in flight. Mirrors
 * `UpdateProgress`. The TS reducer narrows on `phase` to decide
 * which row's state to mutate.
 */
export interface UpdateProgress {
  batch_id: number;
  /** 1-indexed position of the current target. */
  index: number;
  total: number;
  plugin_id: string;
  /** `"starting"` | `"done"` | `"error"`. */
  phase: "starting" | "done" | "error";
  /** Populated only on `phase === "error"`. */
  error: string | null;
}

/**
 * Per-target outcome from `slab_marketplace_update_all`. Discriminated
 * union on `kind` so the reducer can narrow.
 */
export type UpdateOutcome =
  | {
      kind: "succeeded";
      plugin_id: string;
      prior_version: string;
      new_version: string;
      bytes_written: number;
    }
  | {
      kind: "failed";
      plugin_id: string;
      prior_version: string;
      new_version: string;
      error: string;
    };

/**
 * Final report from `slab_marketplace_update_all`. The `succeeded` /
 * `failed` / `bytes_written` fields are pre-computed server-side so
 * the toast / banner-reset logic doesn't need to fold the outcomes
 * list itself.
 */
export interface BatchUpdateReport {
  batch_id: number;
  outcomes: UpdateOutcome[];
  succeeded: number;
  failed: number;
  bytes_written: number;
}

/**
 * List installed plugins for which the marketplace index has a
 * strictly-newer version. Returns an empty plan (`targets.length ===
 * 0`) when there's nothing to update; throws (Promise rejects) only
 * if the index can't be loaded at all (no network + no cache + no
 * embedded seed). Browser mode (non-Tauri) returns an empty plan so
 * the banner UI naturally hides.
 */
export async function listUpdateTargets(): Promise<UpdatePlan> {
  if (!isInTauri()) return { targets: [], total_bytes: 0 };
  return await invoke<UpdatePlan>("slab_marketplace_list_update_targets");
}

/**
 * Bulk-update one or more plugins sequentially. Pass a monotonic
 * `batchId` (typically `Date.now()`) so the progress event stream can
 * be correlated with the matching `slab_marketplace_update_all`
 * call. Returns the structured per-id outcomes report on completion.
 *
 * Browser mode synthesises an empty all-failed report so the UI's
 * "Update all" button gives consistent feedback in dev.
 *
 * Pair with `listenUpdateProgress` BEFORE awaiting this call to avoid
 * dropping early `phase: "starting"` events.
 */
export async function updateAllPlugins(
  batchId: number,
  pluginIds: string[],
): Promise<BatchUpdateReport> {
  if (!isInTauri()) {
    return {
      batch_id: batchId,
      outcomes: pluginIds.map((id) => ({
        kind: "failed" as const,
        plugin_id: id,
        prior_version: "",
        new_version: "",
        error: "Marketplace bulk-update is only available in the Slab desktop app",
      })),
      succeeded: 0,
      failed: pluginIds.length,
      bytes_written: 0,
    };
  }
  return await invoke<BatchUpdateReport>("slab_marketplace_update_all", {
    batchId,
    pluginIds,
  });
}

/**
 * Subscribe to per-step bulk-update progress events. Returns an
 * unlisten function that the caller MUST invoke when the batch
 * completes (or when the panel unmounts) to free the listener slot.
 *
 * The handler runs for EVERY in-flight batch on the host — filter on
 * `payload.batch_id` if multiple are running concurrently (the UI
 * never runs more than one at a time today, but this is the contract
 * the listener honours).
 */
export async function listenUpdateProgress(
  handler: (progress: UpdateProgress) => void,
): Promise<UnlistenFn> {
  if (!isInTauri()) return async () => {};
  return await listen<UpdateProgress>(
    "marketplace://update-progress",
    (e) => handler(e.payload),
  );
}

/**
 * Pluralise a count of updates for the banner header text. The
 * banner reads "1 update available" / "3 updates available" / "12
 * updates available". Pure string helper — no I/O, no locale magic;
 * the existing PluginsPanel i18n table interpolates {count} for the
 * full Linear-grade i18n future when needed.
 */
export function pluralizeUpdates(n: number): string {
  return n === 1 ? "1 update available" : `${n} updates available`;
}

/**
 * Compact one-line summary of a batch result for the success toast.
 * Examples:
 *   { succeeded: 3, failed: 0 } → "Updated 3 plugins (4.2 MB)"
 *   { succeeded: 2, failed: 1 } → "Updated 2 of 3 plugins (1.8 MB) · 1 failed"
 *   { succeeded: 0, failed: 1 } → "Failed to update 1 plugin"
 *   { succeeded: 1, failed: 0, bytes_written: 0 } → "Updated 1 plugin"
 *      (no size shown when bytes_written is 0 — e.g. all-failed batches)
 */
export function formatUpdateSummary(report: BatchUpdateReport): string {
  const { succeeded, failed, bytes_written } = report;
  const total = succeeded + failed;
  const sizePart = bytes_written > 0 ? ` (${formatBytes(bytes_written)})` : "";
  if (succeeded === 0 && failed === 0) {
    return "No plugins updated";
  }
  if (succeeded === 0) {
    return failed === 1
      ? "Failed to update 1 plugin"
      : `Failed to update ${failed} plugins`;
  }
  if (failed === 0) {
    const word = succeeded === 1 ? "plugin" : "plugins";
    return `Updated ${succeeded} ${word}${sizePart}`;
  }
  const word = total === 1 ? "plugin" : "plugins";
  return `Updated ${succeeded} of ${total} ${word}${sizePart} · ${failed} failed`;
}

