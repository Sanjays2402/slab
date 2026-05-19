// Slab plugin runtime — v1.3.0 "Foundry" Slice 9.
//
// Thin TypeScript adapter over the `slab_plugins_*` Tauri commands plus a
// Svelte store that holds the current snapshot of active contributions.
// Every other file consumes plugins through this module — no scattered
// `invoke("slab_plugins_*")` calls in components.

import { invoke } from "@tauri-apps/api/core";
import { writable, get } from "svelte/store";
import { isInTauri } from "$lib/tauri";

// ---------- type mirrors (must match Rust Serde output) ----------

export interface Plugin {
  dir: string;
  id: string;
  manifest: Manifest | null;
  enabled: boolean;
  error: string | null;
}

export interface Manifest {
  id: string;
  name: string;
  version: string;
  slab_compat: string;
  description: string;
  author: string;
  homepage: string;
  permissions: ("fs" | "net" | "spawn")[];
  contributions: {
    themes: ThemeContribution[];
    locales: LocaleContribution[];
    pdf_actions: PdfActionContribution[];
    commands: CommandContribution[];
    ai_providers: AiProviderContribution[];
  };
}

export interface ThemeContribution {
  id: string;
  label: string;
  css: string;
  dark: boolean;
}

export interface LocaleContribution {
  locale: string;
  bundle: string;
}

export interface PdfActionContribution {
  id: string;
  label: string;
  cli: string;
  args: string[];
  timeout_ms: number;
}

export interface CommandContribution {
  id: string;
  label: string;
  shell: string | null;
  url: string | null;
  default_keymap: string | null;
}

export interface AiProviderContribution {
  id: string;
  label: string;
  kind: string;
  base_url: string;
  default_model: string;
  headers: Record<string, string>;
}

// Active* types: contribution + plugin_id + plugin_dir (via #[serde(flatten)]
// on the Rust side — so each contribution field sits next to plugin_id at
// the top level).
export type ActiveTheme = ThemeContribution & { plugin_id: string; plugin_dir: string };
export type ActiveLocale = LocaleContribution & { plugin_id: string; plugin_dir: string };
export type ActiveCommand = CommandContribution & { plugin_id: string; plugin_dir: string };
export type ActiveAiProvider = AiProviderContribution & { plugin_id: string; plugin_dir: string };
export type ActivePdfAction = PdfActionContribution & { plugin_id: string; plugin_dir: string };

export type CommandOutcome =
  | {
      kind: "shell";
      command_id: string;
      plugin_id: string;
      status: "ok" | "nonzeroexit" | "timeout" | "spawnfailed";
      stdout: string;
      stderr: string;
      duration_ms: number;
    }
  | { kind: "url"; url: string };

export interface ActionReport {
  plugin_id: string;
  action_id: string;
  status: "ok" | "nonzeroexit" | "timeout" | "spawnfailed";
  stdout: string;
  stderr: string;
  duration_ms: number;
}

// ---------- store ----------

export interface PluginsSnapshot {
  plugins: Plugin[];
  themes: ActiveTheme[];
  locales: ActiveLocale[];
  commands: ActiveCommand[];
  aiProviders: ActiveAiProvider[];
  pdfActions: ActivePdfAction[];
  /** Last refresh timestamp (ms). 0 means "not loaded yet". */
  loadedAt: number;
}

const EMPTY: PluginsSnapshot = {
  plugins: [],
  themes: [],
  locales: [],
  commands: [],
  aiProviders: [],
  pdfActions: [],
  loadedAt: 0,
};

export const pluginsStore = writable<PluginsSnapshot>({ ...EMPTY });

// ---------- commands ----------

/** Returns whether the plugin system is even reachable (we're in Tauri). */
export function pluginsAvailable(): boolean {
  return isInTauri();
}

/** Refresh the in-memory snapshot. Safe to call repeatedly; no-op in browser. */
export async function refreshPlugins(): Promise<void> {
  if (!isInTauri()) {
    pluginsStore.set({ ...EMPTY });
    return;
  }
  try {
    const [plugins, themes, locales, commands, aiProviders, pdfActions] = await Promise.all([
      invoke<Plugin[]>("slab_plugins_list"),
      invoke<ActiveTheme[]>("slab_plugins_active_themes"),
      invoke<ActiveLocale[]>("slab_plugins_active_locales"),
      invoke<ActiveCommand[]>("slab_plugins_active_commands"),
      invoke<ActiveAiProvider[]>("slab_plugins_active_ai_providers"),
      invoke<ActivePdfAction[]>("slab_plugins_active_pdf_actions"),
    ]);
    pluginsStore.set({
      plugins,
      themes,
      locales,
      commands,
      aiProviders,
      pdfActions,
      loadedAt: Date.now(),
    });
  } catch (e) {
    console.warn("[slab] refreshPlugins failed", e);
    pluginsStore.set({ ...EMPTY });
  }
}

/** Set a plugin's enabled flag and refresh the snapshot. */
export async function setPluginEnabled(id: string, enabled: boolean): Promise<boolean> {
  if (!isInTauri()) return false;
  const ok = await invoke<boolean>("slab_plugins_set_enabled", { id, enabled });
  if (ok) await refreshPlugins();
  return ok;
}

