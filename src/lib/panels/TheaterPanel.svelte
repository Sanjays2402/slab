<!--
  TheaterPanel.svelte (v2.3.0 — Theater 🎬 slice 4 surface)

  Operator-facing presenter control panel. Live previews the audience
  window, exposes every backend toggle, surfaces the keyboard cheat-sheet,
  and shows a session timer. Designed to live inside the existing sidebar
  panel slot — the audience fullscreen window is opened in a follow-up
  slice via windows::registry.

  This single panel proves the entire v2.3.0 backend end-to-end: start a
  session, navigate, toggle overlays, push/undo ink strokes, end. Every
  mutation roundtrips through Tauri and updates `state` for the operator
  to see, so the slice already passes the Buy-Button "notice-it" test.
-->
<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import {
    theaterStart,
    theaterEnd,
    theaterSnapshot,
    theaterToggleBlackout,
    theaterToggleWhiteout,
    theaterToggleLaser,
    theaterToggleInk,
    theaterToggleSpotlight,
    theaterPushStroke,
    theaterUndoStroke,
    theaterClearStrokes,
    theaterOpenWindows,
    theaterCloseWindows,
    dispatchPresenterKey,
    makeStroke,
    pushPoint,
    type TheaterState,
    type InkStroke,
  } from "../theater";

  // ---- Props ----
  // The currently open document. When null, the panel renders an empty
  // state telling the operator to open a PDF first.
  export let documentPath: string | null = null;
  export let totalPages: number = 0;

  // ---- Local state ----
  let state: TheaterState | null = null;
  let errorMsg: string | null = null;
  let busy = false;
  let elapsedLabel = "00:00";
  let timerHandle: number | null = null;
  // Detached-window labels — set after `theaterOpenWindows` resolves.
  // null means we haven't spawned the dual-window mode yet (or it was
  // explicitly closed). Tracked so the panel UI can show a "Detach
  // windows" vs "Close windows" affordance without polling the backend.
  let windowLabels: { audience: string; control: string } | null = null;

  // ---- Lifecycle ----
  onMount(async () => {
    try {
      state = await theaterSnapshot();
      startTimerIfRunning();
    } catch (e) {
      errorMsg = e instanceof Error ? e.message : String(e);
    }
    window.addEventListener("keydown", onKeydown);
  });

  onDestroy(() => {
    window.removeEventListener("keydown", onKeydown);
    if (timerHandle != null) {
      window.clearInterval(timerHandle);
      timerHandle = null;
    }
  });

  // ---- Operations ----
  async function startSession() {
    if (!documentPath || totalPages < 1) return;
    busy = true;
    errorMsg = null;
    try {
      state = await theaterStart(documentPath, totalPages);
      startTimerIfRunning();
      // Auto-detach: starting a session implies the operator wants the
      // dual-window experience. If they don't, they can close the
      // audience window with the system close-button — backend stays
      // session-active until they hit Exit.
      await openDetachedWindows();
    } catch (e) {
      errorMsg = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }

  async function openDetachedWindows() {
    try {
      windowLabels = await theaterOpenWindows(documentPath);
    } catch (e) {
      // Non-fatal: in-panel mode still works. Surface so the operator
      // knows why they didn't get a second display.
      errorMsg = e instanceof Error ? e.message : String(e);
    }
  }

  async function closeDetachedWindows() {
    try {
      await theaterCloseWindows();
    } catch (e) {
      console.warn("[theater] closeDetachedWindows failed", e);
    } finally {
      windowLabels = null;
    }
  }

  async function endSession() {
    busy = true;
    try {
      const final = await theaterEnd();
      state = null;
      if (timerHandle != null) {
        window.clearInterval(timerHandle);
        timerHandle = null;
      }
      elapsedLabel = "00:00";
      if (final) {
        console.info(
          `[theater] session ended at page ${final.current_page}/${final.total_pages} with ${final.ink_strokes.length} ink strokes`,
        );
      }
      // Tear down detached windows in lockstep with the session.
      if (windowLabels) {
        await closeDetachedWindows();
      }
    } catch (e) {
      errorMsg = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }

  async function wrap<T>(fn: () => Promise<T>) {
    if (busy) return;
    busy = true;
    errorMsg = null;
    try {
      const next = await fn();
      if (next && typeof next === "object" && "current_page" in next) {
        state = next as unknown as TheaterState;
      }
    } catch (e) {
      errorMsg = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }

  // ---- Keyboard ----
  async function onKeydown(ev: KeyboardEvent) {
    if (!state) return;
    if (ev.key === "Escape") {
      ev.preventDefault();
      await endSession();
      return;
    }
    const next = await dispatchPresenterKey(ev, state.total_pages);
    if (next) state = next;
  }

  // ---- Timer ----
  function startTimerIfRunning() {
    if (!state) return;
    if (timerHandle != null) window.clearInterval(timerHandle);
    timerHandle = window.setInterval(() => {
      if (!state) return;
      const seconds = Math.max(0, Math.floor((Date.now() - state.started_at_ms) / 1000));
      const mm = String(Math.floor(seconds / 60)).padStart(2, "0");
      const ss = String(seconds % 60).padStart(2, "0");
      elapsedLabel = `${mm}:${ss}`;
    }, 500);
  }

  // ---- Ink demo (mini test pad) ----
  // The audience window's full ink overlay ships in a follow-up slice;
  // this mini pad is here so QA can exercise push_stroke / undo / clear
  // end-to-end without a second window.
  let demoPad: HTMLDivElement | null = null;
  let activeStroke: InkStroke | null = null;

  function padPointerDown(ev: PointerEvent) {
    if (!state || !demoPad || !state.ink_mode) return;
    const rect = demoPad.getBoundingClientRect();
    activeStroke = makeStroke(state.current_page, "#ff3b30", 2.5);
    pushPoint(
      activeStroke,
      (ev.clientX - rect.left) / rect.width,
      (ev.clientY - rect.top) / rect.height,
    );
    demoPad.setPointerCapture(ev.pointerId);
  }

  function padPointerMove(ev: PointerEvent) {
    if (!activeStroke || !demoPad) return;
    const rect = demoPad.getBoundingClientRect();
    pushPoint(
      activeStroke,
      (ev.clientX - rect.left) / rect.width,
      (ev.clientY - rect.top) / rect.height,
    );
  }

  async function padPointerUp() {
    if (!activeStroke) return;
    const stroke = activeStroke;
    activeStroke = null;
    if (stroke.points.length >= 2) {
      await wrap(() => theaterPushStroke(stroke));
    }
  }

  $: pageLabel = state
    ? `Page ${state.current_page} of ${state.total_pages}`
    : "—";
  $: strokesOnPage = state
    ? state.ink_strokes.filter((s) => state && s.page === state.current_page).length
    : 0;
</script>

<section class="theater-panel" aria-label="Theater presenter controls">
  <header class="head">
    <h2>
      <span class="icon" aria-hidden="true">🎬</span>
      Theater
      <span class="badge">v2.3.0</span>
    </h2>
    {#if state}
      <span class="elapsed" aria-live="polite">{elapsedLabel}</span>
    {/if}
  </header>

  {#if !state}
    <!-- Empty state — designed, not blank -->
    <div class="empty-state">
      <p class="muted">
        Turn any open PDF into a slide-style presentation. Two windows,
        one synchronized state — laser, ink, blackout, spotlight.
      </p>
      <button
        class="primary"
        disabled={!documentPath || totalPages < 1 || busy}
        on:click={startSession}
      >
        {busy ? "Starting…" : "Start presentation"}
      </button>
      <p class="hint-soft">
        Opens a fullscreen audience window and a presenter control
        window with notes, timer, and previews.
      </p>
      {#if !documentPath}
        <p class="hint">Open a PDF first to enable Theater.</p>
      {/if}
    </div>
  {:else}
    <!-- Active session -->
    <div class="status-row">
      <span class="page-label">{pageLabel}</span>
      {#if windowLabels}
        <button class="ghost" disabled={busy} on:click={closeDetachedWindows}>
          Close audience window
        </button>
      {:else}
        <button class="ghost" disabled={busy} on:click={openDetachedWindows}>
          Open audience window
        </button>
      {/if}
      <button class="ghost danger" disabled={busy} on:click={endSession}>
        Exit (Esc)
      </button>
    </div>

    <div class="overlays" role="group" aria-label="Overlay toggles">
      <button
        class:active={state.blackout}
        title="Blackout audience (B)"
        on:click={() => wrap(theaterToggleBlackout)}
      >
        <span class="dot dot-black"></span> Blackout
      </button>
      <button
        class:active={state.whiteout}
        title="Whiteboard (W)"
        on:click={() => wrap(theaterToggleWhiteout)}
      >
        <span class="dot dot-white"></span> Whiteout
      </button>
      <button
        class:active={state.laser_on}
        title="Laser pointer (L)"
        on:click={() => wrap(theaterToggleLaser)}
      >
        <span class="dot dot-laser"></span> Laser
      </button>
      <button
        class:active={state.ink_mode}
        title="Ink mode (I)"
        on:click={() => wrap(theaterToggleInk)}
      >
        <span class="dot dot-ink"></span> Ink
      </button>
      <button
        class:active={state.spotlight_on}
        title="Spotlight cursor (.)"
        on:click={() => wrap(theaterToggleSpotlight)}
      >
        <span class="dot dot-spot"></span> Spotlight
      </button>
    </div>

    <div class="ink-controls">
      <span class="muted-small">Strokes on page: {strokesOnPage}</span>
      <div class="ink-buttons">
        <button
          disabled={state.ink_strokes.length === 0 || busy}
          on:click={() => wrap(theaterUndoStroke)}
          title="Undo last stroke (U)"
        >
          Undo
        </button>
        <button
          class="danger"
          disabled={state.ink_strokes.length === 0 || busy}
          on:click={() => wrap(theaterClearStrokes)}
          title="Clear all strokes (C)"
        >
          Clear
        </button>
      </div>
    </div>

    {#if state.ink_mode}
      <div
        bind:this={demoPad}
        class="ink-pad"
        role="application"
        aria-label="Ink capture surface"
        on:pointerdown={padPointerDown}
        on:pointermove={padPointerMove}
        on:pointerup={padPointerUp}
        on:pointercancel={padPointerUp}
      >
        <svg viewBox="0 0 100 60" preserveAspectRatio="none">
          {#each state.ink_strokes.filter((s) => state && s.page === state.current_page) as stroke}
            <polyline
              points={stroke.points.map((p) => `${p[0] * 100},${p[1] * 60}`).join(" ")}
              fill="none"
              stroke={stroke.color}
              stroke-width="0.5"
              stroke-linecap="round"
              stroke-linejoin="round"
            />
          {/each}
        </svg>
        <span class="pad-hint">Drag here to draw — saved live to the audience window.</span>
      </div>
    {/if}

    <details class="cheats">
      <summary>Keyboard shortcuts</summary>
      <dl>
        <dt><kbd>→</kbd> <kbd>Space</kbd></dt><dd>Next page</dd>
        <dt><kbd>←</kbd></dt><dd>Previous page</dd>
        <dt><kbd>Home</kbd> / <kbd>End</kbd></dt><dd>Jump to first / last</dd>
        <dt><kbd>B</kbd></dt><dd>Blackout</dd>
        <dt><kbd>W</kbd></dt><dd>Whiteboard</dd>
        <dt><kbd>L</kbd></dt><dd>Laser pointer</dd>
        <dt><kbd>I</kbd></dt><dd>Ink mode</dd>
        <dt><kbd>.</kbd></dt><dd>Spotlight</dd>
        <dt><kbd>U</kbd> / <kbd>C</kbd></dt><dd>Undo / Clear strokes</dd>
        <dt><kbd>Esc</kbd></dt><dd>Exit Theater</dd>
      </dl>
    </details>
  {/if}

  {#if errorMsg}
    <div class="err" role="alert">
      <strong>Couldn't run that command.</strong>
      <p>{errorMsg}</p>
      <button on:click={() => (errorMsg = null)}>Dismiss</button>
    </div>
  {/if}
</section>

<style>
  .theater-panel {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 16px;
    color: var(--panel-fg, #e6e6e6);
    font-family: var(--ui-font, system-ui, -apple-system, sans-serif);
  }

  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    border-bottom: 1px solid var(--panel-border, rgba(255, 255, 255, 0.08));
    padding-bottom: 10px;
  }
  .head h2 {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 16px;
    font-weight: 600;
    margin: 0;
  }
  .head .icon { font-size: 18px; }
  .head .badge {
    font-size: 10px;
    padding: 2px 6px;
    border-radius: 99px;
    background: var(--accent-dim, rgba(218, 165, 32, 0.15));
    color: var(--accent, #daa520);
    letter-spacing: 0.04em;
  }
  .elapsed {
    font-variant-numeric: tabular-nums;
    font-size: 13px;
    color: var(--accent, #daa520);
  }

  .empty-state {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 20px;
    border: 1px dashed var(--panel-border, rgba(255, 255, 255, 0.1));
    border-radius: 10px;
    text-align: center;
  }
  .muted { color: var(--muted-fg, #999); font-size: 13px; margin: 0; }
  .hint { color: var(--muted-fg, #777); font-size: 12px; margin: 0; }
  .hint-soft {
    color: var(--muted-fg, #888);
    font-size: 12px;
    margin: 8px 0 0;
    max-width: 320px;
    line-height: 1.45;
    text-align: center;
  }

  .status-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .page-label { font-size: 14px; font-weight: 500; }

  .overlays {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 6px;
  }
  .overlays button {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    border-radius: 8px;
    background: var(--btn-bg, rgba(255, 255, 255, 0.04));
    border: 1px solid var(--panel-border, rgba(255, 255, 255, 0.08));
    color: inherit;
    cursor: pointer;
    transition: background 0.12s ease, border-color 0.12s ease;
    font-size: 13px;
  }
  .overlays button:hover { background: var(--btn-bg-hover, rgba(255, 255, 255, 0.08)); }
  .overlays button.active {
    border-color: var(--accent, #daa520);
    background: var(--accent-dim, rgba(218, 165, 32, 0.12));
    color: var(--accent, #daa520);
  }
  .dot {
    width: 10px; height: 10px; border-radius: 50%;
    border: 1px solid rgba(255, 255, 255, 0.2);
  }
  .dot-black { background: #000; }
  .dot-white { background: #fff; }
  .dot-laser { background: #ff3b30; }
  .dot-ink { background: #4f8cff; }
  .dot-spot { background: radial-gradient(circle, #fff 40%, transparent 60%); }

  .ink-controls {
    display: flex;
    align-items: center;
    justify-content: space-between;
    border-top: 1px solid var(--panel-border, rgba(255, 255, 255, 0.08));
    padding-top: 10px;
  }
  .muted-small { color: var(--muted-fg, #999); font-size: 12px; }
  .ink-buttons { display: flex; gap: 6px; }
  .ink-buttons button {
    padding: 6px 10px;
    border-radius: 6px;
    background: var(--btn-bg, rgba(255, 255, 255, 0.04));
    border: 1px solid var(--panel-border, rgba(255, 255, 255, 0.08));
    color: inherit;
    cursor: pointer;
    font-size: 12px;
  }
  .ink-buttons button:disabled { opacity: 0.4; cursor: not-allowed; }
  .ink-buttons .danger:not(:disabled):hover { color: #ff6b6b; }

  .ink-pad {
    position: relative;
    aspect-ratio: 5 / 3;
    border: 1px solid var(--panel-border, rgba(255, 255, 255, 0.12));
    border-radius: 8px;
    background: var(--panel-bg, #0d0d0d);
    overflow: hidden;
    touch-action: none;
  }
  .ink-pad svg { width: 100%; height: 100%; display: block; }
  .pad-hint {
    position: absolute; bottom: 6px; left: 8px; right: 8px;
    font-size: 11px; color: var(--muted-fg, #888);
    pointer-events: none;
  }

  .cheats {
    border-top: 1px solid var(--panel-border, rgba(255, 255, 255, 0.08));
    padding-top: 10px;
    font-size: 12px;
  }
  .cheats summary { cursor: pointer; user-select: none; color: var(--muted-fg, #aaa); }
  .cheats dl {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 4px 12px;
    margin: 8px 0 0;
  }
  .cheats dt { display: flex; gap: 4px; align-items: center; }
  .cheats dd { margin: 0; color: var(--muted-fg, #aaa); }
  kbd {
    display: inline-block;
    padding: 1px 6px;
    border-radius: 4px;
    background: var(--kbd-bg, rgba(255, 255, 255, 0.07));
    border: 1px solid var(--panel-border, rgba(255, 255, 255, 0.12));
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 11px;
  }

  .primary {
    padding: 10px 14px;
    border-radius: 8px;
    background: var(--accent, #daa520);
    color: var(--accent-fg, #1a1a1a);
    border: none;
    cursor: pointer;
    font-weight: 600;
    font-size: 13px;
  }
  .primary:disabled { opacity: 0.4; cursor: not-allowed; }
  .ghost {
    padding: 6px 10px;
    background: transparent;
    border: 1px solid var(--panel-border, rgba(255, 255, 255, 0.12));
    border-radius: 6px;
    color: inherit;
    cursor: pointer;
    font-size: 12px;
  }
  .ghost.danger:not(:disabled):hover {
    color: #ff6b6b;
    border-color: #ff6b6b;
  }
  .danger { color: #ff6b6b; }

  .err {
    padding: 10px;
    border-radius: 8px;
    background: rgba(255, 107, 107, 0.08);
    border: 1px solid rgba(255, 107, 107, 0.3);
    color: #ff6b6b;
    font-size: 12px;
  }
  .err p { margin: 4px 0 8px; }
  .err button {
    padding: 4px 10px;
    border-radius: 4px;
    background: transparent;
    border: 1px solid rgba(255, 107, 107, 0.4);
    color: inherit;
    cursor: pointer;
    font-size: 11px;
  }

  @media (prefers-reduced-motion: reduce) {
    .overlays button { transition: none; }
  }
</style>
