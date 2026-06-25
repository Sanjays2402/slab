// Pure-helper tests for the toast stack presentation layer.
//
// Style matches hopper.test.ts / quill.test.ts — no test runner dep,
// just a tiny inline `expect` so the contract reads at a glance.
//
// Run with:
//   node --import tsx src/lib/toastStack.test.ts

import {
  TOAST_MAX_VISIBLE,
  partitionToasts,
  describeToastOverflow,
  toastCoalesceKey,
  findCoalesceTarget,
  describeToastCount,
  TOAST_CLEAR_ALL_THRESHOLD,
  shouldShowClearAll,
  describeClearAll,
  createToastTimer,
  pauseToastTimer,
  resumeToastTimer,
  toastTimerRemaining,
  toastTimerFraction,
  isToastTimerExpired,
  isToastTimerPaused,
  toastPoliteness,
  toastKindLabel,
  announceToast,
  splitToastsByPoliteness,
  TOAST_ACTION_LABEL_MAX,
  normalizeToastAction,
  clampToastActionLabel,
  hasToastAction,
  toastActionDismisses,
  TOAST_SWIPE_DISMISS_PX,
  TOAST_SWIPE_FLICK_VELOCITY,
  createToastSwipe,
  moveToastSwipe,
  toastSwipeVelocity,
  toastSwipeShouldDismiss,
  toastSwipeOpacity,
  resolveToastMessage,
  describeToastError,
  toastFulfilPatch,
  toastRejectPatch,
  isLoadingToast,
  resolveToastFocusHotkey,
  resolveFocusedToastKey,
  pickToastFocusIndex,
  newestToastFocusIndex,
  type Toast,
  type ToastAction,
  type ToastPromiseSpec,
} from "./toastStack";

let passed = 0;
let failed = 0;

function expect(cond: boolean, label: string): void {
  if (!cond) {
    failed++;
    // eslint-disable-next-line no-console
    console.error("FAIL:", label);
    if (typeof process !== "undefined") process.exitCode = 1;
  } else {
    passed++;
    // eslint-disable-next-line no-console
    console.log("ok:", label);
  }
}

// Minimal Toast factory. Only the id matters for partition tests; the
// rest are filled so the shape type-checks.
function toast(id: number): Toast {
  return {
    id,
    kind: "info",
    message: `msg ${id}`,
    duration: 4000,
    createdAt: 1000 + id,
    count: 1,
  };
}

// Factory with explicit kind/message/detail for coalesce tests.
function mk(
  id: number,
  kind: Toast["kind"],
  message: string,
  detail?: string,
  count = 1,
): Toast {
  return { id, kind, message, detail, duration: 4000, createdAt: 1000 + id, count };
}

function ids(list: readonly Toast[]): string {
  return list.map((t) => t.id).join(",");
}

// ── partitionToasts: under / at / over the cap ───────────────────────

{
  const p = partitionToasts([], 4);
  expect(ids(p.visible) === "" && p.hiddenCount === 0, "partition: empty -> empty");
}
{
  const list = [toast(1), toast(2), toast(3)];
  const p = partitionToasts(list, 4);
  expect(ids(p.visible) === "1,2,3", "partition: under cap -> all visible");
  expect(p.hidden.length === 0 && p.hiddenCount === 0, "partition: under cap -> none hidden");
}
{
  const list = [toast(1), toast(2), toast(3), toast(4)];
  const p = partitionToasts(list, 4);
  expect(ids(p.visible) === "1,2,3,4", "partition: exactly at cap -> all visible");
  expect(p.hiddenCount === 0, "partition: at cap -> none hidden");
}
{
  // 6 toasts, cap 4: oldest 2 (ids 1,2) hide; newest 4 (3,4,5,6) show.
  const list = [toast(1), toast(2), toast(3), toast(4), toast(5), toast(6)];
  const p = partitionToasts(list, 4);
  expect(ids(p.visible) === "3,4,5,6", "partition: over cap -> newest cap visible");
  expect(ids(p.hidden) === "1,2", "partition: over cap -> oldest hidden");
  expect(p.hiddenCount === 2, "partition: hiddenCount matches hidden length");
}

// ── store-order invariant: visible always ends at the newest ─────────

{
  const list = Array.from({ length: 9 }, (_, i) => toast(i + 1));
  const p = partitionToasts(list, 4);
  expect(ids(p.visible) === "6,7,8,9", "partition: 9 toasts -> visible is the last 4");
  expect(p.hiddenCount === 5, "partition: 9 toasts cap 4 -> 5 hidden");
  // visible + hidden reconstruct the input in order.
  expect(
    ids([...p.hidden, ...p.visible]) === ids(list),
    "partition: hidden ++ visible reconstructs input order",
  );
}