/** Re-scan the plugins dir (e.g. after dropping a new plugin). */
export async function reloadPlugins(): Promise<Plugin[]> {
  if (!isInTauri()) return [];
  const fresh = await invoke<Plugin[]>("slab_plugins_reload");
  await refreshPlugins();
  return fresh;
}

/** Resolve a plugin's on-disk directory (creates `~/.slab/plugins` if missing). */
export async function pluginsDir(): Promise<string> {
  return invoke<string>("slab_plugins_dir");
}

/** Read a plugin asset (relative path). Errors propagate. */
export async function readPluginAsset(pluginId: string, relative: string): Promise<string> {
  return invoke<string>("slab_plugins_read_asset", { pluginId, relative });
}

/** Load a plugin locale bundle (flat `key → translation` map). */
export async function loadPluginLocaleBundle(
  pluginId: string,
  locale: string,
): Promise<Record<string, string>> {
  return invoke<Record<string, string>>("slab_plugins_load_locale_bundle", {
    pluginId,
    locale,
  });
}

/** Validate a plugin AI provider. Resolves with `undefined`; rejects with the error string. */
export async function validatePluginAiProvider(
  pluginId: string,
  providerId: string,
): Promise<void> {
  await invoke<void>("slab_plugins_validate_ai_provider", { pluginId, providerId });
}

// ---------- Workshop (v2.0.0) — capability grants ----------

/**
 * Per-plugin capability grant — the user's *decision* about what a
 * plugin is actually allowed to do, as opposed to the manifest's
 * declaration of what it *might* do.
 *
 * Shape mirrors `plugins::PluginGrants` on the Rust side. `Default`
 * is "deny all" — every field absent serialises to all-`none`.
 */
export interface PluginGrants {
  /** File-system read/write scope. */
  fs: "none" | "read" | "read-write";
  /** Network reach. */
  net: "none" | "specific" | "any";
  /** UI surfaces the plugin may install. */
  ui: "none" | "panel" | "tool" | "both";
  /** Beacon hooks the plugin may register. */
  beacon: "none" | "tool-provider" | "ai-provider" | "both";
  /** Allow-listed hosts when `net === "specific"`. */
  net_allow_hosts: string[];
  /** Allow-listed paths when `fs !== "none"`. */
  fs_allow_paths: string[];
}

/** Shape returned by `plugin_grants_get`. */
export interface PluginGrantsResponse {
  /** Has the user ever made an explicit grant decision for this plugin? */
  has_decision: boolean;
  /** Current grants (default = deny-all when `has_decision` is false). */
  grants: PluginGrants;
}

/** Deny-all default — useful as a baseline for the consent modal. */
export function emptyPluginGrants(): PluginGrants {
  return {
    fs: "none",
    net: "none",
    ui: "none",
    beacon: "none",
    net_allow_hosts: [],
    fs_allow_paths: [],
  };
}

/**
 * Fetch the user's grant decision for `pluginId`. Returns
 * `has_decision: false` + default grants when the user has never
 * decided. Cabinet's consent modal uses the flag to know whether to
 * show itself on plugin enable.
 */
export async function getPluginGrants(pluginId: string): Promise<PluginGrantsResponse> {
  if (!isInTauri()) return { has_decision: false, grants: emptyPluginGrants() };
  return invoke<PluginGrantsResponse>("plugin_grants_get", { pluginId });
}

/**
 * Persist a grant decision. Overwrites any previous decision. Rejects
 * with an error string on IO failure (e.g. read-only HOME).
 */
export async function setPluginGrants(pluginId: string, grants: PluginGrants): Promise<void> {
  if (!isInTauri()) return;
  await invoke<void>("plugin_grants_set", { pluginId, grants });
}

/**
 * Forget a plugin's grant decision. Triggers the consent modal on the
 * next enable. No-op when the plugin has no entry.
 */
export async function resetPluginGrants(pluginId: string): Promise<void> {
  if (!isInTauri()) return;
  await invoke<void>("plugin_grants_reset", { pluginId });
}

/** Run a plugin command (shell or url). */
export async function runPluginCommand(
  pluginId: string,
  commandId: string,
): Promise<CommandOutcome> {
  return invoke<CommandOutcome>("slab_plugins_run_command", { pluginId, commandId });
}

/** Run a plugin PDF action against `input`, writing to `output`. */
export async function runPluginPdfAction(
  pluginId: string,
  actionId: string,
  input: string,
  output: string,
): Promise<ActionReport> {
  return invoke<ActionReport>("slab_plugins_run_pdf_action", {
    pluginId,
    actionId,
    input,
    output,
  });
}

/** Sync accessor — read the current snapshot without subscribing. */
export function currentPlugins(): PluginsSnapshot {
  return get(pluginsStore);
}

/**
 * Log a one-line summary of plugin-contributed AI providers to the console.
 * Used by the layout boot as a discoverability aid until a Settings → Beacon
 * provider picker lands (v1.3.x).
 */
export function logActiveAiProviders(): void {
  const snap = currentPlugins();
  if (snap.aiProviders.length === 0) return;
  console.info(
    `[slab] ${snap.aiProviders.length} plugin AI provider(s) detected:`,
    snap.aiProviders.map((p) => `${p.id} (${p.kind}@${p.base_url})`),
  );
}
