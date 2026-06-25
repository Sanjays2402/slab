// Global notification (toast) system.
//
// Tiny store + helper API used app-wide:
//
//   import { notify } from "$lib/notify";
//   notify.success("Saved");
//   notify.error("Render failed", { detail: err.message, duration: 8000 });
//
// Renders via <ToastStack /> mounted once in +layout.svelte.

import { writable } from "svelte/store";
import {
  findCoalesceTarget,
  createToastTimer,
  pauseToastTimer,
  resumeToastTimer,
  toastTimerRemaining,
  type ToastTimer,
} from "./toastStack";

export type ToastKind = "success" | "error" | "info" | "warning";

export interface Toast {
  id: number;
  kind: ToastKind;
  message: string;
  detail?: string;
  duration: number; // ms, 0 = sticky
  createdAt: number;
  /** Repeat count. >1 when identical toasts coalesced; renders as "xN". */
  count: number;
}

export interface NotifyOpts {
  detail?: string;
  /** ms before auto-dismiss. Pass 0 to keep until the user dismisses. */
  duration?: number;
}

const DEFAULT_DURATION: Record<ToastKind, number> = {
  success: 3000,
  info: 4000,
  warning: 6000,
  error: 8000,
};

export const toasts = writable<Toast[]>([]);

let nextId = 1;
const timers = new Map<number, ReturnType<typeof setTimeout>>();
// Pausable lifespan model per live toast (slice 4). The real setTimeout
// above is (re)scheduled from this model's remaining time so a paused
// toast — one the user is hovering to read — never dismisses underneath
// them. Sticky toasts (duration 0) get no entry here.
const timerModels = new Map<number, ToastTimer>();

/** (Re)arm the real setTimeout for `id` from its model's remaining ms. */
function armTimer(id: number): void {
  const model = timerModels.get(id);
  if (!model) return;
  const existing = timers.get(id);
  if (existing) clearTimeout(existing);
  const remaining = toastTimerRemaining(model, Date.now());
  if (!Number.isFinite(remaining)) return; // sticky
  const t = setTimeout(() => dismiss(id), Math.max(0, remaining));
  timers.set(id, t);
}

function push(kind: ToastKind, message: string, opts: NotifyOpts = {}): number {
  const duration = opts.duration ?? DEFAULT_DURATION[kind];
  let mergedId = -1;
  toasts.update((list) => {
    // Coalesce identical toasts: a button mashed N times, or a loop
    // emitting the same line, becomes one toast with a "xN" badge that
    // resurfaces to newest (moved to the end) with a refreshed timer.
    const target = findCoalesceTarget(list, kind, message, opts.detail);
    if (target) {
      mergedId = target.id;
      const bumped: Toast = {
        ...target,
        count: target.count + 1,
        createdAt: Date.now(),
        duration,
      };
      return [...list.filter((x) => x.id !== target.id), bumped];
    }
    const id = nextId++;
    mergedId = id;
    const toast: Toast = {
      id,
      kind,
      message,
      detail: opts.detail,
      duration,
      createdAt: Date.now(),
      count: 1,
    };
    return [...list, toast];
  });
  // Reset the lifespan model to a fresh full run (covers both the new and
  // the coalesced case — a repeat restarts the clock) then arm the timer.
  if (duration > 0) {
    timerModels.set(mergedId, createToastTimer(duration, Date.now()));
    armTimer(mergedId);
  }
  return mergedId;
}

/**
 * Pause a toast's auto-dismiss clock — called when the pointer enters it
 * so a toast can't vanish mid-read. Banks the elapsed time; the bar's
 * CSS animation pauses in lockstep via `:hover`. No-op for sticky toasts.
 */
export function pauseToast(id: number): void {
  const model = timerModels.get(id);
  if (!model) return;
  timerModels.set(id, pauseToastTimer(model, Date.now()));
  const existing = timers.get(id);
  if (existing) {
    clearTimeout(existing);
    timers.delete(id);
  }
}

/**
 * Resume a paused toast's clock — called on pointer leave. Re-arms the
 * real timer for the banked remaining time. No-op for sticky toasts.
 */
export function resumeToast(id: number): void {
  const model = timerModels.get(id);
  if (!model) return;
  timerModels.set(id, resumeToastTimer(model, Date.now()));
  armTimer(id);
}

export function dismiss(id: number): void {
  const t = timers.get(id);
  if (t) {
    clearTimeout(t);
    timers.delete(id);
  }
  timerModels.delete(id);
  toasts.update((list) => list.filter((x) => x.id !== id));
}

export function dismissAll(): void {
  timers.forEach((t) => clearTimeout(t));
  timers.clear();
  timerModels.clear();
  toasts.set([]);
}

export const notify = {
  success: (message: string, opts?: NotifyOpts) => push("success", message, opts),
  error: (message: string, opts?: NotifyOpts) => push("error", message, opts),
  info: (message: string, opts?: NotifyOpts) => push("info", message, opts),
  warning: (message: string, opts?: NotifyOpts) => push("warning", message, opts),
};