// ── default cap + bad maxVisible fallback ────────────────────────────

{
  const list = Array.from({ length: 7 }, (_, i) => toast(i + 1));
  const p = partitionToasts(list);
  expect(
    p.visible.length === TOAST_MAX_VISIBLE,
    "partition: default cap == TOAST_MAX_VISIBLE",
  );
  expect(p.hiddenCount === 7 - TOAST_MAX_VISIBLE, "partition: default cap hides the rest");
}
{
  const list = [toast(1), toast(2), toast(3), toast(4), toast(5)];
  for (const bad of [0, -3, NaN, Infinity]) {
    const p = partitionToasts(list, bad);
    expect(
      p.visible.length === TOAST_MAX_VISIBLE,
      `partition: bad maxVisible ${bad} -> falls back to default cap`,
    );
  }
}
{
  // Fractional cap floors (4.9 -> 4), not rounds.
  const list = Array.from({ length: 6 }, (_, i) => toast(i + 1));
  const p = partitionToasts(list, 4.9);
  expect(p.visible.length === 4, "partition: fractional cap floors");
}

// ── purity: input never mutated ──────────────────────────────────────

{
  const list = [toast(1), toast(2), toast(3), toast(4), toast(5)];
  const before = ids(list);
  partitionToasts(list, 2);
  expect(ids(list) === before, "partition: does not mutate input");
}

// ── describeToastOverflow ────────────────────────────────────────────

{
  expect(describeToastOverflow(2) === "+2 more", "overflow: +2 more");
  expect(describeToastOverflow(1) === "+1 more", "overflow: +1 more (no special-case)");
  expect(describeToastOverflow(0) === "", "overflow: 0 -> empty");
  expect(describeToastOverflow(-1) === "", "overflow: negative -> empty");
  expect(describeToastOverflow(NaN) === "", "overflow: NaN -> empty");
  expect(describeToastOverflow(2.7) === "+2 more", "overflow: fractional floors");
}

// ── toastCoalesceKey: identity + collision-safety ────────────────────

{
  expect(
    toastCoalesceKey("error", "Render failed") === toastCoalesceKey("error", "Render failed"),
    "coalesceKey: same kind+message -> equal",
  );
  expect(
    toastCoalesceKey("error", "x") !== toastCoalesceKey("success", "x"),
    "coalesceKey: different kind -> distinct",
  );
  expect(
    toastCoalesceKey("info", "Saved", "to disk") !== toastCoalesceKey("info", "Saved", "to cloud"),
    "coalesceKey: different detail -> distinct",
  );
  // undefined detail collapses to the same key as "" so notify.x("m") repeats coalesce.
  expect(
    toastCoalesceKey("info", "m") === toastCoalesceKey("info", "m", ""),
    "coalesceKey: undefined detail == empty detail",
  );
  // Escaping defeats the classic "a|b"+"c" vs "a"+"b|c" boundary collision.
  expect(
    toastCoalesceKey("info", "a\nb", "c") !== toastCoalesceKey("info", "a", "b\nc"),
    "coalesceKey: newline in message can't collide across the boundary",
  );
}

// ── findCoalesceTarget ───────────────────────────────────────────────

{
  expect(findCoalesceTarget([], "info", "x") === null, "findCoalesce: empty -> null");
}
{
  const list = [mk(1, "info", "A"), mk(2, "error", "B")];
  expect(
    findCoalesceTarget(list, "warning", "C") === null,
    "findCoalesce: no match -> null",
  );
  const hit = findCoalesceTarget(list, "error", "B");
  expect(hit !== null && hit.id === 2, "findCoalesce: matches kind+message");
}
{
  // Two matching toasts: return the MOST RECENT (latest store order).
  const list = [mk(1, "info", "Loading"), mk(2, "success", "Done"), mk(5, "info", "Loading")];
  const hit = findCoalesceTarget(list, "info", "Loading");
  expect(hit !== null && hit.id === 5, "findCoalesce: returns most-recent match");
}
{
  // Detail participates in the match.
  const list = [mk(1, "info", "Saved", "to disk")];
  expect(findCoalesceTarget(list, "info", "Saved", "to cloud") === null, "findCoalesce: detail mismatch -> null");
  const hit = findCoalesceTarget(list, "info", "Saved", "to disk");
  expect(hit !== null && hit.id === 1, "findCoalesce: detail match -> hit");
}
{
  // Purity: target lookup never mutates the list.
  const list = [mk(1, "info", "A"), mk(2, "info", "A")];
  const before = ids(list);
  findCoalesceTarget(list, "info", "A");
  expect(ids(list) === before, "findCoalesce: does not mutate input");
}

