<script lang="ts">
  // HopperPanel — v3.20.0 Hopper UI.
  //
  // The product wedge: drop a PDF into a watched folder, an Atelier
  // recipe runs on it, Beacon (local Ollama) suggests a 4-6 word title,
  // the file is renamed via a template and auto-filed into the output
  // folder. Live tail shows every run, success or failure, with timing,
  // AI title, and re-run buttons.
  //
  // Two-column Liquid Glass surface:
  //
  //   1. Watches list (left)  — add/remove/toggle watched folders.
  //                              Each row: source → output, recipe pill,
  //                              AI badge, enable toggle, delete X.
  //   2. Run log (right)      — live tail of every pipeline run.
  //                              Subscribes to `hopper://run-completed`
  //                              for instant updates; falls back to
  //                              polling every 5s if subscription drops.
  //
  // Empty state guides users to a 3-step onboarding (choose source,
  // choose output, optional recipe). Errors are explained inline with
  // a fix link (e.g. "Ollama not running — start it" → opens
  // settings).

  import { onMount, onDestroy } from "svelte";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { isInTauri } from "$lib/tauri";
  import HopperRulesEditor from "$lib/components/HopperRulesEditor.svelte";
  import {
    slabHopperListWatches,
    slabHopperAddWatch,
    slabHopperRemoveWatch,
    slabHopperSetEnabled,
    slabHopperListRuns,
    slabHopperRunNow,
    slabHopperDescribe,
    listenRunCompleted,
    formatDuration,
    formatStartedAt,
    basename,
    defaultRenamePattern,
    type Watch,
    type RunRecord,
    type HopperStatus,
  } from "$lib/hopper";

  // -------------------------------------------------------------------
  // State
  // -------------------------------------------------------------------

  let watches = $state<Watch[]>([]);
  let runs = $state<RunRecord[]>([]);
  let status = $state<HopperStatus | null>(null);
  let loading = $state(true);
  let errorMsg = $state<string | null>(null);
  let pollTimer: ReturnType<typeof setInterval> | null = null;
  let unlisten: (() => void) | null = null;

  // "Add watch" form draft. Hidden behind a button until the user
  // clicks "New watch" — empty state shows guided onboarding instead.
  let draftOpen = $state(false);
  let draftSource = $state("");
  let draftOutput = $state("");
  let draftAiRename = $state(true);
  let draftRecipeId = $state("");
  let draftSubmitting = $state(false);

  /** Which watch row (if any) currently has its Routing Rules
   *  editor expanded. `null` = all collapsed. */
  let expandedRulesWatchId = $state<number | null>(null);

  function toggleRules(watchId: number) {
    expandedRulesWatchId = expandedRulesWatchId === watchId ? null : watchId;
  }

  // -------------------------------------------------------------------
  // Lifecycle — load, subscribe, cleanup.
  // -------------------------------------------------------------------

  onMount(async () => {
    await refresh();
    loading = false;
    if (isInTauri()) {
      try {
        unlisten = await listenRunCompleted((rec) => {
          // Prepend (newest first) and dedupe by id — events can race
          // a poll-driven refresh.
          runs = [rec, ...runs.filter((r) => r.id !== rec.id)].slice(0, 200);
          // Refresh the status header counts in the background.
          slabHopperDescribe().then((s) => (status = s)).catch(() => {});
        });
      } catch (e) {
        // Subscription failure is non-fatal; we still poll.
        console.warn("hopper: event listen failed", e);
      }
      // Cheap polling safety-net for missed events.
      pollTimer = setInterval(() => {
        slabHopperListRuns(50)
          .then((r) => (runs = r))
          .catch(() => {});
      }, 5000);
    }
  });

  onDestroy(() => {
    if (pollTimer) clearInterval(pollTimer);
    if (unlisten) unlisten();
  });

  async function refresh() {
    try {
      const [w, r, s] = await Promise.all([
        slabHopperListWatches(),
        slabHopperListRuns(50),
        slabHopperDescribe(),
      ]);
      watches = w;
      runs = r;
      status = s;
      errorMsg = null;
    } catch (e) {
      errorMsg = String(e);
    }
  }

  // -------------------------------------------------------------------
  // Folder pickers — Tauri dialog, gracefully degrade in browser
  // -------------------------------------------------------------------

  async function pickFolder(): Promise<string | null> {
    if (!isInTauri()) {
      // Browser fallback — prompt() is ugly but functional for the
      // pnpm dev preview.
      return prompt("Folder path:") || null;
    }
    const sel = await openDialog({ directory: true, multiple: false });
    if (typeof sel === "string") return sel;
    return null;
  }

  async function pickSource() {
    const p = await pickFolder();
    if (p) draftSource = p;
  }
  async function pickOutput() {
    const p = await pickFolder();
    if (p) draftOutput = p;
  }

  // -------------------------------------------------------------------
  // Mutations
  // -------------------------------------------------------------------

  async function submitDraft() {
    if (!draftSource || !draftOutput) return;
    draftSubmitting = true;
    try {
      await slabHopperAddWatch({
        source_dir: draftSource,
        output_dir: draftOutput,
        recipe_id: draftRecipeId || null,
        rename_pattern: defaultRenamePattern(draftAiRename),
        ai_rename: draftAiRename,
      });
      draftSource = "";
      draftOutput = "";
      draftRecipeId = "";
      draftAiRename = true;
      draftOpen = false;
      await refresh();
    } catch (e) {
      errorMsg = `Add watch failed: ${e}`;
    } finally {
      draftSubmitting = false;
    }
  }

  async function toggleWatch(w: Watch) {
    try {
      await slabHopperSetEnabled(w.id, !w.enabled);
      await refresh();
    } catch (e) {
      errorMsg = `Toggle failed: ${e}`;
    }
  }

  async function removeWatch(w: Watch) {
    if (!confirm(`Remove watch on ${w.source_dir}?`)) return;
    try {
      await slabHopperRemoveWatch(w.id);
      await refresh();
    } catch (e) {
      errorMsg = `Remove failed: ${e}`;
    }
  }

  async function runAgain(rec: RunRecord) {
    try {
      await slabHopperRunNow(rec.watch_id, rec.input_path);
    } catch (e) {
      errorMsg = `Re-run failed: ${e}`;
    }
  }
