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
  type Toast,
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

// eslint-disable-next-line no-console
console.log(`\n${passed} passed, ${failed} failed`);
