// Theater (v2.3.0) — presenter-mode client bindings.
//
// Thin typed wrappers around the `slab_theater_*` Tauri commands. Each
// mutating wrapper resolves to the updated TheaterState snapshot — the
// caller should broadcast it on the `slab:theater-state` event bus so
// both windows redraw in lockstep.

import { invoke } from "@tauri-apps/api/core";
import type { CmdResult } from "./types";

// ---------- Backend DTO mirrors ----------

/** Mirror of `theater::state::InkStroke`. */
export interface InkStroke {
  /** 1-based page index. */
  page: number;
  /** Normalized [x, y] pairs in [0, 1] page-space. */
  points: [number, number][];
  /** CSS color (e.g. `#ff3b30`). */
  color: string;
  /** Stroke width in CSS pixels at 1× zoom. */
  width: number;
}

/** Mirror of `theater::state::TheaterState`. */
export interface TheaterState {
  path: string;
  /** 1-based, always within [1, total_pages]. */
  current_page: number;
  total_pages: number;
  blackout: boolean;
  whiteout: boolean;
  laser_on: boolean;
  ink_mode: boolean;
  spotlight_on: boolean;
  ink_strokes: InkStroke[];
  started_at_ms: number;
}

// ---------- Helpers ----------

function unwrap<T>(res: CmdResult<T>): T {
  if (res.kind === "ok") return res.value;
  throw new Error(res.message);
}

/** Fresh empty stroke ready for point capture. */
export function makeStroke(
  page: number,
  color: string = "#ff3b30",
  width: number = 2.5,
): InkStroke {
  return {
    page,
    points: [],
    color,
    width: Math.max(0.5, width),
  };
}

/** Clamp a point into [0, 1] page-space before pushing onto a stroke. */
export function pushPoint(stroke: InkStroke, x: number, y: number): void {
  stroke.points.push([
    Math.max(0, Math.min(1, x)),
    Math.max(0, Math.min(1, y)),
  ]);
}

// ---------- Lifecycle ----------

export async function theaterStart(
  path: string,
  totalPages: number,
): Promise<TheaterState> {
  return invoke<TheaterState>("slab_theater_start", {
    path,
    totalPages,
  });
}

export async function theaterEnd(): Promise<TheaterState | null> {
  return invoke<TheaterState | null>("slab_theater_end");
}

export async function theaterSnapshot(): Promise<TheaterState | null> {
  return invoke<TheaterState | null>("slab_theater_snapshot");
}

// ---------- Navigation ----------

export async function theaterNext(): Promise<TheaterState> {
  const res = await invoke<CmdResult<TheaterState>>("slab_theater_next");
  return unwrap(res);
}

export async function theaterPrev(): Promise<TheaterState> {
  const res = await invoke<CmdResult<TheaterState>>("slab_theater_prev");
  return unwrap(res);
}

export async function theaterJump(page: number): Promise<TheaterState> {
  const res = await invoke<CmdResult<TheaterState>>("slab_theater_jump", {
    page,
  });
  return unwrap(res);
}

// ---------- Overlay toggles ----------

export async function theaterToggleBlackout(): Promise<TheaterState> {
  const res = await invoke<CmdResult<TheaterState>>(
    "slab_theater_toggle_blackout",
  );
  return unwrap(res);
}

export async function theaterToggleWhiteout(): Promise<TheaterState> {
  const res = await invoke<CmdResult<TheaterState>>(
    "slab_theater_toggle_whiteout",
  );
  return unwrap(res);
}

export async function theaterToggleLaser(): Promise<TheaterState> {
  const res = await invoke<CmdResult<TheaterState>>(
    "slab_theater_toggle_laser",
  );
  return unwrap(res);
}

export async function theaterToggleInk(): Promise<TheaterState> {
  const res = await invoke<CmdResult<TheaterState>>("slab_theater_toggle_ink");
  return unwrap(res);
}

