// Frontend runtime keymap — companion to `src-tauri/src/keymap/`.
//
// Responsibilities:
//   1. `bootKeymap()` — Tauri-side load on app start; populates the
//      `keymapView` store and a fast lookup cache used by `matches()`.
//   2. `matches(event, actionId)` — the single source of truth used
//      across the app to test global shortcuts. Replaces the dozens
//      of hardcoded `e.metaKey && e.key === "k"` checks.
//   3. `writeKeymap(overrides)` / `resetKeymap()` — settings UI hooks.
//
// Storage format mirrors the Rust side: `"Mod+Shift+K"`. `Mod` is the
// platform-abstract modifier (Cmd on macOS, Ctrl elsewhere). We do
// the same platform resolution here so the binding-string round-trip
// is symmetric (typing `Mod+K` in Settings on mac = pressing ⌘K).
//
// The store starts empty; if `bootKeymap()` never runs (e.g. browser
// dev mode without Tauri), `matches()` returns `false` for every id
// and call-sites fall back to whatever default behaviour they encode
// — for now, the wired call-sites still have local fallbacks during
// the migration window. After every call-site is migrated, the
// fallbacks can be removed.

import { invoke } from "@tauri-apps/api/core";
import { writable, get } from "svelte/store";
import { isInTauri } from "$lib/tauri";

export type ActionId =
  | "palette.open"
  | "shortcuts.show"
  | "tabs.new"
  | "tabs.close"
  | "tabs.next"
  | "tabs.prev"
  | "tabs.goto1"
  | "tabs.goto2"
  | "tabs.goto3"
  | "tabs.goto4"
  | "tabs.goto5"
  | "tabs.goto6"
  | "tabs.goto7"
  | "tabs.goto8"
  | "tabs.goto9"
  | "find.open"
  | "zoom.in"
  | "zoom.out"
  | "beacon.send"
  | "library.search"
  | "theater.start"
  | "theater.next"
  | "theater.prev"
  | "theater.blackout"
  | "theater.ink"
  | "theater.exit"
  | "bedrock.open"
  | "press.open"
  | "forms.open"
  | "quill.batch"
  | "atelier.open"
  | "hopper.open";

export interface KeymapAction {
  id: ActionId;
  label: string;
  group: string;
  /** Canonical printable form, e.g. `"Mod+Shift+K"`. */
  binding: string;
  default_binding: string;
  is_override: boolean;
}

export interface KeymapView {
  actions: KeymapAction[];
  is_default: boolean;
}

type CmdResult<T> = { kind: "ok"; value: T } | { kind: "err"; message: string };

const IS_MAC =
  typeof navigator !== "undefined" &&
  /Mac|iPhone|iPad|iPod/.test(navigator.platform || navigator.userAgent || "");

const EMPTY_VIEW: KeymapView = { actions: [], is_default: true };

/** Reactive snapshot of the materialised keymap. */
export const keymapView = writable<KeymapView>(EMPTY_VIEW);

interface Parsed {
  mod: boolean;
  ctrl: boolean;
  alt: boolean;
  shift: boolean;
  /** Single-char keys stored upper-case; named keys keep KeyboardEvent.key casing. */
  key: string;
}

let parsedCache = new Map<ActionId, Parsed>();

/** Parse the canonical `"Mod+Shift+K"` string into a fast-match struct. */
function parseBinding(s: string): Parsed | null {
  const trimmed = s.trim();
  if (!trimmed) return null;
  // Handle `"Mod++"` → key is literally `+`.
  let tokens: string[];
  if (trimmed.endsWith("++")) {
    tokens = trimmed.slice(0, -2).split("+");
    tokens.push("+");
  } else if (trimmed === "+") {
    tokens = ["+"];
  } else {
    tokens = trimmed.split("+");
  }
  const out: Parsed = { mod: false, ctrl: false, alt: false, shift: false, key: "" };
  const last = tokens.length - 1;
  for (let i = 0; i < tokens.length; i++) {
    const tok = tokens[i].trim();
    if (i < last) {
      switch (tok) {
        case "Mod":
          out.mod = true;
          break;
        case "Ctrl":
          out.ctrl = true;
          break;
        case "Alt":
        case "Option":
        case "Opt":
          out.alt = true;
          break;
        case "Shift":
          out.shift = true;
          break;
        default:
          return null;
      }
    } else {
      out.key = tok.length === 1 && /[a-z]/.test(tok) ? tok.toUpperCase() : tok;
    }
  }
  if (!out.key) return null;
  return out;
}

function rebuildCache(view: KeymapView): void {
  parsedCache = new Map();
  for (const a of view.actions) {
    const p = parseBinding(a.binding);
    if (p) parsedCache.set(a.id, p);
  }
}

/**
 * Does `ev` match the binding for `id`? Returns `false` if `bootKeymap()`
 * hasn't run yet OR the binding string is malformed (defensive — the
 * backend prevents this but we still degrade gracefully in browser dev).
 *
 * Modifier semantics:
 *   - `Mod` → meta on macOS, ctrl elsewhere.
 *   - On macOS, an explicit `Ctrl` in the binding maps to the actual
 *     ctrlKey (so `Mod+Ctrl+K` = ⌘⌃K).
 *   - On non-mac, `Mod` IS ctrl, so we accept `Ctrl+K` and `Mod+K` as
 *     functionally equivalent — the explicit-ctrl flag is forced false
 *     after the Mod resolution to avoid double-counting.
 *   - Letters compare case-insensitively (so Shift+K still matches `K`).
 */
