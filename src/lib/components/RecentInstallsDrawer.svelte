<script lang="ts">
  // RecentInstallsDrawer — v3.39 Slice 57.
  //
  // 460px right-side slide-in drawer surfacing the corpus-wide
  // marketplace install log: every install / update / uninstall /
  // failure across all plugins, newest first. Triggered from the
  // PluginsPanel toolbar "History" button which is gated on the
  // log having at least one event (no nag UI on a brand-new install
  // with empty history).
  //
  // The drawer is purely presentational — the parent owns the open
  // state and toast wiring. Its only external dependencies are the
  // four marketplace helpers (listRecentInstallEvents,
  // installLogSummary, pruneInstallLog, formatInstallEventTime).
  //
  // Layout matches PluginDetailDrawer's Notion-side-panel convention
  // so the two drawers feel like siblings: backdrop + slide-from-right,
  // header with title + close button, content scrollable, footer with
  // close-only action (every other action is row-local).

  import { onMount } from "svelte";
  import {
    formatInstallEventTime,
    formatLogSpan,
    installEventGlyph,
    installLogSummary,
    listRecentInstallEvents,
    pruneInstallLog,
    type InstallEvent,
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

  let filteredEvents = $derived.by<InstallEvent[]>(() => {
    if (windowChoice === "all") return events;
    const nowSec = Math.floor(Date.now() / 1000);
    const cutoff = nowSec - (windowChoice === "7d" ? 7 : 30) * 86_400;
    return events.filter((ev) => ev.occurred_at >= cutoff);
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
      if (confirmingPrune) {
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
</script>

<svelte:window onkeydown={onKeydown} />

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
      {#if !confirmingPrune}
        <button
          type="button"
          class="ghost"
          onclick={() => (confirmingPrune = true)}
          disabled={summary.total_events === 0 || pruning}
        >
          Clear older than 90d…
        </button>
        <button type="button" class="primary" onclick={onClose} disabled={pruning}>Close</button>
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
</style>
