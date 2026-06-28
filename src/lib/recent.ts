// Persistent "recent files" store backed by localStorage.
// Slab never reads file contents from here — we only remember paths/names
// so the user can one-click reopen something they had open before.
//
// Thumbnails live in a SEPARATE storage key (`slab.recent.thumbs.v1`) so a
// quota crash on the thumb store doesn't kill the file list. If the thumb
// store can't take more, the oldest thumb is dropped first.
//
// v1.0.0 "Glass" added pinning: pinned items are NEVER auto-evicted by the
// LIMIT cap and always sort to the top of `listRecent()`. Pin state is
// persisted in the same record. Pinned items still respect the LIMIT for
// total cardinality (we keep at most LIMIT records total, but unpinned items
// are evicted first).

const KEY = "slab.recent.v1";
const THUMB_KEY = "slab.recent.thumbs.v1";
const LIMIT = 12; // bumped from 8 — pinning means users keep more around
const THUMB_LIMIT = 12;

export type RecentFile = {
  path: string;        // absolute path on disk (or bare filename when run in browser dev)
  name: string;        // basename for display
  openedAt: number;    // unix ms — last time this file was opened
  pageCount?: number;  // optional cached page count
  pinned?: boolean;    // Glass: user-pinned, floats to top, exempt from auto-evict
  // Stockade: user-defined order WITHIN the pinned strip on RecentsHome.
  // Lower sorts earlier. Absent until the user drags to reorder; the global
  // store sort (sortRecents) deliberately ignores it so nothing else is
  // perturbed — only the RecentsHome strip honours it via orderPinnedStrip.
  pinOrder?: number;
  // Atlas Lite: per-document reading progress so the reader can resume where
  // the user left off and the RecentsHome can render progress dots.
  lastPage?: number;     // 1-indexed last viewed page
  totalPages?: number;   // mirror of pageCount; set whenever progress is known
  lastReadAt?: number;   // unix ms — last time progress was saved (≠ openedAt)
};

type ThumbStore = Record<string, string>; // path -> data URL (JPEG)

type Listener = (files: RecentFile[]) => void;
const listeners = new Set<Listener>();

function read(): RecentFile[] {
  if (typeof localStorage === "undefined") return [];
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(
      (x): x is RecentFile =>
        x && typeof x.path === "string" && typeof x.name === "string" && typeof x.openedAt === "number",
    ).map((x: RecentFile) => {
      // Coerce optional numeric fields if present, drop them otherwise.
      const out: RecentFile = { path: x.path, name: x.name, openedAt: x.openedAt };
      if (typeof x.pageCount === "number") out.pageCount = x.pageCount;
      if (typeof x.pinned === "boolean") out.pinned = x.pinned;
      if (typeof x.pinOrder === "number" && Number.isFinite(x.pinOrder)) out.pinOrder = x.pinOrder;
      if (typeof x.lastPage === "number" && x.lastPage > 0) out.lastPage = x.lastPage;
      if (typeof x.totalPages === "number" && x.totalPages > 0) out.totalPages = x.totalPages;
      if (typeof x.lastReadAt === "number") out.lastReadAt = x.lastReadAt;
      return out;
    });
  } catch {
    return [];
  }
}

/** Sort pinned-first, then by openedAt desc. */
function sortRecents(files: RecentFile[]): RecentFile[] {
  return [...files].sort((a, b) => {
    const ap = a.pinned ? 1 : 0;
    const bp = b.pinned ? 1 : 0;
    if (ap !== bp) return bp - ap;
    return b.openedAt - a.openedAt;
  });
}

/**
 * Cap to LIMIT total entries, but never evict pinned entries to make room.
 * Unpinned entries past LIMIT are dropped (oldest first by openedAt).
 */
function capRecents(files: RecentFile[]): RecentFile[] {
  if (files.length <= LIMIT) return files;
  const pinned = files.filter((f) => f.pinned);
  const unpinned = files
    .filter((f) => !f.pinned)
    .sort((a, b) => b.openedAt - a.openedAt);
  const slots = Math.max(0, LIMIT - pinned.length);
  return [...pinned, ...unpinned.slice(0, slots)];
}

