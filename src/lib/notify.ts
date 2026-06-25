// Global notification (toast) system.
//
// Tiny store + helper API used app-wide:
//
//   import { notify } from "$lib/notify";
//   notify.success("Saved");
//   notify.error("Render failed", { detail: err.message, duration: 8000 });
//
// Renders via <ToastStack /> mounted once in +layout.svelte.

import { writable, get } from "svelte/store";
import {
  findCoalesceTarget,
  createToastTimer,
  pauseToastTimer,
  resumeToastTimer,
  toastTimerRemaining,
  normalizeToastAction,
  toastFulfilPatch,
  toastRejectPatch,
  type ToastTimer,
  type ToastAction,
  type ToastPromiseSpec,
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
  /** Optional inline action button (e.g. "Undo" / "Retry"). */
  action?: ToastAction;
  /** True while a promise toast is pending (renders a spinner). */
  loading?: boolean;
}

export interface NotifyOpts {
  detail?: string;
  /** ms before auto-dismiss. Pass 0 to keep until the user dismisses. */
  duration?: number;
  /**
   * Optional inline action button (e.g. an "Undo" on a destructive op).
   * Ignored if it has no callable handler or a blank label. A toast with
   * an action defaults to STICKY (duration 0) when no explicit duration
   * is passed, so the user isn't racing the timer to click it.
   */
  action?: ToastAction;
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
  const action = normalizeToastAction(opts.action) ?? undefined;
  // A toast carrying an action defaults to STICKY: the user shouldn't be
  // racing a 3s timer to click "Undo". An explicit duration still wins.
  const duration = opts.duration ?? (action ? 0 : DEFAULT_DURATION[kind]);
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
        // A repeat carrying a fresh action rebinds it (the latest handler
        // closes over the latest state); otherwise keep the existing one.
        action: action ?? target.action,
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
      action,
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

/**
 * Activate a toast's inline action (called from the action button's
 * click / Enter / Space handler). Runs the normalized handler, then —
 * unless the action opted out via `dismissOnClick: false` — dismisses
 * the toast. No-op if the toast has no usable action. Centralizing this
 * here keeps the dismiss-after-run policy in one place rather than in
 * the Svelte template.
 */
export function runToastAction(id: number): void {
  const current = get(toasts).find((x) => x.id === id);
  const action = current ? normalizeToastAction(current.action) : null;
  if (!action) return;
  try {
    action.onClick();
  } finally {
    if (action.dismissOnClick !== false) dismiss(id);
  }
}

/**
 * Push a STICKY loading toast (info kind, spinner, no auto-dismiss) and
 * return its id. Used as the pending state of {@link promise}; can also
 * be driven manually and settled with {@link settleToast}.
 */
function pushLoading(message: string, detail?: string): number {
  const id = nextId++;
  const toast: Toast = {
    id,
    kind: "info",
    message,
    detail,
    duration: 0, // sticky until the promise settles
    createdAt: Date.now(),
    count: 1,
    loading: true,
  };
  toasts.update((list) => [...list, toast]);
  return id;
}

/**
 * Transition a live loading toast to its settled state in place (kind +
 * message flip, spinner off, fresh auto-dismiss timer) so the user sees
 * one toast morph "Saving… -> Saved" rather than two stacking. No-op if
 * the id is gone (user dismissed it mid-flight).
 */
function settleToast(
  id: number,
  patch: { kind: ToastKind; message: string; duration: number; detail?: string },
): void {
  let found = false;
  toasts.update((list) =>
    list.map((t) => {
      if (t.id !== id) return t;
      found = true;
      return {
        ...t,
        kind: patch.kind,
        message: patch.message,
        detail: patch.detail ?? t.detail,
        duration: patch.duration,
        loading: false,
        createdAt: Date.now(),
      };
    }),
  );
  if (!found) return;
  if (patch.duration > 0) {
    timerModels.set(id, createToastTimer(patch.duration, Date.now()));
    armTimer(id);
  }
}

/**
 * Sonner-style promise toast: show a loading toast while `input` settles,
 * then morph it to success or error in place. `input` may be a promise or
 * a function returning one (so the work starts lazily here). Returns the
 * original promise so callers can still await/catch it. The success/error
 * messages may be plain strings or functions of the settled value.
 */
function promise<T>(
  input: Promise<T> | (() => Promise<T>),
  spec: ToastPromiseSpec<T>,
  opts: { successDuration?: number; errorDuration?: number } = {},
): Promise<T> {
  const p = typeof input === "function" ? input() : input;
  const id = pushLoading(spec.loading);
  const successMs = opts.successDuration ?? DEFAULT_DURATION.success;
  const errorMs = opts.errorDuration ?? DEFAULT_DURATION.error;
  p.then(
    (value) => settleToast(id, toastFulfilPatch(spec, value, successMs)),
    (reason) => settleToast(id, toastRejectPatch(spec, reason, errorMs)),
  );
  return p;
}

export const notify = {
  success: (message: string, opts?: NotifyOpts) => push("success", message, opts),
  error: (message: string, opts?: NotifyOpts) => push("error", message, opts),
  info: (message: string, opts?: NotifyOpts) => push("info", message, opts),
  warning: (message: string, opts?: NotifyOpts) => push("warning", message, opts),
  promise,
};

export type { ToastAction, ToastPromiseSpec };
