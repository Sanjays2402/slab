// Slab theme system — v1.0.0 "Glass".
//
// Owns the runtime translation between `UiConfig` (theme / accent /
// density) and the CSS variables / data-attributes on <html>. The
// stylesheet (`app.css`) defines the actual palette per
// `[data-theme="dark"]` / `[data-theme="light"]` blocks; this module
// just flips the attribute and persists the choice.
//
// Boot path (called from +layout.svelte once on mount):
//   1. Try to read `[ui]` from the Tauri backend. If we're in the
//      browser dev server, fall back to localStorage.
//   2. Apply to <html> immediately so there's no flash.
//   3. Hook `prefers-color-scheme` for "auto" mode.
//
// Save path (called from SettingsPanel on change):
//   1. Update the in-memory store + apply to <html>.
//   2. Persist via `slab_ui_config_write` (Tauri) or localStorage.

import { invoke } from "@tauri-apps/api/core";
import { writable, get } from "svelte/store";
import { isInTauri } from "$lib/tauri";

export type ThemeMode = "auto" | "light" | "dark";
export type AccentColor = "orange" | "blue" | "purple" | "green" | "pink";
export type Density = "comfortable" | "compact";

export interface UiConfig {
  theme: ThemeMode;
  accent: AccentColor;
  density: Density;
  /** Glass Slice 6: true once user has dismissed the onboarding tour. */
  onboarded: boolean;
}

export const ACCENT_COLORS: { id: AccentColor; label: string; hex: string }[] = [
  { id: "orange", label: "Slab Orange", hex: "#ff7a59" },
  { id: "blue", label: "Cobalt", hex: "#4f8cff" },
  { id: "purple", label: "Iris", hex: "#a780ff" },
  { id: "green", label: "Emerald", hex: "#3fc88c" },
  { id: "pink", label: "Coral", hex: "#ff6aa3" },
];

/** Built-in theme picks shown in the palette / Settings. */
export const BUILT_IN_THEMES: { id: ThemeMode; label: string; icon: string }[] = [
  { id: "auto", label: "Auto (match system)", icon: "◐" },
  { id: "light", label: "Light", icon: "☀" },
  { id: "dark", label: "Dark", icon: "☾" },
];

const DEFAULT_CONFIG: UiConfig = {
  theme: "auto",
  accent: "orange",
  density: "comfortable",
  onboarded: false,
};

const STORAGE_KEY = "slab.ui.config.v1";

/** Reactive snapshot of the live UI config. Components can subscribe. */
export const uiConfig = writable<UiConfig>({ ...DEFAULT_CONFIG });

/** Result of `slab_ui_config_*` Tauri commands. */
type CmdResult<T> = { kind: "ok"; value: T } | { kind: "err"; message: string };

function normalise(raw: unknown): UiConfig {
  const out: UiConfig = { ...DEFAULT_CONFIG };
  if (!raw || typeof raw !== "object") return out;
  const r = raw as Record<string, unknown>;
  if (typeof r.theme === "string" && (r.theme === "auto" || r.theme === "light" || r.theme === "dark")) {
    out.theme = r.theme;
  }
  if (
    typeof r.accent === "string" &&
    (r.accent === "orange" || r.accent === "blue" || r.accent === "purple" || r.accent === "green" || r.accent === "pink")
  ) {
    out.accent = r.accent;
  }
  if (typeof r.density === "string" && (r.density === "comfortable" || r.density === "compact")) {
    out.density = r.density;
  }
  if (typeof r.onboarded === "boolean") {
    out.onboarded = r.onboarded;
  }
  return out;
}

/**
 * Compute the effective "dark" vs "light" given the configured mode and
 * the host OS preference. Exported for tests; pure function.
 */
export function resolveTheme(mode: ThemeMode, prefersDark: boolean): "light" | "dark" {
  if (mode === "dark") return "dark";
  if (mode === "light") return "light";
  return prefersDark ? "dark" : "light";
}

/** Apply `cfg` to the <html> element. Pure side-effect. */
export function applyConfig(cfg: UiConfig): void {
  if (typeof document === "undefined") return;
  const root = document.documentElement;
  const prefersDark =
    typeof window !== "undefined" &&
    typeof window.matchMedia === "function" &&
    window.matchMedia("(prefers-color-scheme: dark)").matches;
  const resolved = resolveTheme(cfg.theme, prefersDark);
  root.setAttribute("data-theme", resolved);
  root.setAttribute("data-accent", cfg.accent);
  root.setAttribute("data-density", cfg.density);
  // Set CSS color-scheme so native form controls + scrollbars follow.
  root.style.colorScheme = resolved;
}

/** True if the configured mode follows the OS. */
let osListener: ((e: MediaQueryListEvent) => void) | null = null;
function hookOsThemeWatcher(cfg: UiConfig): void {
  if (typeof window === "undefined" || typeof window.matchMedia !== "function") return;
  const mql = window.matchMedia("(prefers-color-scheme: dark)");
  if (osListener) {
    mql.removeEventListener("change", osListener);
    osListener = null;
  }
  if (cfg.theme === "auto") {
    osListener = () => applyConfig(get(uiConfig));
    mql.addEventListener("change", osListener);
  }
}

/** Read the persisted config. Tries Tauri first, then localStorage. */
async function readPersisted(): Promise<UiConfig> {
  if (isInTauri()) {
    try {
      const res = (await invoke("slab_ui_config_read")) as CmdResult<UiConfig>;
      if (res.kind === "ok") return normalise(res.value);
    } catch {
      // fall through to localStorage
    }
  }
  if (typeof localStorage !== "undefined") {
    try {
      const raw = localStorage.getItem(STORAGE_KEY);
      if (raw) return normalise(JSON.parse(raw));
    } catch {
      // ignore
    }
  }
  return { ...DEFAULT_CONFIG };
}

/** Persist + apply. Idempotent; safe to call repeatedly. */
export async function setUiConfig(next: Partial<UiConfig>): Promise<void> {
  const merged: UiConfig = { ...get(uiConfig), ...next };
  uiConfig.set(merged);
  applyConfig(merged);
  hookOsThemeWatcher(merged);
  if (isInTauri()) {
    try {
      await invoke("slab_ui_config_write", { ui: merged });
      return;
    } catch (e) {
      // fall through to localStorage so the user's choice isn't lost.
      console.warn("[slab] slab_ui_config_write failed; falling back to localStorage", e);
    }
  }
  if (typeof localStorage !== "undefined") {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(merged));
    } catch {
      // ignore quota
    }
  }
}

/**
 * Boot the theme system. Called once from the root layout. Idempotent —
 * if it's called twice (e.g. HMR) the listener gets re-installed cleanly.
 */
export async function bootTheme(): Promise<void> {
  const cfg = await readPersisted();
  uiConfig.set(cfg);
  applyConfig(cfg);
  hookOsThemeWatcher(cfg);
}
