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

// ─── Toast action buttons (round-36 Slice 1) ────────────────────────

/**
 * An inline action a toast can carry, e.g. an "Undo" on a destructive
 * op or a "Retry" on a failed render. Rendered as a button beside the
 * close ×; clicking (or Enter/Space) runs {@link onClick} and — unless
 * {@link dismissOnClick} is false — dismisses the toast. The label lives
 * in the pure layer (trim/clamp/validate); the handler lives in the
 * store since it closes over caller state.
 */
export interface ToastAction {
  /** Button text, e.g. "Undo". Trimmed; blank after trim = no action. */
  label: string;
  /** Invoked on activation (click / Enter / Space). */
  onClick: () => void;
  /** Dismiss the toast after the handler runs. Defaults to true. */
  dismissOnClick?: boolean;
}

/**
 * Action labels are a single word or two — clamp so a long string can't
 * blow out the toast's fixed width and crowd out the message.
 */
export const TOAST_ACTION_LABEL_MAX = 24;

/**
 * Validate + normalize a raw action into a clean, renderable one, or
 * `null` if it can't be shown. A usable action needs a non-blank label
 * AND a callable handler — a label with no handler is a dead button, a
 * handler with no label has nothing to click. The label is trimmed and
 * length-clamped; `dismissOnClick` defaults to true. Pure: builds a
 * fresh object, never mutates the input.
 */
export function normalizeToastAction(
  action: ToastAction | undefined | null,
): ToastAction | null {
  if (!action || typeof action.onClick !== "function") return null;
  const label = clampToastActionLabel(action.label);
  if (!label) return null;
  return {
    label,
    onClick: action.onClick,
    dismissOnClick: action.dismissOnClick !== false,
  };
}

/**
 * Trim + length-clamp an action label, appending an ellipsis when
 * truncated. A non-string / blank label yields "" so callers can treat
 * it as "no action". The ellipsis counts toward the cap so the rendered
 * string never exceeds {@link TOAST_ACTION_LABEL_MAX}.
 */
export function clampToastActionLabel(label: unknown): string {
  if (typeof label !== "string") return "";
  const trimmed = label.trim();
  if (trimmed.length <= TOAST_ACTION_LABEL_MAX) return trimmed;
  return trimmed.slice(0, TOAST_ACTION_LABEL_MAX - 1).trimEnd() + "\u2026";
}

/** Whether a toast currently carries a usable, renderable action. */
export function hasToastAction(toast: Pick<Toast, "action">): boolean {
  return normalizeToastAction(toast.action) !== null;
}

/**
 * Whether activating an action should also dismiss its toast. Reads the
 * normalized flag (default true); a non-actionable toast returns false
 * so a stray keypress can't dismiss a toast with no action.
 */
export function toastActionDismisses(toast: Pick<Toast, "action">): boolean {
  const a = normalizeToastAction(toast.action);
  return a !== null && a.dismissOnClick === true;
}

// ─── Swipe / drag-to-dismiss (round-36 Slice 2) ─────────────────────

/**
 * Live state of a pointer-drag on a toast. The stack sits bottom-RIGHT,
 * so a drag toward the right edge (positive dx) flicks the toast away;
 * a small drag snaps back. Pure data — `notify.ts` / `ToastStack.svelte`
 * own the real Pointer Events; this models the geometry + decision so
 * the threshold / opacity / dismiss-direction math is unit-testable
 * without a DOM. Same "pure core, thin imperative shell" split as the
 * round-34 drag-reorder geometry.
 */
export interface ToastSwipe {
  /** Pointer X where the drag began. */
  startX: number;
  /** Timestamp the drag began (for flick-velocity). */
  startTime: number;
  /** Current horizontal offset from start, in px. Negative = leftward. */
  dx: number;
  /** Timestamp of the most recent move (for flick-velocity). */
  lastTime: number;
}

/**
 * Past this many px of rightward drag, releasing dismisses the toast.
 * Below it the toast snaps back. A toast is ~260-380px wide, so 80px is
 * a deliberate gesture without needing to drag the whole card off-screen.
 */
export const TOAST_SWIPE_DISMISS_PX = 80;

/**
 * A fast flick past this speed (px/ms) dismisses even below the distance
 * threshold — matching the iOS / Sonner "flick it away" feel. ~0.5px/ms
 * = 500px/s, brisk but not twitchy.
 */
export const TOAST_SWIPE_FLICK_VELOCITY = 0.5;

/** Begin a drag at `startX` / `now`. */
export function createToastSwipe(startX: number, now: number): ToastSwipe {
  return { startX, startTime: now, dx: 0, lastTime: now };
}

/**
 * Advance the drag to pointer `x` at `now`. Only RIGHTWARD travel (toward
 * the corner) accumulates; leftward is clamped to 0 so the toast can't be
 * dragged away from its edge and left floating. Pure: fresh object.
 */
export function moveToastSwipe(swipe: ToastSwipe, x: number, now: number): ToastSwipe {
  if (!swipe) return swipe;
  const raw = x - swipe.startX;
  const dx = raw > 0 ? raw : 0;
  return { ...swipe, dx, lastTime: now };
}