export async function theaterToggleSpotlight(): Promise<TheaterState> {
  const res = await invoke<CmdResult<TheaterState>>(
    "slab_theater_toggle_spotlight",
  );
  return unwrap(res);
}

// ---------- Ink capture ----------

export async function theaterPushStroke(
  stroke: InkStroke,
): Promise<TheaterState> {
  const res = await invoke<CmdResult<TheaterState>>("slab_theater_push_stroke", {
    stroke,
  });
  return unwrap(res);
}

export async function theaterUndoStroke(): Promise<TheaterState> {
  const res = await invoke<CmdResult<TheaterState>>("slab_theater_undo_stroke");
  return unwrap(res);
}

export async function theaterClearStrokes(): Promise<TheaterState> {
  const res = await invoke<CmdResult<TheaterState>>(
    "slab_theater_clear_strokes",
  );
  return unwrap(res);
}

// ---------- Keyboard map (presenter window) ----------

/**
 * Dispatch a single presenter keystroke. Returns the new state if the key
 * was handled, or `null` for unrelated keys (let them bubble).
 *
 * Mappings:
 * - ArrowRight / Space / PageDown → next
 * - ArrowLeft / PageUp → prev
 * - Home → jump(1)
 * - End → jump(total_pages)
 * - B → toggle blackout
 * - W → toggle whiteout
 * - L → toggle laser pointer
 * - I → toggle ink-capture mode
 * - . → toggle spotlight cursor
 * - U → undo last stroke
 * - C → clear all strokes
 */
export async function dispatchPresenterKey(
  ev: KeyboardEvent,
  totalPages: number,
): Promise<TheaterState | null> {
  // Don't steal keys while user is typing into a text input.
  const tgt = ev.target as HTMLElement | null;
  if (
    tgt &&
    (tgt.tagName === "INPUT" ||
      tgt.tagName === "TEXTAREA" ||
      tgt.isContentEditable)
  ) {
    return null;
  }
  if (ev.metaKey || ev.ctrlKey || ev.altKey) return null;

  const key = ev.key;
  switch (key) {
    case "ArrowRight":
    case " ":
    case "PageDown":
      ev.preventDefault();
      return theaterNext();
    case "ArrowLeft":
    case "PageUp":
      ev.preventDefault();
      return theaterPrev();
    case "Home":
      ev.preventDefault();
      return theaterJump(1);
    case "End":
      ev.preventDefault();
      return theaterJump(totalPages);
    case "b":
    case "B":
      ev.preventDefault();
      return theaterToggleBlackout();
    case "w":
    case "W":
      ev.preventDefault();
      return theaterToggleWhiteout();
    case "l":
    case "L":
      ev.preventDefault();
      return theaterToggleLaser();
    case "i":
    case "I":
      ev.preventDefault();
      return theaterToggleInk();
    case ".":
      ev.preventDefault();
      return theaterToggleSpotlight();
    case "u":
    case "U":
      ev.preventDefault();
      return theaterUndoStroke();
    case "c":
    case "C":
      ev.preventDefault();
      return theaterClearStrokes();
    default:
      return null;
  }
}

// ---------- Slice 5: detached audience + control windows ----------

/** Labels returned by `slab_theater_open_windows`. */
export interface TheaterWindowLabels {
  audience: string;
  control: string;
}

/**
 * Spawn the dedicated full-screen audience window and the presenter
 * control window. Singleton — calling twice returns the existing
 * labels rather than stacking duplicates.
 */
export async function theaterOpenWindows(
  targetDoc?: string | null,
): Promise<TheaterWindowLabels> {
  return invoke<TheaterWindowLabels>("slab_theater_open_windows", {
    targetDoc: targetDoc ?? null,
  });
}

/** Close audience + control windows; the session itself stays alive. */
export async function theaterCloseWindows(): Promise<number> {
  return invoke<number>("slab_theater_close_windows");
}

