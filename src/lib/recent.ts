// Persistent "recent files" store backed by localStorage.
// Slab never reads file contents from here — we only remember paths/names
// so the user can one-click reopen something they had open before.

const KEY = "slab.recent.v1";
const LIMIT = 8;

export type RecentFile = {
  path: string;        // absolute path on disk (or bare filename when run in browser dev)
  name: string;        // basename for display
  openedAt: number;    // unix ms
  pageCount?: number;  // optional cached page count
};

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
    );
  } catch {
    return [];
  }
}

function write(files: RecentFile[]) {
  if (typeof localStorage === "undefined") return;
  try {
    localStorage.setItem(KEY, JSON.stringify(files));
  } catch {
    // localStorage full — best effort, ignore
  }
  for (const l of listeners) l(files);
}

export function listRecent(): RecentFile[] {
  return read();
}

export function recordRecent(file: Omit<RecentFile, "openedAt"> & { openedAt?: number }) {
  const now = file.openedAt ?? Date.now();
  const cur = read().filter((r) => r.path !== file.path);
  cur.unshift({ ...file, openedAt: now });
  write(cur.slice(0, LIMIT));
}

export function clearRecent() {
  write([]);
}

export function subscribeRecent(fn: Listener): () => void {
  listeners.add(fn);
  fn(read());
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
