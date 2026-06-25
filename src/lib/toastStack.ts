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

// ─── Clear-all control (round-35 Slice 3) ───────────────────────────

/**
 * Minimum live toasts before the "Clear all" header is worth showing.
 * One toast already has its own × button, so the bulk control only
 * earns its row at two or more.
 */
export const TOAST_CLEAR_ALL_THRESHOLD = 2;

/**
 * Whether the clear-all header should render for `count` live toasts.
 * Defensive against NaN / negative (a corrupt store length never forces
 * the header on).
 */
export function shouldShowClearAll(count: number): boolean {
  return Number.isFinite(count) && count >= TOAST_CLEAR_ALL_THRESHOLD;
}

/**
 * Clear-all button copy: `"Clear all 3"` so the user knows exactly how
 * many toasts the click dismisses. Empty string below the threshold so
 * the UI can `{#if}` on a falsy value. Defensive against bad counts.
 */
export function describeClearAll(count: number): string {
  if (!shouldShowClearAll(count)) return "";
  return `Clear all ${Math.floor(count)}`;
}

// ─── Pausable lifespan timer (round-35 Slice 4) ─────────────────────

/**
 * A toast's auto-dismiss clock, modelled so it can be PAUSED (on hover)
 * and resumed without losing or extending its remaining life. `notify.ts`
 * keeps one of these per live toast and drives the real `setTimeout`
 * from {@link toastTimerRemaining}; the depleting progress bar reads
 * {@link toastTimerFraction}. Pure data — every transition returns a
 * fresh object, never mutates.
 *
 * `duration <= 0` models a STICKY toast (no auto-dismiss, no bar):
 * remaining is Infinity and the fraction pins at 1.
 */
export interface ToastTimer {
  /** Total lifespan in ms. <= 0 means sticky (never expires). */
  duration: number;
  /** Ms banked to run. Equals duration at birth; frozen while paused. */
  remaining: number;
  /** Timestamp the current run segment began; -1 when paused. */
  runningSince: number;
}

/** Sentinel for a paused timer's `runningSince`. */
const TIMER_PAUSED = -1;

/**
 * Start a fresh, running timer. `remaining` seeds to the full duration
 * and the clock begins counting from `now`.
 */
export function createToastTimer(duration: number, now: number): ToastTimer {
  const dur = Number.isFinite(duration) && duration > 0 ? duration : 0;
  return { duration: dur, remaining: dur > 0 ? dur : Infinity, runningSince: now };
}

/**
 * Pause a running timer: bank the elapsed time into `remaining` and stop
 * the clock. Idempotent — pausing an already-paused (or sticky) timer
 * returns it unchanged. Never banks a negative remainder.
 */
export function pauseToastTimer(timer: ToastTimer, now: number): ToastTimer {
  if (timer.duration <= 0) return timer; // sticky: nothing to pause
  if (timer.runningSince === TIMER_PAUSED) return timer; // already paused
  const elapsed = now - timer.runningSince;
  const remaining = Math.max(0, timer.remaining - Math.max(0, elapsed));
  return { ...timer, remaining, runningSince: TIMER_PAUSED };
}

/**
 * Resume a paused timer: restart the clock from `now` keeping the banked
 * `remaining`. Idempotent — resuming an already-running (or sticky)
 * timer returns it unchanged.
 */
export function resumeToastTimer(timer: ToastTimer, now: number): ToastTimer {
  if (timer.duration <= 0) return timer; // sticky
  if (timer.runningSince !== TIMER_PAUSED) return timer; // already running
  return { ...timer, runningSince: now };
}

/**
 * Milliseconds left before this timer fires. A paused timer reports its
 * banked `remaining`; a running timer subtracts the elapsed segment
 * (clamped at 0). Sticky timers report Infinity.
 */
export function toastTimerRemaining(timer: ToastTimer, now: number): number {
  if (timer.duration <= 0) return Infinity;
  if (timer.runningSince === TIMER_PAUSED) return timer.remaining;
  const elapsed = Math.max(0, now - timer.runningSince);
  return Math.max(0, timer.remaining - elapsed);
}

