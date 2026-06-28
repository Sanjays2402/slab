// Tests for the Library Search sort-preference persistence shell.
//
// Style matches ocrSortStore.test.ts — inline expect, MemStore localStorage.
//
// Run with:
//   tsx src/lib/librarySortStore.test.ts

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

const { loadLibrarySort, saveLibrarySort, DEFAULT_LIBRARY_SORT } = await import("./librarySortStore");

const KEY = "slab.library.sort.v1";

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
expect(loadLibrarySort() === "relevance", "unset: defaults to relevance");
expect(DEFAULT_LIBRARY_SORT === "relevance", "default: relevance");

// --- round-trip a non-default mode ------------------------------------
saveLibrarySort("document");
expect(loadLibrarySort() === "document", "round-trip: document persists");
expect(store.raw(KEY) === "document", "round-trip: stored raw");
saveLibrarySort("matches");
expect(loadLibrarySort() === "matches", "round-trip: matches persists");

// --- default clears the key -------------------------------------------
saveLibrarySort("relevance");
expect(store.raw(KEY) === null, "default: relevance clears the key");
expect(loadLibrarySort() === "relevance", "default: load after clear");

// --- garbage tolerance -------------------------------------------------
store.setItem(KEY, "bogus");
expect(loadLibrarySort() === "relevance", "garbage: unknown mode -> default");
store.setItem(KEY, "");
expect(loadLibrarySort() === "relevance", "garbage: empty -> default");

// --- save garbage is a no-op clear ------------------------------------
saveLibrarySort("document");
saveLibrarySort("nope" as never);
expect(store.raw(KEY) === null, "save: garbage clears (never written)");

// eslint-disable-next-line no-console
console.log(`\n${passed} passed, ${failed} failed`);