// ── describeToastCount ───────────────────────────────────────────────

{
  expect(describeToastCount(1) === "", "count: 1 -> empty (no badge for single)");
  expect(describeToastCount(2) === "x2", "count: 2 -> x2");
  expect(describeToastCount(17) === "x17", "count: 17 -> x17");
  expect(describeToastCount(0) === "", "count: 0 -> empty");
  expect(describeToastCount(undefined) === "", "count: undefined -> empty");
  expect(describeToastCount(NaN) === "", "count: NaN -> empty");
  expect(describeToastCount(3.9) === "x3", "count: fractional floors");
}

// ── shouldShowClearAll + describeClearAll ────────────────────────────

{
  expect(TOAST_CLEAR_ALL_THRESHOLD === 2, "clearAll: threshold is 2");
  expect(shouldShowClearAll(0) === false, "clearAll: 0 -> hidden");
  expect(shouldShowClearAll(1) === false, "clearAll: 1 -> hidden (single has own x)");
  expect(shouldShowClearAll(2) === true, "clearAll: 2 -> shown");
  expect(shouldShowClearAll(9) === true, "clearAll: 9 -> shown");
  expect(shouldShowClearAll(-1) === false, "clearAll: negative -> hidden");
  expect(shouldShowClearAll(NaN) === false, "clearAll: NaN -> hidden");
}
{
  expect(describeClearAll(1) === "", "describeClearAll: below threshold -> empty");
  expect(describeClearAll(2) === "Clear all 2", "describeClearAll: 2 -> Clear all 2");
  expect(describeClearAll(12) === "Clear all 12", "describeClearAll: 12 -> Clear all 12");
  expect(describeClearAll(NaN) === "", "describeClearAll: NaN -> empty");
  expect(describeClearAll(3.5) === "Clear all 3", "describeClearAll: fractional floors");
}

// ── createToastTimer ─────────────────────────────────────────────────

{
  const t = createToastTimer(4000, 1000);
  expect(t.duration === 4000, "createTimer: duration set");
  expect(t.remaining === 4000, "createTimer: remaining seeds to full");
  expect(t.runningSince === 1000, "createTimer: clock starts at now");
  expect(isToastTimerPaused(t) === false, "createTimer: born running");
}
{
  // Sticky: duration <= 0 -> Infinity remaining, never expires.
  for (const bad of [0, -5, NaN, Infinity]) {
    const t = createToastTimer(bad, 1000);
    expect(t.duration === 0, `createTimer: sticky duration ${bad} -> 0`);
    expect(toastTimerRemaining(t, 9_999_999) === Infinity, `createTimer: sticky ${bad} -> Infinity remaining`);
    expect(toastTimerFraction(t, 9_999_999) === 1, `createTimer: sticky ${bad} -> fraction 1`);
    expect(isToastTimerExpired(t, 9_999_999) === false, `createTimer: sticky ${bad} never expires`);
  }
}

// ── toastTimerRemaining / fraction over a running clock ──────────────

{
  const t = createToastTimer(4000, 1000);
  expect(toastTimerRemaining(t, 1000) === 4000, "remaining: at birth == full");
  expect(toastTimerRemaining(t, 2000) === 3000, "remaining: 1s elapsed -> 3000");
  expect(toastTimerRemaining(t, 5000) === 0, "remaining: full elapsed -> 0");
  expect(toastTimerRemaining(t, 9000) === 0, "remaining: past end clamps at 0");
  expect(toastTimerFraction(t, 1000) === 1, "fraction: birth == 1");
  expect(toastTimerFraction(t, 3000) === 0.5, "fraction: half elapsed == 0.5");
  expect(toastTimerFraction(t, 5000) === 0, "fraction: end == 0");
  expect(toastTimerFraction(t, 9000) === 0, "fraction: past end clamps at 0");
  // A clock read BEFORE birth (clock skew) never exceeds 1.
  expect(toastTimerFraction(t, 500) === 1, "fraction: pre-birth clamps at 1");
}

// ── pause banks elapsed, freezes remaining ───────────────────────────