function write(files: RecentFile[]) {
  if (typeof localStorage === "undefined") {
    for (const l of listeners) l(sortRecents(files));
    return;
  }
  const capped = capRecents(files);
  try {
    localStorage.setItem(KEY, JSON.stringify(capped));
  } catch {
    // localStorage full — best effort, ignore
  }
  const sorted = sortRecents(capped);
  for (const l of listeners) l(sorted);
}

function readThumbs(): ThumbStore {
  if (typeof localStorage === "undefined") return {};
  try {
    const raw = localStorage.getItem(THUMB_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw);
    return typeof parsed === "object" && parsed !== null ? parsed : {};
  } catch {
    return {};
  }
}

function writeThumbs(thumbs: ThumbStore) {
  if (typeof localStorage === "undefined") return;
  try {
    localStorage.setItem(THUMB_KEY, JSON.stringify(thumbs));
  } catch {
    // Quota crash. Drop half the thumbs (oldest first) and retry once.
    // We use the recents list to determine order.
    const order = sortRecents(read()).map((r) => r.path);
    const keep: ThumbStore = {};
    // Keep only the first 6 entries that have thumbs.
    let kept = 0;
    for (const p of order) {
      if (kept >= 6) break;
      if (thumbs[p]) { keep[p] = thumbs[p]; kept++; }
    }
    try {
      localStorage.setItem(THUMB_KEY, JSON.stringify(keep));
    } catch {
      // Still failing — give up and clear thumbs entirely.
      try { localStorage.removeItem(THUMB_KEY); } catch { /* ignore */ }
    }
  }
}

export function listRecent(): RecentFile[] {
  return sortRecents(read());
}

export function recordRecent(file: Omit<RecentFile, "openedAt"> & { openedAt?: number }) {
  const now = file.openedAt ?? Date.now();
  const cur = read();
  // Preserve existing pinned state and reading-progress on update — don't let
  // recordRecent silently un-pin or wipe lastPage.
  const existing = cur.find((r) => r.path === file.path);
  const merged: RecentFile = {
    ...existing,
    ...file,
    openedAt: now,
    pinned: file.pinned ?? existing?.pinned ?? false,
    lastPage: file.lastPage ?? existing?.lastPage,
    totalPages: file.totalPages ?? existing?.totalPages,
    lastReadAt: file.lastReadAt ?? existing?.lastReadAt,
  };
  const next = [merged, ...cur.filter((r) => r.path !== file.path)];
  write(next);
}

/**
 * Update the reading position for a recent file. No-op if path not found
 * (we never auto-create on progress — opens are the only entry point).
 */
export function recordRecentProgress(
  path: string,
  progress: { lastPage: number; totalPages: number },
) {
  if (!Number.isFinite(progress.lastPage) || progress.lastPage < 1) return;
  if (!Number.isFinite(progress.totalPages) || progress.totalPages < 1) return;
  const cur = read();
  const idx = cur.findIndex((r) => r.path === path);
  if (idx < 0) return;
  const next = [...cur];
  next[idx] = {
    ...next[idx],
    lastPage: Math.min(progress.lastPage, progress.totalPages),
    totalPages: progress.totalPages,
    pageCount: progress.totalPages,
    lastReadAt: Date.now(),
  };
  write(next);
}

/**
 * Read the progress for a file, or undefined if no progress saved.
 * Returns lastPage/totalPages as a tuple-like object for callers.
 */
export function getRecentProgress(path: string):
  | { lastPage: number; totalPages: number }
  | undefined {
  const cur = read();
  const rec = cur.find((r) => r.path === path);
  if (!rec || !rec.lastPage || !rec.totalPages) return undefined;
  return { lastPage: rec.lastPage, totalPages: rec.totalPages };
}

/** Toggle (or set) the pinned state of a recent file. No-op if path not found. */
export function pinRecent(path: string, pinned?: boolean) {
  const cur = read();
  const idx = cur.findIndex((r) => r.path === path);
  if (idx < 0) return;
  const next = [...cur];
  next[idx] = { ...next[idx], pinned: pinned ?? !next[idx].pinned };
  write(next);
}

