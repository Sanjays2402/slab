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
  };
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

// eslint-disable-next-line no-console
console.log(`\n${passed} passed, ${failed} failed`);