{
  const t0 = createToastTimer(4000, 1000);
  const paused = pauseToastTimer(t0, 2500); // 1500ms elapsed
  expect(isToastTimerPaused(paused) === true, "pause: now paused");
  expect(paused.remaining === 2500, "pause: banks 4000-1500 == 2500");
  // While paused, remaining is FROZEN regardless of wall clock.
  expect(toastTimerRemaining(paused, 9000) === 2500, "pause: remaining frozen while paused");
  expect(toastTimerFraction(paused, 9000) === 2500 / 4000, "pause: fraction frozen while paused");
}
{
  // Pause is idempotent + never banks negative.
  const t0 = createToastTimer(4000, 1000);
  const p1 = pauseToastTimer(t0, 2000);
  const p2 = pauseToastTimer(p1, 8000);
  expect(p2.remaining === p1.remaining, "pause: idempotent (second pause no-ops)");
  const late = pauseToastTimer(createToastTimer(1000, 0), 9999);
  expect(late.remaining === 0, "pause: past-expiry banks 0 not negative");
}
{
  // Pausing a sticky timer is a no-op.
  const sticky = createToastTimer(0, 1000);
  expect(pauseToastTimer(sticky, 2000) === sticky, "pause: sticky returns same ref");
}

// ── resume restarts the clock keeping banked remaining ───────────────

{
  const t0 = createToastTimer(4000, 1000);
  const paused = pauseToastTimer(t0, 2500); // 2500 banked
  const resumed = resumeToastTimer(paused, 10000);
  expect(isToastTimerPaused(resumed) === false, "resume: running again");
  expect(toastTimerRemaining(resumed, 10000) === 2500, "resume: banked remaining preserved at resume instant");
  expect(toastTimerRemaining(resumed, 11000) === 1500, "resume: counts down from resume, not original birth");
  expect(toastTimerRemaining(resumed, 12500) === 0, "resume: depletes banked remaining");
}
{
  // Resume is idempotent + sticky no-op.
  const running = createToastTimer(4000, 1000);
  expect(resumeToastTimer(running, 2000) === running, "resume: already-running returns same ref");
  const sticky = createToastTimer(0, 1000);
  expect(resumeToastTimer(sticky, 2000) === sticky, "resume: sticky returns same ref");
}
{
  // Full hover round-trip: pause mid-life, hover a while, resume, the
  // toast still has its remaining life — never vanishes underneath.
  let t = createToastTimer(3000, 0);
  t = pauseToastTimer(t, 1000); // 2000 left
  // ... user reads for 10 seconds ...
  t = resumeToastTimer(t, 11000);
  expect(toastTimerRemaining(t, 11000) === 2000, "round-trip: 2000 still banked after long hover");
  expect(toastTimerRemaining(t, 13000) === 0, "round-trip: depletes 2000 after resume");
  expect(isToastTimerExpired(t, 13000) === true, "round-trip: expired after remaining elapses");
}

// ── isToastTimerExpired ──────────────────────────────────────────────

{
  const t = createToastTimer(1000, 0);
  expect(isToastTimerExpired(t, 500) === false, "expired: mid-life false");
  expect(isToastTimerExpired(t, 1000) === true, "expired: at end true");
  expect(isToastTimerExpired(t, 2000) === true, "expired: past end true");
  const paused = pauseToastTimer(t, 400); // 600 banked, frozen
  expect(isToastTimerExpired(paused, 99999) === false, "expired: paused with life left never expires");
}

// ── toastPoliteness + toastKindLabel ─────────────────────────────────

{
  expect(toastPoliteness("error") === "assertive", "politeness: error -> assertive");
  expect(toastPoliteness("warning") === "assertive", "politeness: warning -> assertive");
  expect(toastPoliteness("success") === "polite", "politeness: success -> polite");
  expect(toastPoliteness("info") === "polite", "politeness: info -> polite");
}
{
  expect(toastKindLabel("success") === "Success", "kindLabel: success");
  expect(toastKindLabel("error") === "Error", "kindLabel: error");
  expect(toastKindLabel("warning") === "Warning", "kindLabel: warning");
  expect(toastKindLabel("info") === "Notice", "kindLabel: info -> Notice");
}

// ── announceToast ────────────────────────────────────────────────────

