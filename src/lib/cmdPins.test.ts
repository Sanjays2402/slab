// Tests for the Command Palette pinned-command persistence shell.
//
// Style matches paletteCollapsed.test.ts — inline expect, MemStore.
//
// Run with:
//   tsx src/lib/cmdPins.test.ts

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

const { loadPinnedCommands, savePinnedCommands } = await import("./cmdPins");

const KEY = "slab.cmd.pinned.v1";

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

// --- unset -> empty ----------------------------------------------------
store.clear();
expect(loadPinnedCommands().length === 0, "unset: empty");

// --- round-trip preserves order ---------------------------------------
savePinnedCommands(["panel:reader", "accent:blue"]);
expect(loadPinnedCommands().join(",") === "panel:reader,accent:blue", "round-trip: order preserved");
expect(store.raw(KEY) === '["panel:reader","accent:blue"]', "round-trip: stored as array");

// --- empty clears the key ---------------------------------------------
savePinnedCommands([]);
expect(store.raw(KEY) === null, "empty: clears the key");

// --- dedupe + garbage filtering ---------------------------------------
savePinnedCommands(["a", "a", "", "b"] as never);
expect(loadPinnedCommands().join(",") === "a,b", "save: dedupe + drop blanks");
store.setItem(KEY, "not json");
expect(loadPinnedCommands().length === 0, "garbage: bad json -> []");
store.setItem(KEY, '{"x":1}');
expect(loadPinnedCommands().length === 0, "garbage: object -> []");
store.setItem(KEY, '["a", 5, null, "b"]');
expect(loadPinnedCommands().join(",") === "a,b", "garbage: mixed -> string ids only");

// eslint-disable-next-line no-console
console.log(`\n${passed} passed, ${failed} failed`);