export function matches(ev: KeyboardEvent, id: ActionId): boolean {
  const b = parsedCache.get(id);
  if (!b) return false;
  const wantMod = b.mod;
  const wantCtrl = b.ctrl;
  const wantAlt = b.alt;
  const wantShift = b.shift;
  // Mod resolves per-platform.
  const gotMod = IS_MAC ? ev.metaKey : ev.ctrlKey;
  // Explicit Ctrl-in-the-binding only matters on macOS (where Mod = ⌘).
  // On non-mac, Mod IS ctrl, so the explicit-ctrl bit is treated as
  // already-covered and we don't double-check the same key.
  const gotExplicitCtrl = IS_MAC ? ev.ctrlKey : false;
  if (gotMod !== wantMod) return false;
  if (gotExplicitCtrl !== wantCtrl) return false;
  if (ev.altKey !== wantAlt) return false;
  if (ev.shiftKey !== wantShift) return false;
  // Compare keys.
  if (b.key.length === 1) {
    if (/[A-Z]/.test(b.key)) {
      // ASCII letter — case-insensitive.
      return ev.key.toLowerCase() === b.key.toLowerCase();
    }
    // Punctuation/digit — exact char.
    return ev.key === b.key;
  }
  // Named key (Tab, Enter, ArrowUp, …).
  return ev.key === b.key;
}

/**
 * Cold-boot the keymap from disk. Idempotent — calling twice is a no-op
 * after the first. Errors are silent: the app still works (matches()
 * just returns false until bound).
 */
let booted = false;
export async function bootKeymap(): Promise<void> {
  if (booted) return;
  booted = true;
  if (!isInTauri()) return;
  try {
    const res = (await invoke("slab_keymap_read")) as CmdResult<KeymapView>;
    if (res.kind === "ok") {
      keymapView.set(res.value);
      rebuildCache(res.value);
    }
  } catch {
    /* keep falling-back-to-false matches; app still usable */
  }
}

/** Apply a batch of (action_id, binding) overrides. Throws on backend error. */
export async function writeKeymap(overrides: Array<[ActionId, string]>): Promise<KeymapView> {
  if (!isInTauri()) {
    throw new Error("writeKeymap requires Tauri runtime");
  }
  const res = (await invoke("slab_keymap_write", {
    args: { overrides },
  })) as CmdResult<KeymapView>;
  if (res.kind === "err") throw new Error(res.message);
  keymapView.set(res.value);
  rebuildCache(res.value);
  return res.value;
}

/** Wipe every override — restore factory defaults. */
export async function resetKeymap(): Promise<KeymapView> {
  if (!isInTauri()) {
    throw new Error("resetKeymap requires Tauri runtime");
  }
  const res = (await invoke("slab_keymap_reset")) as CmdResult<KeymapView>;
  if (res.kind === "err") throw new Error(res.message);
  keymapView.set(res.value);
  rebuildCache(res.value);
  return res.value;
}

/** Pretty-print a binding for the active platform. `Mod` → ⌘ on mac, `Ctrl` elsewhere. */
export function prettyBinding(binding: string): string {
  const p = parseBinding(binding);
  if (!p) return binding;
  const parts: string[] = [];
  if (p.mod) parts.push(IS_MAC ? "⌘" : "Ctrl");
  if (p.ctrl) parts.push(IS_MAC ? "⌃" : "Ctrl");
  if (p.alt) parts.push(IS_MAC ? "⌥" : "Alt");
  if (p.shift) parts.push(IS_MAC ? "⇧" : "Shift");
  // Map a few named keys to icons.
  const keyMap: Record<string, string> = {
    Enter: "↵",
    Tab: "⇥",
    Escape: "Esc",
    ArrowUp: "↑",
    ArrowDown: "↓",
    ArrowLeft: "←",
    ArrowRight: "→",
    PageUp: "PgUp",
    PageDown: "PgDn",
    Backspace: "⌫",
    Delete: "⌦",
    Space: "␣",
  };
  parts.push(keyMap[p.key] ?? p.key);
  return parts.join(IS_MAC ? "" : "+");
}

/** Look up the canonical binding string for an action — useful for tooltips. */
export function bindingFor(id: ActionId): string {
  const v = get(keymapView);
  return v.actions.find((a) => a.id === id)?.binding ?? "";
}

/** Same, but pre-formatted for display. */
export function prettyBindingFor(id: ActionId): string {
  const s = bindingFor(id);
  return s ? prettyBinding(s) : "";
}

/**
 * Build the canonical binding string from a captured `KeyboardEvent`.
 * Used by the Settings UI rebind flow. Returns `null` if the captured
 * key is a modifier-only press (wait for a real key).
 */
export function bindingFromEvent(e: KeyboardEvent): string | null {
  if (["Control", "Shift", "Alt", "Meta", "Option"].includes(e.key)) return null;
  const parts: string[] = [];
  // mac: Mod = meta; non-mac: Mod = ctrl. Treat the platform mod key as `Mod`.
  if (IS_MAC) {
    if (e.metaKey) parts.push("Mod");
    if (e.ctrlKey) parts.push("Ctrl");
  } else {
    if (e.ctrlKey) parts.push("Mod");
    // On non-mac there's no separate explicit Ctrl to capture — it's the same key.
  }
  if (e.altKey) parts.push("Alt");
  if (e.shiftKey) parts.push("Shift");
  let key = e.key;
  // Single-letter → upper case.
  if (key.length === 1 && /[a-z]/i.test(key)) key = key.toUpperCase();
  parts.push(key);
  return parts.join("+");
}