{
  expect(
    announceToast("error", "Render failed") === "Error: Render failed",
    "announce: severity prefix",
  );
  expect(
    announceToast("success", "Saved") === "Success: Saved",
    "announce: success prefix",
  );
  expect(
    announceToast("error", "Render failed", "Disk full") === "Error: Render failed. Disk full",
    "announce: detail appended after period",
  );
  // Blank/whitespace detail is skipped.
  expect(
    announceToast("info", "Hi", "   ") === "Notice: Hi",
    "announce: blank detail skipped",
  );
  expect(
    announceToast("info", "Hi", undefined) === "Notice: Hi",
    "announce: undefined detail skipped",
  );
  // Repeat suffix only for count > 1.
  expect(
    announceToast("warning", "Slow", undefined, 1) === "Warning: Slow",
    "announce: count 1 -> no repeat suffix",
  );
  expect(
    announceToast("warning", "Slow", undefined, 3) === "Warning: Slow (repeated 3 times)",
    "announce: count 3 -> repeat suffix",
  );
  expect(
    announceToast("error", "X", "Y", 2) === "Error: X. Y (repeated 2 times)",
    "announce: detail + repeat together",
  );
  expect(
    announceToast("info", "X", undefined, NaN) === "Notice: X",
    "announce: NaN count -> no suffix",
  );
}

// ── splitToastsByPoliteness ──────────────────────────────────────────

{
  const list = [
    mk(1, "success", "A"),
    mk(2, "error", "B"),
    mk(3, "info", "C"),
    mk(4, "warning", "D"),
  ];
  const s = splitToastsByPoliteness(list);
  expect(ids(s.assertive) === "2,4", "split: error+warning -> assertive");
  expect(ids(s.polite) === "1,3", "split: success+info -> polite");
  // Every toast lands in exactly one bucket.
  expect(
    s.assertive.length + s.polite.length === list.length,
    "split: partition is total (no toast dropped or duplicated)",
  );
}
{
  const s = splitToastsByPoliteness([]);
  expect(s.assertive.length === 0 && s.polite.length === 0, "split: empty -> empty buckets");
}
{
  // Purity: input order preserved within each bucket, input not mutated.
  const list = [mk(5, "error", "E"), mk(6, "error", "F")];
  const before = ids(list);
  const s = splitToastsByPoliteness(list);
  expect(ids(s.assertive) === "5,6", "split: preserves store order within bucket");
  expect(ids(list) === before, "split: does not mutate input");
}

// ── Toast action buttons (round-36 Slice 1) ──────────────────────────

{
  // normalizeToastAction: a usable action needs label + callable handler.
  const noop = () => {};
  expect(
    normalizeToastAction({ label: "Undo", onClick: noop })?.label === "Undo",
    "action: valid label + handler -> normalized",
  );
  expect(
    normalizeToastAction({ label: "Undo", onClick: noop })?.dismissOnClick === true,
    "action: dismissOnClick defaults true",
  );
  expect(
    normalizeToastAction({ label: "Keep", onClick: noop, dismissOnClick: false })
      ?.dismissOnClick === false,
    "action: explicit dismissOnClick false preserved",
  );
  expect(normalizeToastAction(undefined) === null, "action: undefined -> null");
  expect(normalizeToastAction(null) === null, "action: null -> null");
  expect(
    normalizeToastAction({ label: "", onClick: noop }) === null,
    "action: blank label -> null (dead button)",
  );
  expect(
    normalizeToastAction({ label: "   ", onClick: noop }) === null,
    "action: whitespace-only label -> null",
  );
  expect(
    // @ts-expect-error deliberately bad handler
    normalizeToastAction({ label: "Undo", onClick: "nope" }) === null,
    "action: non-function handler -> null (nothing to run)",
  );
  // Purity: returns a fresh object, input untouched.
  const raw: ToastAction = { label: "  Undo  ", onClick: noop };
  const norm = normalizeToastAction(raw);
  expect(norm !== null && norm !== raw, "action: returns fresh object");
  expect(raw.label === "  Undo  ", "action: does not mutate input label");
  expect(norm?.label === "Undo", "action: trims label");
}

{
  // clampToastActionLabel: trim + length clamp with ellipsis.
  expect(clampToastActionLabel("Undo") === "Undo", "clampLabel: short passes through");
  expect(clampToastActionLabel("  Retry  ") === "Retry", "clampLabel: trims");
  expect(clampToastActionLabel(42 as unknown) === "", "clampLabel: non-string -> empty");
  expect(clampToastActionLabel("") === "", "clampLabel: empty -> empty");
  const long = "Reconnect to the document server now";
  const clamped = clampToastActionLabel(long);
  expect(clamped.length <= TOAST_ACTION_LABEL_MAX, "clampLabel: never exceeds max");
  expect(clamped.endsWith("\u2026"), "clampLabel: truncated gets ellipsis");
  const exact = "x".repeat(TOAST_ACTION_LABEL_MAX);
  expect(
    clampToastActionLabel(exact) === exact,
    "clampLabel: exactly at max not truncated",
  );
}

