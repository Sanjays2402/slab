// In-app update check.
//
// Slab never phones home on its own: a check only runs when the user clicks
// "Check for updates" or has switched on "Check at launch" (off by default).
// The backend asks GitHub's public releases API for the latest stable release
// and returns whether it is newer than the running version.

import { invoke } from "@tauri-apps/api/core";
import type { CmdResult } from "$lib/types";

export interface UpdateInfo {
  current: string;
  latest: string;
  update_available: boolean;
  url: string;
  notes: string;
}

export const AUTO_CHECK_KEY = "slab.updates.autocheck";
export const LAST_CHECK_KEY = "slab.updates.lastcheck";
/** Launch checks run at most once per day. */
export const AUTO_CHECK_INTERVAL_MS = 24 * 60 * 60 * 1000;

/** Pure: is a launch check due, given the last check time (ms) or null? */
export function autoCheckDue(now: number, last: number | null): boolean {
  if (last === null || !Number.isFinite(last)) return true;
  // A clock set backwards must not suppress checks forever.
  if (last > now) return true;
  return now - last >= AUTO_CHECK_INTERVAL_MS;
}

/** Only ever open the release page on GitHub under the Slab repo. */
export function isSafeReleaseUrl(url: string): boolean {
  return url.startsWith("https://github.com/Sanjays2402/slab/");
}

function read(key: string): string | null {
  try { return localStorage.getItem(key); } catch { return null; }
}
function write(key: string, value: string): void {
  try { localStorage.setItem(key, value); } catch { /* storage unavailable */ }
}

export function getAutoCheck(): boolean {
  return read(AUTO_CHECK_KEY) === "1";
}
export function setAutoCheck(on: boolean): void {
  write(AUTO_CHECK_KEY, on ? "1" : "0");
}
export function getLastCheck(): number | null {
  const raw = read(LAST_CHECK_KEY);
  if (raw === null) return null;
  const n = Number(raw);
  return Number.isFinite(n) ? n : null;
}

/** Ask the backend for the latest release. Throws on any failure. */
export async function checkForUpdates(): Promise<UpdateInfo> {
  const res = await invoke<CmdResult<UpdateInfo>>("slab_check_for_updates");
  if (res.kind === "err") throw new Error(res.message);
  write(LAST_CHECK_KEY, String(Date.now()));
  return res.value;
}
