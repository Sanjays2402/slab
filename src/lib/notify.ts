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
import { findCoalesceTarget } from "./toastStack";

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
  if (duration > 0) {
    // Refresh the auto-dismiss timer for both the new and the coalesced
    // case so a repeated toast extends its life rather than vanishing on
    // the original toast's clock.
    const existing = timers.get(mergedId);
    if (existing) clearTimeout(existing);
    const t = setTimeout(() => dismiss(mergedId), duration);
    timers.set(mergedId, t);
  }
  return mergedId;
}

export function dismiss(id: number): void {
  const t = timers.get(id);
  if (t) {
    clearTimeout(t);
    timers.delete(id);
  }
  toasts.update((list) => list.filter((x) => x.id !== id));
}

export function dismissAll(): void {
  timers.forEach((t) => clearTimeout(t));
  timers.clear();
  toasts.set([]);
}

export const notify = {
  success: (message: string, opts?: NotifyOpts) => push("success", message, opts),
  error: (message: string, opts?: NotifyOpts) => push("error", message, opts),
  info: (message: string, opts?: NotifyOpts) => push("info", message, opts),
  warning: (message: string, opts?: NotifyOpts) => push("warning", message, opts),
};