{
  // hasToastAction / toastActionDismisses read the normalized action.
  const noop = () => {};
  const withAction: Pick<Toast, "action"> = { action: { label: "Undo", onClick: noop } };
  const noAction: Pick<Toast, "action"> = {};
  const deadAction: Pick<Toast, "action"> = { action: { label: "", onClick: noop } };
  expect(hasToastAction(withAction) === true, "hasAction: usable action -> true");
  expect(hasToastAction(noAction) === false, "hasAction: no action -> false");
  expect(hasToastAction(deadAction) === false, "hasAction: dead action -> false");
  expect(
    toastActionDismisses(withAction) === true,
    "actionDismisses: default action dismisses",
  );
  expect(
    toastActionDismisses(noAction) === false,
    "actionDismisses: no action -> false (no stray dismiss)",
  );
  expect(
    toastActionDismisses({ action: { label: "Keep", onClick: noop, dismissOnClick: false } }) ===
      false,
    "actionDismisses: opt-out action does not dismiss",
  );
}

// ── Swipe / drag-to-dismiss (round-36 Slice 2) ───────────────────────

{
  // createToastSwipe seeds a zero-offset drag.
  const s = createToastSwipe(100, 1000);
  expect(s.startX === 100 && s.dx === 0, "swipe: created at startX, dx 0");
  expect(s.startTime === 1000 && s.lastTime === 1000, "swipe: timestamps seeded");
}

{
  // moveToastSwipe: rightward accumulates, leftward clamps to 0.
  const s = createToastSwipe(100, 0);
  const right = moveToastSwipe(s, 150, 50);
  expect(right.dx === 50, "swipe: rightward drag accumulates dx");
  expect(right.lastTime === 50, "swipe: move updates lastTime");
  const left = moveToastSwipe(s, 40, 50);
  expect(left.dx === 0, "swipe: leftward clamps to 0 (can't drag off edge)");
  // Purity: original untouched.
  expect(s.dx === 0, "swipe: move does not mutate input");
  expect(right !== s, "swipe: move returns fresh object");
}

{
  // toastSwipeVelocity: px/ms over the gesture.
  const s = { startX: 0, startTime: 0, dx: 100, lastTime: 200 };
  expect(toastSwipeVelocity(s) === 0.5, "swipe: velocity = dx/dt");
  // Zero / negative dt guarded.
  expect(toastSwipeVelocity({ startX: 0, startTime: 5, dx: 50, lastTime: 5 }) === 0, "swipe: zero dt -> 0");
  expect(toastSwipeVelocity({ startX: 0, startTime: 10, dx: 50, lastTime: 5 }) === 0, "swipe: negative dt -> 0");
}

{
  // toastSwipeShouldDismiss: distance threshold OR flick velocity.
  // Past distance threshold, slow.
  const far = { startX: 0, startTime: 0, dx: TOAST_SWIPE_DISMISS_PX + 5, lastTime: 5000 };
  expect(toastSwipeShouldDismiss(far) === true, "swipe: past distance -> dismiss");
  // Below distance, slow -> snap back.
  const near = { startX: 0, startTime: 0, dx: 40, lastTime: 5000 };
  expect(toastSwipeShouldDismiss(near) === false, "swipe: short + slow -> keep");
  // Below distance but fast flick -> dismiss (40px in 50ms = 0.8px/ms).
  const flick = { startX: 0, startTime: 0, dx: 40, lastTime: 50 };
  expect(
    toastSwipeVelocity(flick) >= TOAST_SWIPE_FLICK_VELOCITY,
    "swipe: flick velocity above threshold (sanity)",
  );
  expect(toastSwipeShouldDismiss(flick) === true, "swipe: fast flick -> dismiss below distance");
  // A fast micro-jitter (<16px) does NOT dismiss even if velocity is high.
  const jitter = { startX: 0, startTime: 0, dx: 8, lastTime: 1 };
  expect(toastSwipeShouldDismiss(jitter) === false, "swipe: sub-16px jitter never dismisses");
  // Custom threshold honoured; bad threshold falls back to default.
  expect(
    toastSwipeShouldDismiss({ startX: 0, startTime: 0, dx: 30, lastTime: 9999 }, 20) === true,
    "swipe: custom threshold honoured",
  );
  expect(
    toastSwipeShouldDismiss(far, NaN) === true,
    "swipe: NaN threshold falls back to default",
  );
}