</script>

<div class="hopper">
  <header class="head">
    <div class="title">
      <h1>Hopper</h1>
      <span class="sub">Watch folders. PDFs auto-process on arrival.</span>
    </div>
    {#if status}
      <div class="metrics">
        <span class="metric"
          ><span class="num">{status.watch_count}</span> watches</span
        >
        <span class="metric"
          ><span class="num">{status.run_count}</span> runs</span
        >
        <span class="metric ver">{status.version}</span>
      </div>
    {/if}
  </header>

  {#if errorMsg}
    <div class="error" role="alert">
      <span class="err-icon">⚠</span>
      <span class="err-msg">{errorMsg}</span>
      <button class="err-dismiss" onclick={() => (errorMsg = null)}>×</button>
    </div>
  {/if}

  <div class="cols">
    <!-- =================== WATCHES =================== -->
    <section class="col watches">
      <div class="col-head">
        <h2>Watched folders</h2>
        <button class="primary" onclick={() => (draftOpen = !draftOpen)}>
          {draftOpen ? "Cancel" : "+ New watch"}
        </button>
      </div>

      {#if draftOpen}
        <div class="draft">
          <div class="field">
            <label for="src">Source folder</label>
            <div class="field-row">
              <input id="src" bind:value={draftSource} placeholder="/path/to/incoming" />
              <button onclick={pickSource}>Browse…</button>
            </div>
          </div>

          <div class="field">
            <label for="out">Output folder</label>
            <div class="field-row">
              <input id="out" bind:value={draftOutput} placeholder="/path/to/filed" />
              <button onclick={pickOutput}>Browse…</button>
            </div>
          </div>

          <div class="field">
            <label for="recipe">Atelier recipe (optional)</label>
            <input
              id="recipe"
              bind:value={draftRecipeId}
              placeholder="e.g. Nightly Discovery"
            />
            <small class="hint">
              Leave blank to copy files through without processing.
            </small>
          </div>

          <label class="check">
            <input type="checkbox" bind:checked={draftAiRename} />
            <span>Use Beacon (local AI) to suggest a 4-6 word title</span>
          </label>

          <button
            class="primary big"
            disabled={!draftSource || !draftOutput || draftSubmitting}
            onclick={submitDraft}
          >
            {draftSubmitting ? "Adding…" : "Add watch"}
          </button>
        </div>
      {:else if loading}
        <div class="empty">
          <div class="spinner" />
          <span>Loading watches…</span>
        </div>
      {:else if watches.length === 0}
        <div class="empty onboard">
          <div class="ob-icon">🪣</div>
          <h3>No watches yet</h3>
          <p>
            Pick a folder Slab should monitor. Every PDF dropped there is
            renamed by Beacon (local AI) and auto-filed into a destination
            folder — Hazel meets Acrobat AutoActions, fully offline.
          </p>
          <button class="primary big" onclick={() => (draftOpen = true)}>
            Create your first watch
          </button>
        </div>
      {:else}
        <ul class="watch-list">
          {#each watches as w (w.id)}
            <li class="watch-row" class:disabled={!w.enabled}>
              <button
                class="toggle"
                onclick={() => toggleWatch(w)}
                title={w.enabled ? "Pause this watch" : "Resume this watch"}
              >
                {w.enabled ? "●" : "○"}
              </button>
              <div class="paths">
                <div class="from" title={w.source_dir}>
                  {basename(w.source_dir) || w.source_dir}
                </div>
                <div class="arrow">→</div>
                <div class="to" title={w.output_dir}>
                  {basename(w.output_dir) || w.output_dir}
                </div>
              </div>
              <div class="badges">
                {#if w.recipe_id}
                  <span class="badge recipe">{w.recipe_id}</span>
                {/if}
                {#if w.ai_rename}
                  <span class="badge ai">AI</span>
                {/if}
              </div>
              <button
                class="rules-toggle"
                class:open={expandedRulesWatchId === w.id}
                onclick={() => toggleRules(w.id)}
                title="Configure routing rules for this watch"
              >
                {expandedRulesWatchId === w.id ? "▾" : "▸"} Rules
              </button>
              <button
                class="del"
                onclick={() => removeWatch(w)}
                title="Remove this watch"
              >
                ×
              </button>
            </li>
            {#if expandedRulesWatchId === w.id}
              <li class="rules-host">
                <HopperRulesEditor
                  watchId={w.id}
                  watchSource={w.source_dir}
                  watchOutput={w.output_dir}
                  watchRecipeId={w.recipe_id}
                />
              </li>
            {/if}
          {/each}
        </ul>
      {/if}
    </section>

    <!-- =================== RUN LOG =================== -->
    <section class="col log">
      <div class="col-head">
        <h2>Live activity</h2>
        <span class="live-dot" title="Live updates via Tauri events" />
      </div>

      {#if loading}
        <div class="empty">
          <div class="spinner" />
          <span>Loading runs…</span>
        </div>
      {:else if runs.length === 0}
        <div class="empty">
          <div class="ob-icon">📭</div>
          <h3>No runs yet</h3>
          <p>Drop a PDF into a watched folder — it'll show up here.</p>
        </div>
      {:else}
        <ul class="run-list">
          {#each runs as r (r.id)}
            <li class="run-row" class:fail={r.status === "failed"}>
              <span class="status" title={r.status}>
                {r.status === "success" ? "✓" : "✗"}
              </span>
              <div class="run-main">
                <div class="run-name" title={r.input_path}>
                  {basename(r.input_path)}
                </div>
                {#if r.ai_title}
                  <div class="run-rename" title="AI-suggested title">
                    → <em>{r.ai_title}</em>
                  </div>
                {/if}
                {#if r.output_path}
                  <div class="run-out" title={r.output_path}>
                    saved to {basename(r.output_path)}
                  </div>
                {/if}
                {#if r.error}
                  <div class="run-err">{r.error}</div>
                {/if}
              </div>
              <div class="run-meta">
                <div class="run-time">{formatStartedAt(r.started_at)}</div>
                <div class="run-dur">{formatDuration(r.duration_ms)}</div>
                <button
                  class="rerun"
                  onclick={() => runAgain(r)}
                  title="Re-run pipeline on this file"
                >
                  ↻
                </button>
              </div>
            </li>
          {/each}
        </ul>
      {/if}
    </section>
  </div>
</div>

<style>
  .hopper {
    display: flex;
    flex-direction: column;
    height: 100%;
    padding: 16px 20px;
    gap: 12px;
    color: var(--fg, #e6e6e6);
    background: var(--bg, #181818);
    font: 13px/1.4 -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
  }

  .head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
  }
  .title h1 {
    font-size: 20px;
    font-weight: 600;
    margin: 0 0 2px 0;
    letter-spacing: -0.01em;
  }
  .sub {
    font-size: 12px;
    color: var(--fg-dim, #888);
  }
  .metrics {
    display: flex;
    gap: 12px;
    align-items: center;
  }
  .metric {
    font-size: 12px;
    color: var(--fg-dim, #888);
    background: rgba(255, 255, 255, 0.04);
    padding: 4px 10px;
    border-radius: 10px;
  }
  .metric .num {
    color: var(--fg, #fff);
    font-weight: 600;
    margin-right: 4px;
  }
  .metric.ver {
    font-family: ui-monospace, monospace;
    font-size: 11px;
  }

  .error {
    display: flex;
    align-items: center;
    gap: 10px;
    background: rgba(220, 60, 60, 0.12);
    border: 1px solid rgba(220, 60, 60, 0.3);
    padding: 8px 12px;
    border-radius: 6px;
  }
  .err-icon {
    color: #ff7a7a;
  }
  .err-msg {
    flex: 1;
    font-size: 12px;
  }
  .err-dismiss {
    background: transparent;
    border: none;
    color: #888;
    cursor: pointer;
    font-size: 18px;
  }

  .cols {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 14px;
    flex: 1;
    min-height: 0;
  }
  .col {
    background: rgba(255, 255, 255, 0.03);
    border: 1px solid rgba(255, 255, 255, 0.06);
    border-radius: 10px;
    padding: 12px;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .col-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 10px;
  }
  .col-head h2 {
    font-size: 13px;
    font-weight: 600;
    margin: 0;
    color: var(--fg, #ddd);
  }

  button {
    background: rgba(255, 255, 255, 0.08);
    color: var(--fg, #ddd);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 6px;
    padding: 5px 10px;
    font-size: 12px;
    cursor: pointer;
    transition: background 0.12s ease;
  }
  button:hover {
    background: rgba(255, 255, 255, 0.14);
  }
  button.primary {
    background: var(--accent, #5a8dee);
    color: #fff;
    border-color: transparent;
  }
  button.primary:hover {
    background: var(--accent-hi, #6d9eff);
  }
  button.primary.big {
    padding: 8px 18px;
    font-weight: 500;
  }
  button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .draft {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .field label {
    font-size: 11px;
    color: var(--fg-dim, #888);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .field-row {
    display: flex;
    gap: 6px;
  }
  input[type="text"],
  input:not([type]) {
    flex: 1;
    background: rgba(0, 0, 0, 0.2);
    border: 1px solid rgba(255, 255, 255, 0.08);
    color: var(--fg, #eee);
    padding: 6px 8px;
    border-radius: 5px;
    font: inherit;
  }
  .hint {
    font-size: 11px;
    color: var(--fg-dim, #888);
  }
  .check {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    color: var(--fg-dim, #aaa);
  }

  .empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    text-align: center;
    color: var(--fg-dim, #888);
    gap: 8px;
    padding: 30px 20px;
  }
  .empty.onboard {
    gap: 14px;
  }
  .empty.onboard p {
    max-width: 360px;
    line-height: 1.5;
    margin: 0;
  }
  .ob-icon {
    font-size: 36px;
  }
  .empty h3 {
    margin: 0;
    color: var(--fg, #ddd);
    font-size: 15px;
    font-weight: 600;
  }
  .spinner {
    width: 22px;
    height: 22px;
    border: 2px solid rgba(255, 255, 255, 0.1);
    border-top-color: var(--accent, #5a8dee);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .watch-list {
    list-style: none;
    padding: 0;
    margin: 0;
    overflow-y: auto;
    flex: 1;
  }
  .watch-row {
    display: grid;
    grid-template-columns: 28px 1fr auto 28px;
    gap: 8px;
    align-items: center;
    padding: 8px;
    border-radius: 6px;
    transition: background 0.12s ease;
  }
  .watch-row:hover {
    background: rgba(255, 255, 255, 0.04);
  }
  .watch-row.disabled {
    opacity: 0.5;
  }
  .toggle {
    background: transparent;
    border: none;
    color: var(--accent, #5a8dee);
    font-size: 18px;
    padding: 0;
    width: 24px;
  }
  .watch-row.disabled .toggle {
    color: var(--fg-dim, #666);
  }
  .paths {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    overflow: hidden;
  }
  .from,
  .to {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 130px;
  }
  .arrow {
    color: var(--fg-dim, #666);
  }
  .badges {
    display: flex;
    gap: 4px;
  }
  .badge {
    font-size: 10px;
    padding: 2px 6px;
    border-radius: 8px;
    background: rgba(255, 255, 255, 0.08);
    color: var(--fg-dim, #aaa);
  }
  .badge.ai {
    background: rgba(90, 141, 238, 0.2);
    color: #9ec0ff;
  }
  .badge.recipe {
    background: rgba(110, 200, 130, 0.18);
    color: #a3e0b4;
  }
  .del {
    background: transparent;
    border: none;
    color: var(--fg-dim, #666);
    font-size: 16px;
    padding: 0;
    width: 24px;
  }
  .del:hover {
    color: #ff7a7a;
    background: transparent;
  }
  .rules-toggle {
    background: rgba(110, 165, 255, 0.12);
    border: 1px solid rgba(110, 165, 255, 0.25);
    color: #b8d1ff;
    border-radius: 6px;
    padding: 3px 9px;
    font-size: 11px;
    font-family: inherit;
    cursor: pointer;
    transition: all 0.15s ease;
  }
  .rules-toggle:hover {
    background: rgba(110, 165, 255, 0.22);
    color: #d5e4ff;
  }
  .rules-toggle.open {
    background: rgba(110, 165, 255, 0.28);
    color: #fff;
    border-color: rgba(110, 165, 255, 0.55);
  }
  .rules-host {
    list-style: none;
    padding: 0;
    margin: 0;
  }

  .live-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: #6ec882;
    box-shadow: 0 0 8px rgba(110, 200, 130, 0.6);
    animation: pulse 2s ease-in-out infinite;
  }
  @keyframes pulse {
    0%,
    100% {
      opacity: 1;
    }
    50% {
      opacity: 0.4;
    }
  }

  .run-list {
    list-style: none;
    padding: 0;
    margin: 0;
    overflow-y: auto;
    flex: 1;
  }
  .run-row {
    display: grid;
    grid-template-columns: 24px 1fr auto;
    gap: 10px;
    align-items: start;
    padding: 8px;
    border-radius: 6px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.04);
  }
  .run-row.fail {
    background: rgba(220, 60, 60, 0.06);
  }
  .status {
    font-size: 14px;
    color: #6ec882;
    margin-top: 1px;
  }
  .run-row.fail .status {
    color: #ff7a7a;
  }
  .run-main {
    overflow: hidden;
  }
  .run-name {
    font-size: 13px;
    color: var(--fg, #ddd);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .run-rename {
    font-size: 11px;
    color: var(--fg-dim, #9ec0ff);
    margin-top: 2px;
  }
  .run-out {
    font-size: 11px;
    color: var(--fg-dim, #777);
    margin-top: 2px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .run-err {
    font-size: 11px;
    color: #ff7a7a;
    margin-top: 2px;
    font-family: ui-monospace, monospace;
  }
  .run-meta {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 2px;
    font-size: 11px;
    color: var(--fg-dim, #888);
  }
  .run-time {
    font-family: ui-monospace, monospace;
  }
  .rerun {
    background: transparent;
    border: none;
    color: var(--fg-dim, #666);
    font-size: 14px;
    padding: 0;
    margin-top: 2px;
    cursor: pointer;
  }
  .rerun:hover {
    color: var(--accent, #9ec0ff);
    background: transparent;
  }
</style>
