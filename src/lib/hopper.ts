// Hopper client — v3.20.0
//
// Typed bridge to the Rust Hopper engine in `src-tauri/src/pdf/hopper/`.
// Hopper watches one or more folders for newly-dropped PDFs and runs an
// Atelier recipe + AI-rename + auto-file pipeline on each one. Think
// Hazel × Adobe AutoActions × a local AI paralegal, all offline.
//
// Event stream: every completed pipeline run is broadcast as a Tauri
// event named `hopper://run-completed` carrying a `RunRecord`. The UI
// listens and prepends to the live log; on missed events the
// `slabHopperListRuns` poll fills the gap.
//
// See `src-tauri/src/pdf/hopper/{registry,log,pipeline,watcher,cmds}.rs`
// for canonical shapes.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

// ---------------------------------------------------------------------
// Wire types — mirror the Rust serde shapes verbatim.
// ---------------------------------------------------------------------

/** A persisted watched-folder configuration. */
export interface Watch {
  id: number;
  source_dir: string;
  output_dir: string;
  recipe_id: string | null;
  rename_pattern: string | null;
  ai_rename: boolean;
  enabled: boolean;
  created_at: string;
}

/** Input payload for `slabHopperAddWatch`. The backend assigns id +
 *  timestamps. */
export interface WatchInput {
  source_dir: string;
  output_dir: string;
  recipe_id: string | null;
  rename_pattern: string | null;
  ai_rename: boolean;
}

/** One row in the run history. */
export interface RunRecord {
  id: number;
  watch_id: number;
  input_path: string;
  output_path: string | null;
  status: "success" | "failed";
  error: string | null;
  duration_ms: number;
  ai_title: string | null;
  started_at: string;
}

/** Service-status payload returned by `slabHopperDescribe`. */
export interface HopperStatus {
  watch_count: number;
  run_count: number;
  version: string;
}

// ---------------------------------------------------------------------
// Tauri command bindings — one-liner wrappers around `invoke`.
// ---------------------------------------------------------------------

export const slabHopperListWatches = (): Promise<Watch[]> =>
  invoke("slab_hopper_list_watches");

export const slabHopperAddWatch = (input: WatchInput): Promise<Watch> =>
  invoke("slab_hopper_add_watch", { input });

export const slabHopperRemoveWatch = (id: number): Promise<void> =>
  invoke("slab_hopper_remove_watch", { id });

export const slabHopperSetEnabled = (
  id: number,
  enabled: boolean,
): Promise<void> => invoke("slab_hopper_set_enabled", { id, enabled });

export const slabHopperListRuns = (limit: number): Promise<RunRecord[]> =>
  invoke("slab_hopper_list_runs", { limit });

export const slabHopperRunNow = (
  watch_id: number,
  path: string,
): Promise<void> => invoke("slab_hopper_run_now", { watchId: watch_id, path });

export const slabHopperDescribe = (): Promise<HopperStatus> =>
  invoke("slab_hopper_describe");

// ---------------------------------------------------------------------
// Event listener — subscribe to `hopper://run-completed`.
// ---------------------------------------------------------------------

/** Subscribe to live pipeline-completion events. Returns an unlisten
 *  function the caller stores and invokes on component unmount. */
export const listenRunCompleted = async (
  handler: (rec: RunRecord) => void,
): Promise<UnlistenFn> => {
  return listen<RunRecord>("hopper://run-completed", (e) => handler(e.payload));
};

// ---------------------------------------------------------------------
// Pure formatting helpers — also used in unit tests.
// ---------------------------------------------------------------------

/** Format a duration in milliseconds as a compact human string. */
export const formatDuration = (ms: number): string => {
  if (ms < 1000) return `${ms} ms`;
  const seconds = ms / 1000;
  if (seconds < 60) return `${seconds.toFixed(1)} s`;
  const minutes = Math.floor(seconds / 60);
  const rem = Math.floor(seconds % 60);
  return `${minutes}m ${rem}s`;
};

/** Format a unix-seconds string (what the backend stores) as a short
 *  local time. Returns "—" for malformed inputs. */
export const formatStartedAt = (seconds: string): string => {
  const n = Number(seconds);
  if (!Number.isFinite(n) || n <= 0) return "—";
  const d = new Date(n * 1000);
  if (isNaN(d.valueOf())) return "—";
  return d.toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
};

/** Extract just the filename from a full path, robust to win/unix. */
export const basename = (path: string): string => {
  const norm = path.replace(/\\/g, "/");
  const ix = norm.lastIndexOf("/");
  return ix === -1 ? norm : norm.slice(ix + 1);
};

/** Suggest a sensible default rename pattern. Power users can edit. */
export const defaultRenamePattern = (aiRename: boolean): string =>
  aiRename ? "{date}_{ai_title}.pdf" : "{date}_{stem}.pdf";
