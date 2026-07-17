<script lang="ts">
  // BulkUpdateProgressOverlay — v3.39 round-15 slice 72.
  //
  // Modal-style overlay shown while a bulk plugin update batch is in
  // flight. Reads per-step progress from the `marketplace://update-
  // progress` event stream (subscribed by the parent PluginsPanel)
  // and renders a live "Updating 2/5 · Acme PDF Tools…" headline plus
  // a per-target row list showing pending / updating / done / failed
  // state. Stays mounted on completion until the user dismisses it
  // so they can read the per-row outcomes after the batch lands.
  //
  // Phase model per target (driven by the parent's reducer over
  // UpdateProgress events + the final BatchUpdateReport):
  //   pending  — queued, no event seen yet
  //   updating — `starting` event arrived for this id
  //   done     — `done` event arrived
  //   failed   — `error` event arrived (carries the error message)
  //
  // Parent dismisses by setting the bound `open` prop to false; the
  // overlay also dismisses itself on Esc once the batch is in a
  // terminal state (every row is done or failed, OR a final report
  // has been emitted by the parent).

  import { formatBytes } from "$lib/marketplace";
  import type { UpdateTarget } from "$lib/marketplace";

  type RowPhase = "pending" | "updating" | "done" | "failed";

  export type BulkUpdateRowState = {
    target: UpdateTarget;
    phase: RowPhase;
    /** Populated when phase === "failed". */
    error: string | null;
  };

  type Props = {
    /** Ordered row state, one per target in the batch. The parent
     *  reducer mutates this in place based on the event stream. */
    rows: BulkUpdateRowState[];
    /** Index of the current target being updated (1-indexed). 0 means
     *  the batch hasn't started any row yet (pre-first-event window).
     *  Equal to rows.length once the batch lands. */
    currentIndex: number;
    /** True once every row is in a terminal state (done or failed). */
    finished: boolean;
    /** Summary line for the finished state, e.g. "Updated 3 plugins
     *  (4.2 MB)". Empty string while the batch is in flight. */
    summary: string;
    /** Caller closes the overlay. We refuse to close while !finished
     *  so the user can't strand a half-running batch off-screen. */
    onDismiss: () => void;
  };

  let { rows, currentIndex, finished, summary, onDismiss }: Props = $props();

  let succeeded = $derived(rows.filter((r) => r.phase === "done").length);
  let failed = $derived(rows.filter((r) => r.phase === "failed").length);

  function rowIcon(phase: RowPhase): string {
    switch (phase) {
      case "done":
        return "✓";
      case "failed":
        return "✕";
      case "updating":
        return "…";
      case "pending":
      default:
        return "○";
    }
  }

  function rowLabel(phase: RowPhase): string {
    switch (phase) {
      case "done":
        return "Updated";
      case "failed":
        return "Failed";
      case "updating":
        return "Updating…";
      case "pending":
      default:
        return "Queued";
    }
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape" && finished) {
      e.preventDefault();
      onDismiss();
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div
  class="backdrop"
  role="dialog"
  aria-modal="true"
  aria-labelledby="bulk-update-title"
  aria-live="polite"
  tabindex="-1"
>
  <div class="modal" role="document">
    <header>
      <div
        class="icon"
        class:icon-done={finished && failed === 0}
        class:icon-mixed={finished && failed > 0 && succeeded > 0}
        class:icon-err={finished && succeeded === 0 && failed > 0}
        aria-hidden="true"
      >
        {#if finished && failed === 0}
          ✓
        {:else if finished && succeeded === 0}
          ✕
        {:else if finished}
          !
        {:else}
          ↑
        {/if}
      </div>
      <div class="title-block">
        <h1 id="bulk-update-title">
          {#if finished}
            {summary || "Bulk update complete"}
          {:else}
            Updating plugins
          {/if}
        </h1>
        <p class="sub">
          {#if finished}
            {succeeded} succeeded · {failed} failed
          {:else if currentIndex === 0}
            Starting batch · {rows.length} plugins
          {:else}
            {Math.min(currentIndex, rows.length)}/{rows.length}
            {#if currentIndex >= 1 && currentIndex <= rows.length}
              · {rows[currentIndex - 1]?.target.entry.name ?? ""}
            {/if}
          {/if}
        </p>
      </div>
    </header>

    <!-- Top-level progress bar — fills as rows land in terminal state. -->
    <div class="progress" aria-hidden="true">
      <div
        class="progress-fill"
        class:progress-fill-finished={finished}
        style="width: {rows.length === 0 ? 0 : ((succeeded + failed) / rows.length) * 100}%"
      ></div>
    </div>

    <!-- Per-row list. -->
    <ul class="rows">
      {#each rows as row (row.target.id)}
        <li
          class="row"
          class:row-current={!finished && row.phase === "updating"}
          class:row-done={row.phase === "done"}
          class:row-failed={row.phase === "failed"}
        >
          <span class="row-icon" aria-hidden="true">{rowIcon(row.phase)}</span>
          <div class="row-meta">
            <div class="row-top">
              <span class="row-name">{row.target.entry.name}</span>
              <span class="row-versions">
                v{row.target.installed_version}
                →
                v{row.target.available_version}
              </span>
              <span class="row-size">{formatBytes(row.target.size_bytes)}</span>
            </div>
            <div class="row-bottom">
              <span class="row-label">{rowLabel(row.phase)}</span>
              {#if row.phase === "failed" && row.error}
                <span class="row-error">— {row.error}</span>
              {/if}
            </div>
          </div>
        </li>
      {/each}
    </ul>

    <footer>
      <button
        type="button"
        class="dismiss-btn"
        onclick={onDismiss}
        disabled={!finished}
        title={finished ? "Close" : "Waiting for batch to finish"}
      >
        {finished ? "Close" : "Updating…"}
      </button>
    </footer>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
    z-index: 100;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 40px 24px;
  }
  .modal {
    width: min(560px, 100%);
    max-height: calc(100vh - 80px);
    display: flex;
    flex-direction: column;
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.5);
    overflow: hidden;
  }
  header {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 16px 18px 12px;
    border-bottom: 1px solid var(--border);
  }
  .icon {
    width: 36px;
    height: 36px;
    border-radius: 50%;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    background: var(--bg-2);
    border: 1px solid var(--border);
    color: var(--accent);
    font-size: 16px;
    font-weight: 600;
    flex-shrink: 0;
  }
  .icon-done {
    color: #3fc88c;
    border-color: #3fc88c;
  }
  .icon-mixed {
    color: #e0b450;
    border-color: #e0b450;
  }
  .icon-err {
    color: #ff6b6b;
    border-color: #ff6b6b;
  }
  .title-block {
    flex: 1;
    min-width: 0;
  }
  .title-block h1 {
    margin: 0 0 2px;
    font-size: 16px;
    font-weight: 600;
    color: var(--text-1);
    line-height: 1.3;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .sub {
    margin: 0;
    font-size: 12px;
    color: var(--text-3);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .progress {
    height: 4px;
    background: var(--bg-2);
    overflow: hidden;
  }
  .progress-fill {
    height: 100%;
    background: var(--accent);
    transition: width 200ms ease-out;
  }
  .progress-fill-finished {
    background: #3fc88c;
  }
  .rows {
    list-style: none;
    margin: 0;
    padding: 8px 0;
    overflow-y: auto;
    flex: 1;
  }
  .row {
    display: grid;
    grid-template-columns: 20px 1fr;
    align-items: start;
    gap: 10px;
    padding: 8px 18px;
    border-bottom: 1px solid var(--border);
  }
  .row:last-child {
    border-bottom: none;
  }
  .row-current {
    background: color-mix(in srgb, var(--accent) 6%, var(--bg-1));
  }
  .row-done {
    color: var(--text-2);
  }
  .row-failed {
    background: color-mix(in srgb, #ff6b6b 6%, var(--bg-1));
  }
  .row-icon {
    color: var(--text-3);
    font-size: 13px;
    line-height: 1.4;
    padding-top: 1px;
  }
  .row-done .row-icon {
    color: #3fc88c;
  }
  .row-failed .row-icon {
    color: #ff6b6b;
  }
  .row-current .row-icon {
    color: var(--accent);
  }
  .row-meta {
    min-width: 0;
  }
  .row-top {
    display: grid;
    grid-template-columns: minmax(120px, 1fr) auto auto;
    align-items: baseline;
    gap: 10px;
    margin-bottom: 2px;
  }
  .row-name {
    font-size: 13px;
    font-weight: 500;
    color: var(--text-1);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .row-done .row-name {
    color: var(--text-2);
  }
  .row-versions {
    font-size: 11.5px;
    color: var(--text-3);
    font-family: var(--font-mono);
    white-space: nowrap;
  }
  .row-size {
    font-size: 11px;
    color: var(--text-3);
    font-family: var(--font-mono);
    white-space: nowrap;
  }
  .row-bottom {
    font-size: 11.5px;
    color: var(--text-3);
    display: flex;
    align-items: baseline;
    gap: 6px;
    min-width: 0;
  }
  .row-label {
    font-weight: 500;
  }
  .row-done .row-label {
    color: #3fc88c;
  }
  .row-failed .row-label {
    color: #ff6b6b;
  }
  .row-current .row-label {
    color: var(--accent);
  }
  .row-error {
    color: #ff6b6b;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  footer {
    border-top: 1px solid var(--border);
    padding: 12px 18px;
    display: flex;
    justify-content: flex-end;
  }
  .dismiss-btn {
    padding: 6px 16px;
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    background: var(--bg-2);
    color: var(--text-1);
    font-size: 12px;
    font-weight: 500;
    font-family: inherit;
    line-height: 1;
    cursor: pointer;
  }
  .dismiss-btn:hover:not(:disabled) {
    background: var(--bg-3);
    border-color: var(--accent);
  }
  .dismiss-btn:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }
</style>
