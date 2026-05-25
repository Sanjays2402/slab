// Smoke tests for Atlas Lite reading-progress fields on the recent store.
//
// Style matches `quill.test.ts` — no test runner dep, just an inline
// `expect` so the contract is readable. Run with:
//   node --import tsx src/lib/recent.progress.test.ts
// (or copy into the browser console after importing $lib/recent).
//
// Atlas Lite's value prop is "Slab remembers where you left off." This
// store is the single source of truth for that promise.

import {
  recordRecent,
  recordRecentProgress,
  getRecentProgress,
  listRecent,
  pinRecent,
  clearRecent,
} from "./recent";

// Polyfill localStorage when running under bare Node.
declare const globalThis: { localStorage?: Storage };
if (typeof globalThis.localStorage === "undefined") {
  const store = new Map<string, string>();
  globalThis.localStorage = {
    getItem: (k: string) => store.get(k) ?? null,
    setItem: (k: string, v: string) => void store.set(k, v),
    removeItem: (k: string) => void store.delete(k),
    clear: () => store.clear(),
    key: (i: number) => Array.from(store.keys())[i] ?? null,
    get length() {
      return store.size;
    },
  } as unknown as Storage;
}

function expect(cond: boolean, label: string): void {
  if (!cond) {
    // eslint-disable-next-line no-console
    console.error("FAIL:", label);
    if (typeof process !== "undefined") process.exitCode = 1;
  } else {
    // eslint-disable-next-line no-console
    console.log("ok  ", label);
  }
}

function reset() {
  localStorage.clear();
  // clearRecent preserves pinned, so a full localStorage clear is the only
  // way to start truly fresh between cases.
  clearRecent();
}

// --- 1. records lastPage + totalPages, preserves other fields --------------
reset();
recordRecent({ path: "/a.pdf", name: "a.pdf", pageCount: 10 });
recordRecentProgress("/a.pdf", { lastPage: 5, totalPages: 10 });
{
  const got = listRecent()[0];
  expect(got.lastPage === 5, "records lastPage");
  expect(got.totalPages === 10, "records totalPages");
  expect(got.name === "a.pdf", "preserves name");
  expect(got.pageCount === 10, "preserves pageCount");
}

// --- 2. reopening preserves saved lastPage ---------------------------------
reset();
recordRecent({ path: "/b.pdf", name: "b.pdf" });
recordRecentProgress("/b.pdf", { lastPage: 7, totalPages: 20 });
recordRecent({ path: "/b.pdf", name: "b.pdf" });
{
  const got = listRecent()[0];
  expect(got.lastPage === 7, "preserves lastPage across re-open");
  expect(got.totalPages === 20, "preserves totalPages across re-open");
}

// --- 3. pinned state coexists with progress --------------------------------
reset();
recordRecent({ path: "/c.pdf", name: "c.pdf" });
pinRecent("/c.pdf", true);
recordRecentProgress("/c.pdf", { lastPage: 3, totalPages: 9 });
recordRecent({ path: "/c.pdf", name: "c.pdf" });
{
  const got = listRecent().find((r) => r.path === "/c.pdf");
  expect(got?.pinned === true, "pin survives re-open");
  expect(got?.lastPage === 3, "progress survives pin + re-open");
}

// --- 4. no-op for unknown path ---------------------------------------------
reset();
recordRecentProgress("/never.pdf", { lastPage: 1, totalPages: 5 });
expect(listRecent().length === 0, "progress on unknown path does not create entry");

// --- 5. invalid values rejected --------------------------------------------
reset();
recordRecent({ path: "/d.pdf", name: "d.pdf" });
recordRecentProgress("/d.pdf", { lastPage: 0, totalPages: 10 });
recordRecentProgress("/d.pdf", { lastPage: 5, totalPages: 0 });
recordRecentProgress("/d.pdf", { lastPage: NaN, totalPages: 10 });
expect(listRecent()[0].lastPage === undefined, "rejects invalid progress");

// --- 6. clamps over-shoot lastPage to totalPages ---------------------------
reset();
recordRecent({ path: "/e.pdf", name: "e.pdf" });
recordRecentProgress("/e.pdf", { lastPage: 999, totalPages: 12 });
expect(listRecent()[0].lastPage === 12, "clamps lastPage to totalPages");

// --- 7. getRecentProgress shape --------------------------------------------
reset();
recordRecent({ path: "/f.pdf", name: "f.pdf" });
expect(getRecentProgress("/f.pdf") === undefined, "no progress before record");
recordRecentProgress("/f.pdf", { lastPage: 4, totalPages: 11 });
{
  const p = getRecentProgress("/f.pdf");
  expect(p?.lastPage === 4 && p?.totalPages === 11, "getRecentProgress returns saved tuple");
}

// --- 8. localStorage round-trip --------------------------------------------
reset();
recordRecent({ path: "/g.pdf", name: "g.pdf" });
recordRecentProgress("/g.pdf", { lastPage: 8, totalPages: 30 });
{
  const raw = localStorage.getItem("slab.recent.v1");
  expect(!!raw, "writes raw JSON");
  const parsed = JSON.parse(raw!);
  expect(parsed[0].lastPage === 8, "lastPage survives JSON round-trip");
  expect(parsed[0].totalPages === 30, "totalPages survives JSON round-trip");
}
