// Tests for the Beacon Cache Inspector sort-preference persistence shell.
//
// Style matches ocrSortStore.test.ts — no test runner, an inline `expect`,
// a minimal in-memory localStorage so the round-trip, default-clears-key,
// and garbage-tolerance branches are exercised without a browser.
//
// Run with:
//   tsx src/lib/beaconSortStore.test.ts

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
  raw(k: string): string | null {
    return this.getItem(k);
  }
}

const store = new MemStore();
// eslint-disable-next-line @typescript-eslint/no-explicit-any
(globalThis as any).localStorage = store;

const { loadBeaconSort, saveBeaconSort, DEFAULT_BEACON_SORT } = await import("./beaconSortStore");

const KEY = "slab.beacon.sort.v1";

let passed = 0;
let failed = 0;
function expect(cond: boolean, label: string): void {
  if (cond) {
    passed++;
  } else {
    failed++;
    // eslint-disable-next-line no-console
    console.error(`FAIL: ${label}`);
  }
}

// --- unset -> default (indexed-desc) ----------------------------------
store.clear();
expect(loadBeaconSort().field === "indexed", "unset: field defaults to indexed");
expect(loadBeaconSort().dir === "desc", "unset: dir defaults to desc");
expect(DEFAULT_BEACON_SORT.field === "indexed" && DEFAULT_BEACON_SORT.dir === "desc", "default: indexed-desc");

// --- round-trip a non-default sort ------------------------------------
saveBeaconSort({ field: "chunks", dir: "desc" });
expect(loadBeaconSort().field === "chunks", "round-trip: field persists");
expect(store.raw(KEY) === '{"field":"chunks","dir":"desc"}', "round-trip: stored as object");

// --- default clears the key -------------------------------------------
saveBeaconSort({ field: "indexed", dir: "desc" });
expect(store.raw(KEY) === null, "default: indexed-desc clears the key");
expect(loadBeaconSort().field === "indexed", "default: load after clear");

// --- name/model/pages survive -----------------------------------------
saveBeaconSort({ field: "name", dir: "asc" });
expect(loadBeaconSort().field === "name", "name: survives");
saveBeaconSort({ field: "model", dir: "asc" });
expect(loadBeaconSort().field === "model", "model: survives");

// --- garbage tolerance -------------------------------------------------
store.setItem(KEY, "not json");
expect(loadBeaconSort().field === "indexed", "garbage: bad json -> default");
store.setItem(KEY, '{"field":"bogus","dir":"asc"}');
expect(loadBeaconSort().field === "indexed", "garbage: unknown field -> default");
store.setItem(KEY, '{"field":"pages","dir":"up"}');
expect(loadBeaconSort().field === "indexed", "garbage: unknown dir -> default");

// --- save garbage is a no-op clear ------------------------------------
saveBeaconSort({ field: "chunks", dir: "asc" });
saveBeaconSort({ field: "nope" } as never);
expect(store.raw(KEY) === null, "save: garbage clears (never written)");

// --- fresh copy each call ---------------------------------------------
saveBeaconSort({ field: "chunks", dir: "asc" });
const a = loadBeaconSort();
a.field = "name";
expect(loadBeaconSort().field === "chunks", "load: returns fresh copy (no aliasing)");

// eslint-disable-next-line no-console
console.log(`\n${passed} passed, ${failed} failed`);
