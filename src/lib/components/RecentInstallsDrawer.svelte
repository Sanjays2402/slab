<script lang="ts">
  // RecentInstallsDrawer — v3.39 Slice 57 + Slice 62.
  //
  // 460px right-side slide-in drawer surfacing the corpus-wide
  // marketplace install log: every install / update / uninstall /
  // failure across all plugins, newest first. Triggered from the
  // PluginsPanel toolbar "History" button which is gated on the
  // log having at least one event (no nag UI on a brand-new install
  // with empty history).
  //
  // Slice 62 (Install-log export round-13) adds an "Export…" menu in
  // the footer with CSV and JSON entries that respect the currently
  // selected window (7d / 30d / All) so what you see is what gets
  // exported. The export goes through a native save-as dialog; the
  // Tauri layer owns the actual file write.
  //
  // The drawer is otherwise purely presentational — the parent owns
  // the open state, toast wiring, and the post-prune refresh.

  import { onMount } from "svelte";
  import { save as saveDialog } from "@tauri-apps/plugin-dialog";
  import {
    exportInstallLogCsv,
    exportInstallLogJson,
    formatBytes,
    formatInstallEventTime,
    formatLogSpan,
    installEventGlyph,
    installLogSummary,
    listRecentInstallEvents,
    pruneInstallLog,
    suggestInstallLogExportFilename,
    type InstallEvent,
    type InstallLogExportFilter,
    type InstallLogSummary,
  } from "$lib/marketplace";

  type Props = {
    onClose: () => void;
    /**
     * Called after a successful prune so the parent (PluginsPanel) can
     * refresh its toolbar badge gating. Optional — drawer works
     * standalone if the parent doesn't care.
     */
    onPruned?: (rowsRemoved: number) => void;
  };

  let { onClose, onPruned }: Props = $props();

  let events = $state<InstallEvent[]>([]);
  let summary = $state<InstallLogSummary>({
    total_events: 0,
    distinct_plugins: 0,
    oldest_occurred_at: null,
  });
  let loading = $state(false);
  let err = $state<string | null>(null);

  /** "Last 7d" / "Last 30d" / "All" filter over the loaded events. */
  let windowChoice = $state<"7d" | "30d" | "all">("all");
  /** The pruning is gated behind this small confirm dialog. */
  let confirmingPrune = $state(false);
  let pruning = $state(false);
  /** Open state for the footer Export… popover (Slice 62). */
  let exportMenuOpen = $state(false);
  /** True while a save-as dialog or backend write is in flight. */
  let exporting = $state(false);
  /** Slim 4-second toast for export success. */
  let exportToast = $state<string | null>(null);

  let filteredEvents = $derived.by<InstallEvent[]>(() => {
    if (windowChoice === "all") return events;
    const nowSec = Math.floor(Date.now() / 1000);
    const cutoff = nowSec - (windowChoice === "7d" ? 7 : 30) * 86_400;
    return events.filter((ev) => ev.occurred_at >= cutoff);
  });

  /**
   * The since-unix cutoff implied by the current window choice — fed
   * into the export filter so a "Last 7d" export ships only the 7d
   * window rather than the whole loaded buffer.
   */
  let windowSinceUnix = $derived.by<number | null>(() => {
    if (windowChoice === "all") return null;
    const nowSec = Math.floor(Date.now() / 1000);
    return nowSec - (windowChoice === "7d" ? 7 : 30) * 86_400;
  });

  /**
   * "N events across X days" subtitle from the summary, plus the
   * filtered-count addendum when the window narrows the visible set.
   */
  let subtitleText = $derived.by<string>(() => {
    const base = formatLogSpan(summary);
    if (windowChoice === "all" || filteredEvents.length === events.length) {
      return base;
    }
    return `${base} · showing ${filteredEvents.length} in window`;
  });

  async function load(): Promise<void> {
    loading = true;
    err = null;
    try {
      const [es, sm] = await Promise.all([
        listRecentInstallEvents(100),
        installLogSummary(),
      ]);
      events = es;
      summary = sm;
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    void load();
  });

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      if (exportMenuOpen) {
        exportMenuOpen = false;
      } else if (confirmingPrune) {
        confirmingPrune = false;
      } else {
        onClose();
      }
    }
  }

  async function runPrune(retainDays: number): Promise<void> {
    pruning = true;
    try {
      const removed = await pruneInstallLog(retainDays);
      confirmingPrune = false;
      await load();
      onPruned?.(removed);
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    } finally {
      pruning = false;
    }
  }

  function flashToast(msg: string): void {
    exportToast = msg;
    window.setTimeout(() => {
      exportToast = null;
    }, 4000);
  }

  async function runExport(kind: "csv" | "json"): Promise<void> {
    exportMenuOpen = false;
    exporting = true;
    try {
      const filter: InstallLogExportFilter = {
        since_unix: windowSinceUnix,
      };
      const defaultPath = suggestInstallLogExportFilename(filter, kind);
      const target = await saveDialog({
        defaultPath,
        filters: [
          kind === "csv"
            ? { name: "CSV", extensions: ["csv"] }
            : { name: "JSON", extensions: ["json"] },
        ],
      });
      if (!target) return; // user cancelled
      const bytes =
        kind === "csv"
          ? await exportInstallLogCsv(target, filter)
          : await exportInstallLogJson(target, filter);
      const count = filteredEvents.length;
      flashToast(
        `Exported ${count} event${count === 1 ? "" : "s"} (${formatBytes(bytes)})`,
      );
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    } finally {
      exporting = false;
    }
  }

  /**
   * Dismiss the export popover on outside click — matches the
   * Notion / Linear convention. The check uses .closest() on the
   * anchor wrapper so clicks INSIDE the menu (its buttons) don't
   * dismiss it before the handler fires.
   */
  function onWindowClick(e: MouseEvent): void {
    if (!exportMenuOpen) return;
    const target = e.target as HTMLElement | null;
    if (target && !target.closest(".export-anchor")) {
      exportMenuOpen = false;
    }
  }
