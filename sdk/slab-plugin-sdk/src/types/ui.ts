/**
 * @slab/plugin-sdk — `slab.ui.*` types.
 *
 * `slab.ui` is the Cabinet (the plugin host's UI surface) — plugins
 * may register side-panels, toolbar tools, and emit toast notifications.
 *
 * Surface ground truth: `src-tauri/src/plugins/runtime/slab_global.rs`
 * lines 130-140. Registrations accumulate into the host's
 * `Registrations` struct and the Cabinet renderer consumes them at
 * post-enable time.
 */

/**
 * Severity tier for `slab.ui.notify(...)`. Maps to the host's toast
 * system: info (neutral), warn (soft alert), error (red).
 */
export type NotifyLevel = "info" | "warn" | "error";

/**
 * Descriptor passed to `slab.ui.registerPanel(...)`.
 *
 * Each registered panel becomes a Cabinet tab the user can pin to
 * the side-panel rail. `render` is called every time the panel is
 * shown; the optional teardown function returned by `render` runs
 * when the panel is hidden or the plugin is disabled.
 */
export interface UiPanel {
  /** Stable id; namespaced by plugin id host-side. */
  id: string;
  /** Tab label shown on the rail. */
  label: string;
  /**
   * Optional icon — either an inline SVG string or a `mdi:`-prefixed
   * Material Design Icons key (host resolves either form).
   */
  icon?: string;
  /**
   * Render callback. Receives the panel's root DOM element. Plugins
   * mount their UI inside `root` using whatever rendering strategy
   * they like (vanilla DOM, lit, mithril, etc.).
   *
   * If the callback returns a function, that function is invoked as
   * teardown when the panel is hidden or the plugin is disabled.
   */
  render(root: HTMLElement): void | (() => void);
}

/**
 * Descriptor passed to `slab.ui.registerTool(...)`.
 *
 * Tools appear in the Cabinet quick-actions toolbar and in the
 * Cmd/Ctrl+K command palette. `invoke` fires on click / palette
 * activation.
 */
export interface UiTool {
  /** Stable id; namespaced by plugin id host-side. */
  id: string;
  /** Display label shown in the toolbar + palette. */
  label: string;
  /** Optional icon (same conventions as UiPanel.icon). */
  icon?: string;
  /**
   * Optional default keyboard shortcut, e.g. `"Ctrl+Shift+H"`. Host
   * conflict-resolves: if another binding already owns the chord,
   * this one is registered without a chord and surfaced in the
   * shortcut-conflicts panel.
   */
  shortcut?: string;
  /**
   * Invocation handler. Sync or async. Throwing surfaces a toast
   * with the error message at `"error"` level.
   */
  invoke(): Promise<void> | void;
}

/**
 * The shape of `slab.ui`. All three methods are present on the
 * surface regardless of capability; the host enforces grants on
 * each individual call. `notify` is currently always permitted.
 */
export interface UiSurface {
  /**
   * Register a Cabinet side-panel. Requires `UiCap === "panel"` or
   * `"both"`.
   */
  registerPanel(panel: UiPanel): void;
  /**
   * Register a toolbar / command-palette tool. Requires
   * `UiCap === "tool"` or `"both"`.
   */
  registerTool(tool: UiTool): void;
  /**
   * Emit a transient toast notification. No capability gate (all
   * plugins can notify). Default `level` is `"info"`.
   */
  notify(message: string, level?: NotifyLevel): void;
}
