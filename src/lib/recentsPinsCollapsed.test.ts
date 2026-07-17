// Tests for the RecentsHome pinned-strip collapsed-state persistence shell.
//
// Style matches ocrSortStore / beaconPins tests — no test runner, an inline
// `expect`, a minimal in-memory localStorage stub. Exercises the round-trip,
// the expanded-clears-key, and the garbage-tolerance branches.
//
// Run with:
//   tsx src/lib/recentsPinsCollapsed.test.ts

export {};

class MemStore {
  private m = new Map<string, string>();
  getItem(k: string): string | null {
    return this.m.has(k) ? (this.m.get(k) as string) : null;
  }
  setItem(k: string, v: string): void {
    this.m.set(k, String(v));
  }
  removeItem(k: string): void {
    this.m.delete(k);
  }
}

const store = new MemStore();
// eslint-disable-next-line @typescript-eslint/no-explicit-any
(globalThis as any).localStorage = store;

const { loadPinsCollapsed, savePinsCollapsed } = await import("./recentsPinsCollapsed");

const KEY = "slab.recents.pinsCollapsed.v1";
let passed = 0;
let failed = 0;
function expect(cond: boolean, msg: string): void {
  if (cond) {
    passed++;
  } else {
    failed++;
    // eslint-disable-next-line no-console
    console.error(`FAIL: ${msg}`);
  }
}

// default expanded
expect(loadPinsCollapsed() === false, "load: unset -> false (expanded)");

// collapse persists
savePinsCollapsed(true);
expect(store.getItem(KEY) === "1", "collapse: writes flag");
expect(loadPinsCollapsed() === true, "round-trip: collapsed");

// expand clears key
savePinsCollapsed(false);
expect(store.getItem(KEY) === null, "expand clears key");
expect(loadPinsCollapsed() === false, "round-trip: expanded");

// garbage -> expanded
store.setItem(KEY, "yes");
expect(loadPinsCollapsed() === false, "garbage value -> false");

// eslint-disable-next-line no-console
console.log(`\n${passed} passed, ${failed} failed`);