</script>

<svelte:window onkeydown={onKeydown} onclick={onWindowClick} />

<div
  class="backdrop"
  role="dialog"
  aria-modal="true"
  aria-labelledby="recent-installs-title"
  onclick={(e) => {
    if (e.target === e.currentTarget) onClose();
  }}
  onkeydown={(e) => {
    if (e.key === "Escape") onClose();
  }}
  tabindex="-1"
>
  <aside class="drawer" role="document">
    <header>
      <div class="title-block">
        <h1 id="recent-installs-title">Recent installs</h1>
        <p class="sub">{subtitleText}</p>
      </div>
      <button type="button" class="icon-btn" aria-label="Close" onclick={onClose} disabled={pruning}
        >✕</button
      >
    </header>

    <div class="window-strip seg" role="tablist" aria-label="Time window">
      {#each [
        { id: "7d" as const, label: "Last 7d" },
        { id: "30d" as const, label: "Last 30d" },
        { id: "all" as const, label: "All" },
      ] as opt (opt.id)}
        <button
          type="button"
          role="tab"
          aria-selected={windowChoice === opt.id}
          class:active={windowChoice === opt.id}
          onclick={() => (windowChoice = opt.id)}>{opt.label}</button
        >
      {/each}
    </div>

    {#if err}
      <p class="err">Could not load history: {err}</p>
    {:else if loading}
      <p class="loading">Loading history…</p>
    {:else if filteredEvents.length === 0}
      <p class="empty">
        {events.length === 0
          ? "No install history yet. Browse the Marketplace tab to install your first plugin."
          : `No events in the last ${windowChoice}. Try widening the window.`}
      </p>
    {:else}
      <ul class="event-list" aria-label="Install events (newest first)">
        {#each filteredEvents as ev (ev.id)}
          <li class="event-row" data-action={ev.action}>
            <span class="ev-glyph" aria-hidden="true">{installEventGlyph(ev.action)}</span>
            <div class="ev-body">
              <span class="ev-line">
                <span class="ev-action">{ev.action}</span>
                <span class="ev-plugin">{ev.plugin_id}</span>
                <span class="ev-version">v{ev.version}</span>
                {#if ev.action === "update" && ev.prior_version}
                  <span class="ev-from">← v{ev.prior_version}</span>
                {/if}
              </span>
              {#if ev.error_msg}
                <span class="ev-err" title={ev.error_msg}>{ev.error_msg}</span>
              {/if}
            </div>
            <time class="ev-time" datetime={new Date(ev.occurred_at * 1000).toISOString()}>
              {formatInstallEventTime(ev.occurred_at)}
            </time>
          </li>
        {/each}
      </ul>
    {/if}

    <footer class="drawer-foot">
      {#if exportToast}
        <span class="export-toast" role="status">{exportToast}</span>
      {/if}
      {#if !confirmingPrune}
        <div class="export-anchor">
          <button
            type="button"
            class="ghost"
            onclick={() => (exportMenuOpen = !exportMenuOpen)}
            disabled={summary.total_events === 0 || pruning || exporting}
            aria-haspopup="menu"
            aria-expanded={exportMenuOpen}
          >
            {exporting ? "Exporting…" : "Export…"}
          </button>
          {#if exportMenuOpen}
            <div class="export-menu" role="menu" aria-label="Export install log">
              <button type="button" role="menuitem" onclick={() => void runExport("csv")}>
                <span class="menu-glyph" aria-hidden="true">⤓</span>
                <span class="menu-body">
                  <span class="menu-title">Export as CSV…</span>
                  <span class="menu-sub">
                    {windowChoice === "all"
                      ? "Whole log · spreadsheet-friendly"
                      : `Last ${windowChoice} · spreadsheet-friendly`}
                  </span>
                </span>
              </button>
              <button type="button" role="menuitem" onclick={() => void runExport("json")}>
                <span class="menu-glyph" aria-hidden="true">⤓</span>
                <span class="menu-body">
                  <span class="menu-title">Export as JSON…</span>
                  <span class="menu-sub">
                    {windowChoice === "all"
                      ? "Whole log · with envelope metadata"
                      : `Last ${windowChoice} · with envelope metadata`}
                  </span>
                </span>
              </button>
            </div>
          {/if}
        </div>
        <button
          type="button"
          class="ghost"
          onclick={() => (confirmingPrune = true)}
          disabled={summary.total_events === 0 || pruning || exporting}
        >
          Clear older than 90d…
        </button>
        <button
          type="button"
          class="primary"
          onclick={onClose}
          disabled={pruning || exporting}>Close</button
        >
      {:else}
        <span class="confirm-msg">Delete log entries older than 90 days?</span>
        <button
          type="button"
          class="ghost"
          onclick={() => (confirmingPrune = false)}
          disabled={pruning}>Cancel</button
        >
        <button
          type="button"
          class="primary danger"
          onclick={() => void runPrune(90)}
          disabled={pruning}>{pruning ? "Pruning…" : "Delete"}</button
        >
      {/if}
    </footer>
  </aside>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 110;
    background: rgba(0, 0, 0, 0.42);
    backdrop-filter: blur(2px);
    display: flex;
    justify-content: flex-end;
    animation: backdrop-in 0.16s ease-out;
  }
  @keyframes backdrop-in {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }
  .drawer {
    background: var(--bg-1);
    border-left: 1px solid var(--border);
    box-shadow: -8px 0 30px rgba(0, 0, 0, 0.32);
    width: min(460px, 100%);
    height: 100%;
    display: flex;
    flex-direction: column;
    padding: 22px 24px 18px;
    gap: 12px;
    overflow-y: auto;
    animation: drawer-in 0.18s ease-out;
  }
  @keyframes drawer-in {
    from {
      transform: translateX(20px);
      opacity: 0;
    }
    to {
      transform: translateX(0);
      opacity: 1;
    }
  }
  header {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 10px;
    align-items: start;
  }
  .title-block h1 {
    margin: 0;
    font-size: 17px;
    font-weight: 600;
    line-height: 1.3;
    color: var(--text);
  }
  .title-block .sub {
    margin: 4px 0 0;
    color: var(--text-3);
    font-size: 11.5px;
    line-height: 1.4;
  }
  .icon-btn {
    background: transparent;
    border: 1px solid transparent;
    color: var(--text-3);
    width: 28px;
    height: 28px;
    border-radius: 6px;
    cursor: pointer;
    font: inherit;
    font-size: 13px;
    line-height: 1;
  }
  .icon-btn:hover:not(:disabled) {
    background: var(--bg-2);
    border-color: var(--border);
    color: var(--text);
  }
  .window-strip {
    display: inline-flex;
    border: 1px solid var(--border);
    border-radius: 6px;
    overflow: hidden;
    align-self: flex-start;
  }
  .window-strip button {
    background: var(--bg-1);
    color: var(--text-3);
    border: none;
    border-right: 1px solid var(--border);
    padding: 4px 10px;
    font: inherit;
    font-size: 11.5px;
    cursor: pointer;
  }
  .window-strip button:last-child {
    border-right: none;
  }
  .window-strip button:hover:not(.active) {
    background: var(--bg-2);
    color: var(--text);
  }
  .window-strip button.active {
    background: var(--bg-3);
    color: var(--text);
  }
  .event-list {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    border: 1px solid var(--border);
    border-radius: 6px;
    overflow: hidden;
  }
  .event-row {
    display: grid;
    grid-template-columns: 18px minmax(0, 1fr) auto;
    align-items: start;
    gap: 8px;
    padding: 8px 10px;
    font-size: 12px;
    border-top: 1px solid var(--border);
  }
  .event-row:first-child {
    border-top: none;
  }
  .ev-glyph {
    color: var(--text-3);
    font-size: 12px;
    line-height: 1.4;
    text-align: center;
  }
  .event-row[data-action="failed"] .ev-glyph {
    color: var(--danger, rgb(255, 100, 100));
  }
  .event-row[data-action="update"] .ev-glyph {
    color: rgb(245, 180, 70);
  }
  .event-row[data-action="install"] .ev-glyph {
    color: var(--accent);
  }
  .ev-body {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .ev-line {
    display: flex;
    align-items: baseline;
    gap: 6px;
    flex-wrap: wrap;
  }
  .ev-action {
    color: var(--text);
    text-transform: capitalize;
    font-weight: 500;
  }
  .ev-plugin {
    color: var(--text-2);
    font-family: var(--font-mono);
    font-size: 11px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 200px;
  }
  .ev-version {
    color: var(--text-3);
    font-family: var(--font-mono);
    font-size: 11px;
  }
  .ev-from {
    color: var(--text-3);
    font-family: var(--font-mono);
    font-size: 11px;
  }
  .ev-err {
    color: var(--danger, rgb(255, 100, 100));
    font-size: 11px;
    line-height: 1.4;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ev-time {
    color: var(--text-3);
    font-size: 11px;
    line-height: 1.4;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .err,
  .empty,
  .loading {
    margin: 0;
    color: var(--text-3);
    font-size: 12px;
    line-height: 1.5;
    padding: 12px;
    border: 1px dashed var(--border);
    border-radius: 6px;
  }
  .err {
    color: var(--danger, rgb(255, 100, 100));
    border-color: rgba(255, 100, 100, 0.4);
  }
  .drawer-foot {
    margin-top: auto;
    padding-top: 14px;
    border-top: 1px solid var(--border);
    display: flex;
    justify-content: flex-end;
    align-items: center;
    gap: 8px;
  }
  .drawer-foot button {
    font: inherit;
    padding: 7px 14px;
    border-radius: 6px;
    border: 1px solid var(--border);
    cursor: pointer;
  }
  .drawer-foot .ghost {
    background: transparent;
    color: var(--text-2);
  }
  .drawer-foot .ghost:hover:not(:disabled) {
    background: var(--bg-2);
    color: var(--text);
  }
  .drawer-foot .primary {
    background: var(--accent);
    color: var(--accent-fg, white);
    border-color: var(--accent);
  }
  .drawer-foot .primary.danger {
    background: var(--danger, rgb(255, 100, 100));
    border-color: var(--danger, rgb(255, 100, 100));
  }
  .drawer-foot button:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
  .confirm-msg {
    flex: 1;
    color: var(--text);
    font-size: 12px;
  }
  .export-anchor {
    position: relative;
    margin-right: auto;
  }
  .export-menu {
    position: absolute;
    bottom: calc(100% + 6px);
    left: 0;
    min-width: 260px;
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: 8px;
    box-shadow: 0 6px 18px rgba(0, 0, 0, 0.4);
    padding: 4px;
    display: flex;
    flex-direction: column;
    gap: 1px;
    z-index: 10;
  }
  .export-menu button {
    background: transparent;
    color: var(--text);
    border: none;
    border-radius: 6px;
    padding: 8px 10px;
    text-align: left;
    cursor: pointer;
    font: inherit;
    font-size: 12px;
    display: grid;
    grid-template-columns: 18px minmax(0, 1fr);
    gap: 8px;
    align-items: start;
  }
  .export-menu button:hover {
    background: var(--bg-2);
  }
  .menu-glyph {
    color: var(--text-3);
    font-size: 13px;
    line-height: 1.4;
    text-align: center;
  }
  .menu-body {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .menu-title {
    color: var(--text);
    font-weight: 500;
  }
  .menu-sub {
    color: var(--text-3);
    font-size: 11px;
    line-height: 1.4;
  }
  .export-toast {
    color: var(--text-2);
    font-size: 11.5px;
    line-height: 1.3;
    padding: 4px 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg-2);
  }
</style>
