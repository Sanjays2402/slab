// Tests for the palette collapsed-group persistence shell.
//
// Style matches paletteSearch.test.ts — no test runner, an inline `expect`.
// We stub a minimal in-memory localStorage so the round-trip + the
// garbage-tolerance branches are exercised without a browser.
//
// Run with:
//   tsx src/lib/paletteCollapsed.test.ts

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
  /** Test helper: peek the raw stored value. */
  raw(k: string): string | null {
    return this.getItem(k);
  }
}

const store = new MemStore();
// eslint-disable-next-line @typescript-eslint/no-explicit-any
(globalThis as any).localStorage = store;

// Import AFTER the stub is in place (the module reads typeof localStorage).
const { loadCollapsedGroups, saveCollapsedGroups } = await import("./paletteCollapsed");

const KEY = "slab.palette.collapsed.v1";

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

function eqSet(a: Set<string>, b: string[]): boolean {
  if (a.size !== b.length) return false;
  return b.every((x) => a.has(x));
}

// --- unset -> empty ---------------------------------------------------
{
  store.clear();
  expect(eqSet(loadCollapsedGroups(), []), "load: unset key -> empty set");
}

// --- round-trip -------------------------------------------------------
{
  store.clear();
  saveCollapsedGroups(new Set(["Appearance", "Library"]));
  expect(eqSet(loadCollapsedGroups(), ["Appearance", "Library"]), "roundtrip: save then load");
  // Stored as a JSON array of the names.
  expect(store.raw(KEY) === JSON.stringify(["Appearance", "Library"]), "roundtrip: stored as JSON array");
}

// --- empty set removes the key (all-open == fresh) --------------------
{
  store.clear();
  saveCollapsedGroups(new Set(["X"]));
  expect(store.raw(KEY) !== null, "empty: key present after a save");
  saveCollapsedGroups(new Set());
  expect(store.raw(KEY) === null, "empty: saving an empty set removes the key");
  expect(eqSet(loadCollapsedGroups(), []), "empty: load after clear -> empty");
}

// --- garbage tolerance ------------------------------------------------
{
  store.clear();
  store.setItem(KEY, "not json {{");
  expect(eqSet(loadCollapsedGroups(), []), "garbage: malformed JSON -> empty");

  store.setItem(KEY, JSON.stringify({ not: "an array" }));
  expect(eqSet(loadCollapsedGroups(), []), "garbage: non-array -> empty");

  store.setItem(KEY, JSON.stringify(["Good", 42, null, "", "AlsoGood"]));
  expect(eqSet(loadCollapsedGroups(), ["Good", "AlsoGood"]), "garbage: filters non-string / empty entries");
}

// --- cap --------------------------------------------------------------
{
  store.clear();
  const many = Array.from({ length: 100 }, (_, i) => `G${i}`);
  saveCollapsedGroups(new Set(many));
  expect(loadCollapsedGroups().size === 64, "cap: stored set capped at 64");
}

// eslint-disable-next-line no-console
console.log(`\n${passed} passed, ${failed} failed`);
