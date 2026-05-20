/**
 * @slab/plugin-sdk — Manifest types.
 *
 * Mirrors the live host structs in `src-tauri/src/plugins/manifest.rs`
 * and the typed wrapper in `src/lib/plugins.ts`. Authors of plugin
 * `manifest.toml` files can import these for parse/serialize tooling,
 * but the on-disk format is TOML, not JSON — these interfaces describe
 * the post-parse shape after the host loader runs.
 */

// ---------- capability lattices (declared upper bounds) ----------

/**
 * Declared filesystem capability bound. Plugins can `none` (default),
 * `read` (read-only), or `read-write` (mutating ops permitted). The
 * user can dial this DOWN at consent time but never UP.
 *
 * Ground truth: `plugins::manifest::FsCap` in src-tauri.
 */
export type FsCap = "none" | "read" | "read-write";

/**
 * Declared network capability bound.
 * - `none` — no host access at all
 * - `specific` — only hosts listed in `net_allow_hosts`
 * - `any` — every host (use sparingly; the consent modal will scream)
 */
export type NetCap = "none" | "specific" | "any";

/**
 * Declared UI surface bound.
 * - `none` — no UI registrations
 * - `panel` — may register a Cabinet side-panel
 * - `tool` — may register toolbar / quick-action tools
 * - `both` — panel + tool
 */
export type UiCap = "none" | "panel" | "tool" | "both";

/**
 * Declared Beacon (AI) hook bound.
 * - `none` — no Beacon hooks
 * - `tool-provider` — may `registerTool({...})` so the LLM can call it
 * - `ai-provider` — may `registerAiProvider({...})` (alt LLM endpoint)
 * - `both` — both of the above
 */
export type BeaconCap =
  | "none"
  | "tool-provider"
  | "ai-provider"
  | "both";

/**
 * The full set of declared capability upper bounds for a plugin.
 *
 * **Distinct from the user's grant.** This is what the plugin *asks
 * for*; the user's actual decision is stored in `PluginGrants` (see
 * `src/lib/plugins.ts`). The consent modal shows declared bounds and
 * lets the user dial each axis down (never up).
 */
export interface ManifestCapabilities {
  fs: FsCap;
  net: NetCap;
  ui: UiCap;
  beacon: BeaconCap;
  /** Hosts the plugin asks to reach when `net === "specific"`. */
  net_allow_hosts: string[];
  /** Paths the plugin asks to access when `fs !== "none"`. */
  fs_allow_paths: string[];
}

// ---------- runtime section ----------

/**
 * Optional `[runtime]` section in `manifest.toml` — present when the
 * plugin ships a `script.js` (or any other JS file) to be executed in
 * the QuickJS sandbox.
 *
 * The host verifies `sha256` against the on-disk bytes at load time
 * (TOFU + pin). A mismatch is a hard failure and the plugin loads in
 * error state.
 */
export interface RuntimeManifest {
  /** Path to the JS entry file, relative to the plugin directory. */
  entry: string;
  /** Lowercase hex SHA-256 of the entry file (64 chars). */
  sha256: string;
  /** Declared capability upper bounds. */
  capabilities: ManifestCapabilities;
}

// ---------- contribution interfaces ----------

export interface ThemeContribution {
  id: string;
  label: string;
  /** Path to a CSS file relative to the plugin dir. */
  css: string;
  dark: boolean;
}

export interface LocaleContribution {
  /** BCP-47 tag, e.g. `en-US`, `pt-BR`. */
  locale: string;
  /** Path to the flat-JSON bundle relative to the plugin dir. */
  bundle: string;
}

export interface PdfActionContribution {
  id: string;
  label: string;
  /** Argv[0] for the external action. Resolved via PATH. */
  cli: string;
  /** Remaining argv; `{INPUT}` / `{OUTPUT}` placeholders supported. */
  args: string[];
  /** Hard timeout in milliseconds (default 30_000 host-side). */
  timeout_ms: number;
}

export interface CommandContribution {
  id: string;
  label: string;
  /** Inline shell command — mutually exclusive with `url`. */
  shell: string | null;
  /** URL to open externally — mutually exclusive with `shell`. */
  url: string | null;
  /** Default keyboard shortcut, e.g. `"Ctrl+Shift+P"`. */
  default_keymap: string | null;
}

export interface AiProviderContribution {
  id: string;
  label: string;
  /** Provider kind tag. Host validates this; see Slab AI docs. */
  kind: string;
  base_url: string;
  default_model: string;
  headers: Record<string, string>;
}

// ---------- top-level manifest ----------

/**
 * The complete post-parse shape of a Slab plugin `manifest.toml`.
 *
 * Backward-compat with v1.x: `runtime` is `null` for declarative-only
 * plugins (themes / locales / actions / commands / AI providers
 * without a `script.js`). New plugins in v2.0.0+ typically set
 * `runtime` and use `script.js` for behaviour.
 */
export interface SlabManifest {
  id: string;
  name: string;
  version: string;
  /**
   * Semver range describing which Slab versions this plugin
   * supports — e.g. `">=2.0.0 <3.0.0"`.
   */
  slab_compat: string;
  description: string;
  author: string;
  homepage: string;
  /** Legacy v1.x permissions axis. New plugins should leave empty. */
  permissions: ("fs" | "net" | "spawn")[];
  contributions: {
    themes: ThemeContribution[];
    locales: LocaleContribution[];
    pdf_actions: PdfActionContribution[];
    commands: CommandContribution[];
    ai_providers: AiProviderContribution[];
  };
  /**
   * v2.0.0 Workshop runtime descriptor. `null` for declarative-only
   * plugins; non-null when the plugin ships a JS entry file.
   */
  runtime: RuntimeManifest | null;
}
