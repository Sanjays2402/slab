// Tests for the OCR Queue sort-preference persistence shell.
//
// Style matches paletteCollapsed.test.ts — no test runner, an inline
// `expect`. We stub a minimal in-memory localStorage so the round-trip,
// the default-clears-key, and the garbage-tolerance branches are exercised
// without a browser.
//
// Run with:
//   tsx src/lib/ocrSortStore.test.ts

export {};

// --- minimal localStorage stub ---------------------------------------
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

const { loadOcrSort, saveOcrSort, DEFAULT_OCR_SORT } = await import("./ocrSortStore");

const KEY = "slab.ocr.sort.v1";

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

// --- unset -> default --------------------------------------------------
store.clear();
expect(loadOcrSort().field === "name", "unset: field defaults to name");
expect(loadOcrSort().dir === "asc", "unset: dir defaults to asc");
expect(DEFAULT_OCR_SORT.field === "name" && DEFAULT_OCR_SORT.dir === "asc", "default: name-asc");

// --- round-trip a non-default sort ------------------------------------
saveOcrSort({ field: "pages", dir: "desc" });
expect(loadOcrSort().field === "pages", "round-trip: field persists");
expect(loadOcrSort().dir === "desc", "round-trip: dir persists");
expect(store.raw(KEY) === '{"field":"pages","dir":"desc"}', "round-trip: stored as object");

// --- saving the default clears the key --------------------------------
saveOcrSort({ field: "name", dir: "asc" });
expect(store.raw(KEY) === null, "default: name-asc clears the key");
expect(loadOcrSort().field === "name", "default: load after clear -> name");

// --- folder/state survive ---------------------------------------------
saveOcrSort({ field: "folder", dir: "asc" });
expect(loadOcrSort().field === "folder", "folder: survives");
saveOcrSort({ field: "state", dir: "desc" });
expect(loadOcrSort().dir === "desc", "state: dir survives");

// --- garbage tolerance -------------------------------------------------
store.setItem(KEY, "not json");
expect(loadOcrSort().field === "name", "garbage: bad json -> default");
store.setItem(KEY, '{"field":"bogus","dir":"asc"}');
expect(loadOcrSort().field === "name", "garbage: unknown field -> default");
store.setItem(KEY, '{"field":"pages","dir":"sideways"}');
expect(loadOcrSort().field === "name", "garbage: unknown dir -> default");
store.setItem(KEY, "[]");
expect(loadOcrSort().field === "name", "garbage: array -> default");

// --- save garbage is a no-op clear ------------------------------------
saveOcrSort({ field: "folder", dir: "asc" });
saveOcrSort({ field: "nope", dir: "asc" } as never);
expect(store.raw(KEY) === null, "save: garbage clears (never written)");

// --- fresh copy each call (mutation isolation) ------------------------
saveOcrSort({ field: "pages", dir: "desc" });
const a = loadOcrSort();
a.field = "state";
expect(loadOcrSort().field === "pages", "load: returns fresh copy (no aliasing)");

// eslint-disable-next-line no-console
console.log(`\n${passed} passed, ${failed} failed`);
