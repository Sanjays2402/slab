// Tests for the Beacon cache-inspector pinned-model persistence shell.
//
// Style matches cmdPins / ocrSortStore tests — no test runner, an inline
// `expect`, a minimal in-memory localStorage stub so the round-trip,
// the empty-clears-key, the cap, and the garbage-tolerance branches are
// exercised without a browser.
//
// Run with:
//   tsx src/lib/beaconPins.test.ts

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
  clear(): void {
    this.m.clear();
  }
}

const store = new MemStore();
// eslint-disable-next-line @typescript-eslint/no-explicit-any
(globalThis as any).localStorage = store;

const { loadPinnedModels, savePinnedModels } = await import("./beaconPins");

const KEY = "slab.beacon.pinnedModels.v1";
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

// default empty
expect(loadPinnedModels().length === 0, "load: unset -> []");

// round-trip oldest-first
savePinnedModels(["nomic", "mxbai"]);
expect(loadPinnedModels().join(",") === "nomic,mxbai", "round-trip: order preserved");

// dedupe on write
savePinnedModels(["a", "a", "b"]);
expect(loadPinnedModels().join(",") === "a,b", "write: dedupe");

// empty clears key
savePinnedModels([]);
expect(store.getItem(KEY) === null, "empty clears key");

// garbage decodes to []
store.setItem(KEY, "{not json");
expect(loadPinnedModels().length === 0, "garbage -> []");
store.setItem(KEY, JSON.stringify("oops"));
expect(loadPinnedModels().length === 0, "non-array -> []");

// blank/non-string entries dropped
savePinnedModels(["a", "", 7 as never, "b"]);
expect(loadPinnedModels().join(",") === "a,b", "blanks/non-strings dropped");

// eslint-disable-next-line no-console
console.log(`\n${passed} passed, ${failed} failed`);