/** Average drag speed in px/ms over the gesture's lifetime (>= 0). */
export function toastSwipeVelocity(swipe: ToastSwipe): number {
  if (!swipe) return 0;
  const dt = swipe.lastTime - swipe.startTime;
  if (dt <= 0) return 0;
  return Math.max(0, swipe.dx) / dt;
}

/**
 * On pointer-release, should the toast be dismissed? True when it was
 * dragged past {@link TOAST_SWIPE_DISMISS_PX} OR flicked faster than
 * {@link TOAST_SWIPE_FLICK_VELOCITY} (with at least a token 16px of
 * travel so a fast tap-without-drag doesn't dismiss).
 */
export function toastSwipeShouldDismiss(
  swipe: ToastSwipe,
  thresholdPx: number = TOAST_SWIPE_DISMISS_PX,
): boolean {
  if (!swipe) return false;
  const dist = Math.max(0, swipe.dx);
  const threshold =
    Number.isFinite(thresholdPx) && thresholdPx > 0 ? thresholdPx : TOAST_SWIPE_DISMISS_PX;
  if (dist >= threshold) return true;
  return dist >= 16 && toastSwipeVelocity(swipe) >= TOAST_SWIPE_FLICK_VELOCITY;
}

/**
 * Opacity `[0.25, 1]` for the dragged toast — fades toward (but never to)
 * transparent as it approaches the dismiss threshold so the user sees the
 * gesture "taking". Pinned at 1 before any travel; floored at 0.25 so a
 * mid-drag toast stays legible if it snaps back. Defensive on bad input.
 */
export function toastSwipeOpacity(
  dx: number,
  thresholdPx: number = TOAST_SWIPE_DISMISS_PX,
): number {
  const dist = Number.isFinite(dx) && dx > 0 ? dx : 0;
  const threshold =
    Number.isFinite(thresholdPx) && thresholdPx > 0 ? thresholdPx : TOAST_SWIPE_DISMISS_PX;
  const faded = 1 - (dist / threshold) * 0.75;
  if (faded <= 0.25) return 0.25;
  if (faded >= 1) return 1;
  return faded;
}

// ─── Promise / loading lifecycle (round-36 Slice 3) ─────────────────

/**
 * A message that can be a fixed string OR a function of the resolved /
 * rejected value — so `notify.promise` can render "Saved 3 files" from
 * the promise's result, or "Upload failed: <err>" from the rejection.
 */
export type ToastMessageSpec<T> = string | ((value: T) => string);

/**
 * The three messages a promise toast cycles through: while pending, on
 * fulfilment, and on rejection. `loading` is a plain string (no value
 * yet); `success`/`error` may be functions of the settled value.
 */
export interface ToastPromiseSpec<T> {
  loading: string;
  success: ToastMessageSpec<T>;
  error: ToastMessageSpec<unknown>;
}

/**
 * Resolve a {@link ToastMessageSpec} against a settled value. A function
 * spec is invoked with the value; a string spec passes through. A thrown
 * formatter or a non-string return degrades to `fallback` so a bad
 * message function can never crash the toast that reports an error.
 */
export function resolveToastMessage<T>(
  spec: ToastMessageSpec<T>,
  value: T,
  fallback = "",
): string {
  if (typeof spec === "function") {
    try {
      const out = (spec as (v: T) => string)(value);
      return typeof out === "string" ? out : fallback;
    } catch {
      return fallback;
    }
  }
  return typeof spec === "string" ? spec : fallback;
}

/**
 * Best-effort human string for a rejection reason, for the default
 * error-message fallback: an `Error`'s message, a plain string, else a
 * generic "Something went wrong". Never throws.
 */
export function describeToastError(reason: unknown): string {
  if (reason instanceof Error && reason.message) return reason.message;
  if (typeof reason === "string" && reason.trim()) return reason.trim();
  return "Something went wrong";
}

/**
 * The fields to patch onto a loading toast when its promise FULFILS:
 * flip to success, swap in the resolved message, stop loading, and apply
 * the settled auto-dismiss duration (loading toasts are sticky). Pure —
 * returns a patch object the store applies.
 */
export interface ToastSettlePatch {
  kind: ToastKind;
  message: string;
  loading: false;
  duration: number;
  detail?: string;
}

export function toastFulfilPatch<T>(
  spec: ToastPromiseSpec<T>,
  value: T,
  duration: number,
): ToastSettlePatch {
  return {
    kind: "success",
    message: resolveToastMessage(spec.success, value, "Done"),
    loading: false,
    duration,
  };
}

/**
 * The patch for a REJECTED promise: flip to error, render the error
 * message (falling back through {@link describeToastError}), stop
 * loading, apply the settled duration.
 */
export function toastRejectPatch<T>(
  spec: ToastPromiseSpec<T>,
  reason: unknown,
  duration: number,
): ToastSettlePatch {
  const fallback = describeToastError(reason);
  return {
    kind: "error",
    message: resolveToastMessage(spec.error, reason, fallback) || fallback,
    loading: false,
    duration,
  };
}

/** Whether a toast is in the pending/loading state (renders a spinner). */
export function isLoadingToast(toast: Pick<Toast, "loading">): boolean {
  return toast.loading === true;
}

// Re-export the kind union so consumers can import everything toast-shaped
// from one module without reaching into notify.ts for a type.
export type { Toast, ToastKind };