/**
 * Persist a user-defined order for the pinned strip. `orderedPaths` is the
 * desired left-to-right order of pinned cards; we stamp each matching
 * record's `pinOrder` to its index in that array so the RecentsHome strip
 * (via orderPinnedStrip) can render the manual order. Paths that aren't
 * currently pinned (or don't exist) are ignored. Records absent from the
 * list keep whatever pinOrder they had. The GLOBAL store sort is untouched
 * — only the strip reads pinOrder — so reordering can't perturb the hero,
 * the palette, or eviction. No-op when nothing matches.
 */
export function reorderPinned(orderedPaths: string[]) {
  if (!Array.isArray(orderedPaths) || orderedPaths.length === 0) return;
  const cur = read();
  const rank = new Map<string, number>();
  orderedPaths.forEach((p, i) => {
    if (typeof p === "string") rank.set(p, i);
  });
  let changed = false;
  const next = cur.map((r) => {
    const i = rank.get(r.path);
    // Only stamp pinned rows present in the order list; a non-pinned or
    // missing row is left exactly as-is.
    if (i === undefined || !r.pinned) return r;
    if (r.pinOrder === i) return r;
    changed = true;
    return { ...r, pinOrder: i };
  });
  if (changed) write(next);
}

/**
 * Clear every record's `pinOrder` stamp, dropping the strip back to the
 * store's natural pinned-first/openedAt-desc order. Backs the RecentsHome
 * "reset order" affordance — the inverse of reorderPinned. The GLOBAL sort
 * never read pinOrder, so this only affects the strip's resting order.
 * No-op when no record carries a stamp.
 */
export function clearPinOrder() {
  const cur = read();
  let changed = false;
  const next = cur.map((r) => {
    if (r.pinOrder === undefined) return r;
    changed = true;
    const { pinOrder: _drop, ...rest } = r;
    return rest;
  });
  if (changed) write(next);
}

/** Remove a single recent file (and its thumb). Pinned or not — user wins. */
export function removeRecent(path: string) {
  const cur = read();
  const next = cur.filter((r) => r.path !== path);
  write(next);
  const thumbs = readThumbs();
  if (thumbs[path]) {
    delete thumbs[path];
    writeThumbs(thumbs);
  }
}

export function clearRecent() {
  // Preserves pinned items by default — "Clear unpinned" semantics. Users who
  // truly want a clean slate can unpin first.
  const cur = read();
  const next = cur.filter((r) => r.pinned);
  write(next);
  // Drop thumbs for non-pinned only.
  const thumbs = readThumbs();
  const pinnedPaths = new Set(next.map((r) => r.path));
  const kept: ThumbStore = {};
  for (const [p, v] of Object.entries(thumbs)) {
    if (pinnedPaths.has(p)) kept[p] = v;
  }
  writeThumbs(kept);
}

export function setRecentThumb(path: string, dataUrl: string) {
  const thumbs = readThumbs();
  thumbs[path] = dataUrl;
  // Prune to THUMB_LIMIT — keep the ones referenced by current recents,
  // sorted (pinned first, then newest).
  const order = sortRecents(read()).map((r) => r.path);
  const pruned: ThumbStore = {};
  let count = 0;
  for (const p of order) {
    if (count >= THUMB_LIMIT) break;
    if (thumbs[p]) { pruned[p] = thumbs[p]; count++; }
  }
  writeThumbs(pruned);
}

export function getRecentThumb(path: string): string | undefined {
  return readThumbs()[path];
}

export function subscribeRecent(fn: Listener): () => void {
  listeners.add(fn);
  fn(sortRecents(read()));
  return () => listeners.delete(fn);
}

export function formatRelTime(ms: number): string {
  const now = Date.now();
  const diff = Math.max(0, now - ms);
  const sec = Math.floor(diff / 1000);
  if (sec < 60) return "just now";
  const min = Math.floor(sec / 60);
  if (min < 60) return `${min}m ago`;
  const hr = Math.floor(min / 60);
  if (hr < 24) return `${hr}h ago`;
  const days = Math.floor(hr / 24);
  if (days < 7) return `${days}d ago`;
  return new Date(ms).toLocaleDateString();
}