{
  // toastSwipeOpacity: 1 at rest, fades toward 0.25 floor.
  expect(toastSwipeOpacity(0) === 1, "swipeOpacity: no travel -> 1");
  expect(toastSwipeOpacity(-50) === 1, "swipeOpacity: leftward treated as 0 -> 1");
  expect(toastSwipeOpacity(TOAST_SWIPE_DISMISS_PX) === 0.25, "swipeOpacity: at threshold -> 0.25 floor");
  expect(toastSwipeOpacity(9999) === 0.25, "swipeOpacity: beyond threshold clamps at 0.25");
  const mid = toastSwipeOpacity(TOAST_SWIPE_DISMISS_PX / 2);
  expect(mid > 0.25 && mid < 1, "swipeOpacity: mid-drag between floor and 1");
  expect(toastSwipeOpacity(NaN) === 1, "swipeOpacity: NaN -> 1");
}

// ── Promise / loading lifecycle (round-36 Slice 3) ───────────────────

{
  // resolveToastMessage: string passes through, function gets the value.
  expect(resolveToastMessage("Saved", 5) === "Saved", "resolveMsg: string spec");
  expect(
    resolveToastMessage((n: number) => `Saved ${n} files`, 3) === "Saved 3 files",
    "resolveMsg: function spec sees value",
  );
  // A throwing / non-string formatter degrades to fallback.
  expect(
    resolveToastMessage(() => {
      throw new Error("boom");
    }, 0, "fb") === "fb",
    "resolveMsg: throwing formatter -> fallback",
  );
  expect(
    resolveToastMessage((() => 42) as unknown as (v: number) => string, 0, "fb") === "fb",
    "resolveMsg: non-string return -> fallback",
  );
  expect(
    resolveToastMessage(null as unknown as string, 0, "fb") === "fb",
    "resolveMsg: non-string/func spec -> fallback",
  );
}

{
  // describeToastError: Error message / string / generic.
  expect(describeToastError(new Error("disk full")) === "disk full", "describeErr: Error message");
  expect(describeToastError("nope") === "nope", "describeErr: plain string");
  expect(describeToastError("  trimmed  ") === "trimmed", "describeErr: trims string");
  expect(describeToastError(new Error("")) === "Something went wrong", "describeErr: empty Error -> generic");
  expect(describeToastError(undefined) === "Something went wrong", "describeErr: undefined -> generic");
  expect(describeToastError({ weird: 1 }) === "Something went wrong", "describeErr: object -> generic");
}

{
  // toastFulfilPatch: success kind, resolved message, loading off.
  const spec: ToastPromiseSpec<number> = {
    loading: "Saving…",
    success: (n) => `Saved ${n}`,
    error: "Failed",
  };
  const patch = toastFulfilPatch(spec, 4, 3000);
  expect(patch.kind === "success", "fulfil: kind success");
  expect(patch.message === "Saved 4", "fulfil: resolved message from value");
  expect(patch.loading === false, "fulfil: loading off");
  expect(patch.duration === 3000, "fulfil: applies settled duration");
  // String success spec.
  const patch2 = toastFulfilPatch({ loading: "L", success: "Done", error: "E" }, 0, 1000);
  expect(patch2.message === "Done", "fulfil: string success spec");
}

{
  // toastRejectPatch: error kind, error message, fallback to reason.
  const spec: ToastPromiseSpec<number> = {
    loading: "Uploading…",
    success: "Uploaded",
    error: (e) => `Upload failed: ${describeToastError(e)}`,
  };
  const patch = toastRejectPatch(spec, new Error("timeout"), 8000);
  expect(patch.kind === "error", "reject: kind error");
  expect(patch.message === "Upload failed: timeout", "reject: error formatter sees reason");
  expect(patch.loading === false, "reject: loading off");
  expect(patch.duration === 8000, "reject: applies settled duration");
  // A formatter that returns blank falls back to describeToastError.
  const blank = toastRejectPatch(
    { loading: "L", success: "S", error: () => "" },
    new Error("real reason"),
    5000,
  );
  expect(blank.message === "real reason", "reject: blank formatter -> describeToastError fallback");
}

