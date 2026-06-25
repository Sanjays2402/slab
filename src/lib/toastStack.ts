// Pure presentation helpers for the global toast stack.
//
// The store + imperative API (push / dismiss / timers / pause) live in
// `notify.ts`; this module holds ONLY pure functions so they can be unit
// tested without a DOM or the Svelte store. `ToastStack.svelte` and
// `notify.ts` both import from here.
//
// Run the tests with:
//   node --import tsx src/lib/toastStack.test.ts

import type { Toast, ToastKind } from "./notify";

// ─── Overflow partition (round-35 Slice 1) ──────────────────────────

/**
 * Max toasts rendered at once. When more than this are live, the OLDEST
 * collapse into a single "+N more" pill so a burst of actions never
 * fills the viewport. Four matches the original `ToastStack` comment's
 * intent ("stacks up to 5") while leaving headroom for the clear-all
 * header row added in slice 3.
 */
export const TOAST_MAX_VISIBLE = 4;

/**
 * The newest `maxVisible` toasts (rendered) plus the older remainder
 * (collapsed into an overflow indicator). `hiddenCount === hidden.length`
 * is precomputed so the pill copy doesn't re-measure.
 */
export interface ToastPartition {
  /** Newest `maxVisible` toasts, oldest-of-them first (store order). */
  visible: Toast[];
  /** Older toasts hidden behind the "+N more" pill, store order. */
  hidden: Toast[];
  hiddenCount: number;
}

/**
 * Split the toast list into the newest `maxVisible` (rendered) and the
 * older remainder (collapsed). The store appends newest-LAST, and the
 * bottom-right stack renders DOM order top-to-bottom — so the newest
 * toast sits at the bottom (nearest the corner) and the oldest overflow
 * at the top, where the "+N more" pill renders above them.
 *
 * Pure: never mutates the input array or its toasts. A non-positive /
 * non-finite `maxVisible` falls back to {@link TOAST_MAX_VISIBLE} so a
 * caller passing a bad config can't blank the whole stack.
 */
export function partitionToasts(
  toasts: readonly Toast[],
  maxVisible: number = TOAST_MAX_VISIBLE,
): ToastPartition {
  if (!Array.isArray(toasts) || toasts.length === 0) {
    return { visible: [], hidden: [], hiddenCount: 0 };
  }
  const cap =
    Number.isFinite(maxVisible) && maxVisible > 0
      ? Math.floor(maxVisible)
      : TOAST_MAX_VISIBLE;
  if (toasts.length <= cap) {
    return { visible: [...toasts], hidden: [], hiddenCount: 0 };
  }
  const splitAt = toasts.length - cap;
  const hidden = toasts.slice(0, splitAt);
  const visible = toasts.slice(splitAt);
  return { visible, hidden, hiddenCount: hidden.length };
}

/**
 * Overflow-pill copy: `"+2 more"` for a positive hidden count, empty
 * string otherwise (so the UI can `{#if}` on a falsy value). Defensive
 * against NaN / negative / fractional counts.
 */
export function describeToastOverflow(hiddenCount: number): string {
  if (!Number.isFinite(hiddenCount) || hiddenCount <= 0) return "";
  return `+${Math.floor(hiddenCount)} more`;
}

// ─── Duplicate coalescing (round-35 Slice 2) ────────────────────────

/**
 * The fields that make two toasts "the same notification". Kind +
 * message + detail: a button mashed three times, or a loop emitting the
 * same status line, should read as one toast with a `x3` badge rather
 * than three identical rows. Two undefined details collapse to the same
 * key as two empty-string details so `notify.error("x")` repeated still
 * coalesces.
 */
export function toastCoalesceKey(
  kind: ToastKind,
  message: string,
  detail?: string,
): string {
  // Newline can't appear in kind and is escaped here from message/detail
  // so "a|b" + "c" never collides with "a" + "b|c".
  const esc = (s: string) => s.replace(/\\/g, "\\\\").replace(/\n/g, "\\n");
  return `${kind}\n${esc(message)}\n${esc(detail ?? "")}`;
}

/**
 * Find a live toast that an incoming `{kind, message, detail}` should
 * merge INTO, or `null` if none match. Returns the most-recent match
 * (the toast nearest the corner / latest in store order) so a repeat
 * resurfaces the freshest instance. Pure: reads, never mutates.
 */
export function findCoalesceTarget(
  toasts: readonly Toast[],
  kind: ToastKind,
  message: string,
  detail?: string,
): Toast | null {
  if (!Array.isArray(toasts) || toasts.length === 0) return null;
  const key = toastCoalesceKey(kind, message, detail);
  for (let i = toasts.length - 1; i >= 0; i--) {
    const t = toasts[i];
    if (toastCoalesceKey(t.kind, t.message, t.detail) === key) return t;
  }
  return null;
}

/**
 * Count-badge copy: `"x3"` for a repeated toast, empty string for a
 * single occurrence (count <= 1) or a bad count. Uses a plain "x" so
 * the badge stays ASCII-safe in app chrome (no multiply glyph).
 */
export function describeToastCount(count: number | undefined): string {
  if (count === undefined || !Number.isFinite(count) || count <= 1) return "";
  return `x${Math.floor(count)}`;
}

// Re-export the kind union so consumers can import everything toast-shaped
// from one module without reaching into notify.ts for a type.
export type { Toast, ToastKind };
