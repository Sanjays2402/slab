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

export type ToastKind = "success" | "error" | "info" | "warning";

export interface Toast {
  id: number;
  kind: ToastKind;
  message: string;
  detail?: string;
  duration: number; // ms, 0 = sticky
  createdAt: number;
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
  const id = nextId++;
  const duration = opts.duration ?? DEFAULT_DURATION[kind];
  const toast: Toast = {
    id,
    kind,
    message,
    detail: opts.detail,
    duration,
    createdAt: Date.now(),
  };
  toasts.update((list) => [...list, toast]);
  if (duration > 0) {
    const t = setTimeout(() => dismiss(id), duration);
    timers.set(id, t);
  }
  return id;
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