{
  // isLoadingToast reads the loading flag.
  expect(isLoadingToast({ loading: true }) === true, "isLoading: true");
  expect(isLoadingToast({ loading: false }) === false, "isLoading: false");
  expect(isLoadingToast({}) === false, "isLoading: absent -> false");
}

// ── Keyboard focus + dismiss (round-36 Slice 4) ──────────────────────

{
  // resolveToastFocusHotkey: Alt+T, case-insensitive, no Cmd/Ctrl/Shift.
  expect(resolveToastFocusHotkey({ key: "t", altKey: true }) === true, "hotkey: Alt+t -> true");
  expect(resolveToastFocusHotkey({ key: "T", altKey: true }) === true, "hotkey: Alt+T (caps) -> true");
  expect(resolveToastFocusHotkey({ key: "t", altKey: false }) === false, "hotkey: no Alt -> false");
  expect(
    resolveToastFocusHotkey({ key: "t", altKey: true, metaKey: true }) === false,
    "hotkey: Alt+Cmd+t -> false (Cmd disqualifies)",
  );
  expect(
    resolveToastFocusHotkey({ key: "t", altKey: true, ctrlKey: true }) === false,
    "hotkey: Alt+Ctrl+t -> false",
  );
  expect(
    resolveToastFocusHotkey({ key: "t", altKey: true, shiftKey: true }) === false,
    "hotkey: Alt+Shift+t -> false",
  );
  expect(resolveToastFocusHotkey({ key: "k", altKey: true }) === false, "hotkey: wrong key -> false");
  expect(
    resolveToastFocusHotkey({ key: undefined as unknown as string, altKey: true }) === false,
    "hotkey: bad key -> false",
  );
}

{
  // resolveFocusedToastKey: Escape/Delete/Backspace dismiss; Enter/Space
  // act when there's an action; modifiers fall through.
  expect(resolveFocusedToastKey({ key: "Escape" }, false) === "dismiss", "focusKey: Escape -> dismiss");
  expect(resolveFocusedToastKey({ key: "Delete" }, false) === "dismiss", "focusKey: Delete -> dismiss");
  expect(resolveFocusedToastKey({ key: "Backspace" }, true) === "dismiss", "focusKey: Backspace -> dismiss");
  expect(resolveFocusedToastKey({ key: "Enter" }, true) === "action", "focusKey: Enter + action -> action");
  expect(resolveFocusedToastKey({ key: " " }, true) === "action", "focusKey: Space + action -> action");
  expect(resolveFocusedToastKey({ key: "Spacebar" }, true) === "action", "focusKey: legacy Spacebar -> action");
  expect(resolveFocusedToastKey({ key: "Enter" }, false) === "none", "focusKey: Enter no action -> none");
  expect(resolveFocusedToastKey({ key: "Tab" }, true) === "none", "focusKey: Tab -> none (falls through)");
  expect(
    resolveFocusedToastKey({ key: "Escape", metaKey: true }, false) === "none",
    "focusKey: Cmd+Escape -> none (modifier disqualifies)",
  );
  expect(
    resolveFocusedToastKey({ key: "Enter", altKey: true }, true) === "none",
    "focusKey: Alt+Enter -> none",
  );
}

{
  // pickToastFocusIndex: where focus lands after a dismiss.
  expect(pickToastFocusIndex(3, 1) === 1, "pickFocus: middle -> same index (sibling slides up)");
  expect(pickToastFocusIndex(2, 0) === 0, "pickFocus: first dismissed -> 0");
  expect(pickToastFocusIndex(2, 2) === 1, "pickFocus: last dismissed -> new last");
  expect(pickToastFocusIndex(2, 5) === 1, "pickFocus: out-of-range high -> clamps to last");
  expect(pickToastFocusIndex(0, 0) === -1, "pickFocus: emptied stack -> -1");
  expect(pickToastFocusIndex(-1, 0) === -1, "pickFocus: bad remaining -> -1");
  expect(pickToastFocusIndex(3, -2) === 0, "pickFocus: negative index -> 0");
}

{
  // newestToastFocusIndex: last index, -1 for empty.
  expect(newestToastFocusIndex(4) === 3, "newestFocus: 4 -> index 3");
  expect(newestToastFocusIndex(1) === 0, "newestFocus: 1 -> index 0");
  expect(newestToastFocusIndex(0) === -1, "newestFocus: empty -> -1");
  expect(newestToastFocusIndex(NaN) === -1, "newestFocus: NaN -> -1");
}

// eslint-disable-next-line no-console
console.log(`\n${passed} passed, ${failed} failed`);
