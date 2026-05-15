export type CmdResult<T> =
  | { kind: "ok"; value: T }
  | { kind: "err"; message: string };

export type Status =
  | { kind: "idle"; msg: "" }
  | { kind: "working"; msg: string }
  | { kind: "ok"; msg: string }
  | { kind: "err"; msg: string };

export const idle: Status = { kind: "idle", msg: "" };

export function basename(p: string): string {
  const i = Math.max(p.lastIndexOf("/"), p.lastIndexOf("\\"));
  return i >= 0 ? p.slice(i + 1) : p;
}

export function stripExt(name: string): string {
  const i = name.lastIndexOf(".");
  return i > 0 ? name.slice(0, i) : name;
}

export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(2)} MB`;
}