/**
 * Fraction of life LEFT in `[0, 1]` — `1` full, `0` expired — for the
 * depleting progress bar. Sticky timers pin at `1` (the UI hides the bar
 * for them). Guards a zero/negative duration against divide-by-zero.
 */
export function toastTimerFraction(timer: ToastTimer, now: number): number {
  if (timer.duration <= 0) return 1;
  const remaining = toastTimerRemaining(timer, now);
  if (!Number.isFinite(remaining)) return 1;
  const f = remaining / timer.duration;
  if (f <= 0) return 0;
  if (f >= 1) return 1;
  return f;
}

/** Whether a timer has fully depleted (running and out of time). */
export function isToastTimerExpired(timer: ToastTimer, now: number): boolean {
  return Number.isFinite(toastTimerRemaining(timer, now)) &&
    toastTimerRemaining(timer, now) <= 0;
}

/** Whether a timer is currently paused (and not sticky). */
export function isToastTimerPaused(timer: ToastTimer): boolean {
  return timer.duration > 0 && timer.runningSince === TIMER_PAUSED;
}

// ─── Screen-reader announcements (round-35 Slice 5) ─────────────────

/**
 * ARIA live-region politeness for a toast kind. Errors and warnings
 * interrupt (`assertive`) because they report a failure the user needs
 * to know NOW; success/info wait their turn (`polite`) so they don't
 * stomp on whatever the user is doing.
 */
export type ToastPoliteness = "assertive" | "polite";

export function toastPoliteness(kind: ToastKind): ToastPoliteness {
  return kind === "error" || kind === "warning" ? "assertive" : "polite";
}

/**
 * Spoken prefix for a toast kind so a screen-reader user hears the
 * SEVERITY, not just the message body ("Error: Render failed" rather
 * than a bare "Render failed" indistinguishable from a success).
 */
export function toastKindLabel(kind: ToastKind): string {
  switch (kind) {
    case "success":
      return "Success";
    case "error":
      return "Error";
    case "warning":
      return "Warning";
    default:
      return "Notice";
  }
}

/**
 * Compose the full spoken string for a toast: `"Error: Render failed.
 * Disk full"` plus a `"(repeated 3 times)"` suffix when coalesced and a
 * trailing period so consecutive announcements don't run together. The
 * detail is appended after the message; an absent/blank detail is
 * skipped. Count <= 1 adds no repeat suffix.
 */
export function announceToast(
  kind: ToastKind,
  message: string,
  detail?: string,
  count = 1,
): string {
  const parts = [`${toastKindLabel(kind)}: ${message}`];
  const trimmedDetail = (detail ?? "").trim();
  if (trimmedDetail) parts.push(trimmedDetail);
  let out = parts.join(". ");
  if (Number.isFinite(count) && count > 1) {
    out += ` (repeated ${Math.floor(count)} times)`;
  }
  return out;
}

/**
 * Pick the toasts to route to the ASSERTIVE vs POLITE live region. The
 * visual stack reorders + coalesces toasts, which makes it a poor live
 * region directly; instead the UI mirrors each toast into one of two
 * dedicated hidden regions by politeness so screen readers announce
 * every toast exactly once at the right urgency. Pure: never mutates.
 */
export interface ToastAnnounceSplit {
  assertive: Toast[];
  polite: Toast[];
}

export function splitToastsByPoliteness(
  toasts: readonly Toast[],
): ToastAnnounceSplit {
  const assertive: Toast[] = [];
  const polite: Toast[] = [];
  if (Array.isArray(toasts)) {
    for (const t of toasts) {
      if (toastPoliteness(t.kind) === "assertive") assertive.push(t);
      else polite.push(t);
    }
  }
  return { assertive, polite };
}

// Re-export the kind union so consumers can import everything toast-shaped
// from one module without reaching into notify.ts for a type.
export type { Toast, ToastKind };
