<script lang="ts">
  /*
    /theater-control presenter window (v2.3.0 Slice 5)

    The operator's window. Shows:
      - the current slide, in a roomy preview
      - the next slide, dimmer and smaller (planning ahead)
      - a session timer (huge, glanceable)
      - speaker notes (free-text, saved per-slide via slab_kv_set)
      - a toolbar — Prev / Next / End / Jump + chips for the toggles
      - the full keyboard cheat-sheet, always visible

    Every interaction here drives the backend via `slab_theater_*`
    commands. The backend then broadcasts `slab:theater-state` to every
    open window — including this one — so we render from the single
    source of truth instead of optimistic local mutation.

    Keyboard map (mirrors PresenterOverlay's bindings):
      Right / Space / PageDown   → next
      Left / PageUp              → prev
      Home / End                 → first / last
      B                          → blackout
      W                          → whiteout
      L                          → laser
      I                          → ink mode
      .                          → spotlight
      U                          → undo last stroke
      C                          → clear strokes (current page)
      Esc                        → end session + close windows

    Notes are persisted via the existing `slab_kv_set` / `slab_kv_get`
    commands (key = `theater.notes:<pdf path>:<page>`). Anything fancier
    waits for Atlas — for now a textarea + debounced save is enough.
  */
  import { onDestroy, onMount, tick } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import {
    theaterSnapshot,
    theaterNext,
    theaterPrev,
    theaterJump,
    theaterEnd,
    theaterToggleBlackout,
    theaterToggleWhiteout,
    theaterToggleLaser,
    theaterToggleInk,
    theaterToggleSpotlight,
    theaterUndoStroke,
    theaterClearStrokes,
    theaterCloseWindows,
    type TheaterState,
  } from "$lib/theater";
  import TheaterCanvas from "$lib/components/TheaterCanvas.svelte";

  // ---- State ----
  let state: TheaterState | null = null;
  let unlisten: UnlistenFn | null = null;
  let initErr: string | null = null;
  let busyMsg: string | null = null;

  // Timer ticks every second; we display HH:MM:SS or MM:SS.
  let timerLabel = "00:00";
  let timerHandle: ReturnType<typeof setInterval> | null = null;

  // Notes textarea + debounced save.
  let notes = "";
  let lastSavedNotes = "";
  let notesSaveHandle: ReturnType<typeof setTimeout> | null = null;
  let notesKey: string | null = null;

  // Jump-to-page input.
  let jumpInput = "";
  let jumpErr: string | null = null;

  // ---- Lifecycle ----
  onMount(async () => {
    try {
      state = await theaterSnapshot();
    } catch (e) {
      initErr = e instanceof Error ? e.message : String(e);
    }
    unlisten = await listen<TheaterState>("slab:theater-state", (ev) => {
      state = ev.payload;
      void reloadNotesForCurrent();
    });
    timerHandle = setInterval(updateTimer, 1000);
    updateTimer();
    window.addEventListener("keydown", onKey);
    await reloadNotesForCurrent();
    await tick();
  });

  onDestroy(() => {
    if (unlisten) unlisten();
    if (timerHandle) clearInterval(timerHandle);
    if (notesSaveHandle) clearTimeout(notesSaveHandle);
    window.removeEventListener("keydown", onKey);
  });

  // ---- Time formatting ----
  function updateTimer() {
    if (!state) {
      timerLabel = "00:00";
      return;
    }
    const ms = Date.now() - state.started_at_ms;
    const total = Math.max(0, Math.floor(ms / 1000));
    const h = Math.floor(total / 3600);
    const m = Math.floor((total % 3600) / 60);
    const s = total % 60;
    const mm = String(m).padStart(2, "0");
    const ss = String(s).padStart(2, "0");
    timerLabel = h > 0 ? `${h}:${mm}:${ss}` : `${mm}:${ss}`;
  }

  // ---- Notes persistence ----
  function notesKeyFor(st: TheaterState): string {
    return `theater.notes:${st.path}:${st.current_page}`;
  }

  async function reloadNotesForCurrent() {
    if (!state) return;
    const key = notesKeyFor(state);
    if (key === notesKey) return;
    notesKey = key;
    // Notes are stored in webview localStorage — survives presenter
    // window restart, doesn't survive an OS reinstall. A proper backend
    // key/value store lands with Atlas (v2.2.0); this is the lightest
    // shim that already makes notes useful end-to-end this slice.
    try {
      const stored = window.localStorage.getItem(key);
      notes = stored ?? "";
      lastSavedNotes = notes;
    } catch {
      notes = "";
      lastSavedNotes = "";
    }
  }

  function scheduleNotesSave() {
    if (notesSaveHandle) clearTimeout(notesSaveHandle);
    notesSaveHandle = setTimeout(saveNotesNow, 400);
  }

  function saveNotesNow() {
    if (!notesKey) return;
    if (notes === lastSavedNotes) return;
    try {
      window.localStorage.setItem(notesKey, notes);
      lastSavedNotes = notes;
    } catch (e) {
      // eslint-disable-next-line no-console
      console.warn("[theater-control] localStorage.setItem failed", e);
    }
    if (notesSaveHandle) {
      clearTimeout(notesSaveHandle);
      notesSaveHandle = null;
    }
  }

  // ---- Action wrappers (with error surface) ----
  async function withBusy<T>(label: string, fn: () => Promise<T>): Promise<void> {
    busyMsg = null;
    try {
      await fn();
    } catch (e) {
      busyMsg = `${label}: ${e instanceof Error ? e.message : String(e)}`;
    }
  }

  const doNext = () => withBusy("next", () => theaterNext());
  const doPrev = () => withBusy("prev", () => theaterPrev());
  const doJump = (n: number) => withBusy("jump", () => theaterJump(n));
  const doBlackout = () =>
    withBusy("blackout", () => theaterToggleBlackout());
  const doWhiteout = () =>
    withBusy("whiteout", () => theaterToggleWhiteout());
  const doLaser = () => withBusy("laser", () => theaterToggleLaser());
  const doInk = () => withBusy("ink", () => theaterToggleInk());
  const doSpotlight = () =>
    withBusy("spotlight", () => theaterToggleSpotlight());
  const doUndo = () => withBusy("undo", () => theaterUndoStroke());
  const doClear = () => withBusy("clear", () => theaterClearStrokes());

  async function doEnd() {
    busyMsg = null;
    try {
      await theaterEnd();
      await theaterCloseWindows();
    } catch (e) {
      busyMsg = `end: ${e instanceof Error ? e.message : String(e)}`;
    }
  }

  function onJumpSubmit(ev: Event) {
    ev.preventDefault();
    jumpErr = null;
    const n = parseInt(jumpInput, 10);
    if (!state) return;
    if (!Number.isFinite(n) || n < 1 || n > state.total_pages) {
      jumpErr = `Page must be 1..${state.total_pages}`;
      return;
    }
    jumpInput = "";
    void doJump(n);
  }

  // ---- Keyboard ----
  function onKey(e: KeyboardEvent) {
    // Don't hijack keys while typing in the notes textarea.
    const tgt = e.target as HTMLElement | null;
    if (tgt && (tgt.tagName === "TEXTAREA" || tgt.tagName === "INPUT")) {
      // Allow Esc to escape the textarea, then end on next Esc.
      if (e.key === "Escape") {
        (tgt as HTMLElement).blur();
        e.preventDefault();
      }
      return;
    }
    switch (e.key) {
      case "ArrowRight":
      case " ":
      case "PageDown":
        e.preventDefault();
        void doNext();
        return;
      case "ArrowLeft":
      case "PageUp":
        e.preventDefault();
        void doPrev();
        return;
      case "Home":
        e.preventDefault();
        void doJump(1);
        return;
      case "End":
        if (state) {
          e.preventDefault();
          void doJump(state.total_pages);
        }
        return;
      case "b":
      case "B":
        e.preventDefault();
        void doBlackout();
        return;
      case "w":
      case "W":
        e.preventDefault();
        void doWhiteout();
        return;
      case "l":
      case "L":
        e.preventDefault();
        void doLaser();
        return;
      case "i":
      case "I":
        e.preventDefault();
        void doInk();
        return;
      case ".":
        e.preventDefault();
        void doSpotlight();
        return;
      case "u":
      case "U":
        e.preventDefault();
        void doUndo();
        return;
      case "c":
      case "C":
        e.preventDefault();
        void doClear();
        return;
      case "Escape":
        e.preventDefault();
        void doEnd();
        return;
    }
  }

  // Reactive next page index, clamped to total.
  $: nextPage = state
    ? Math.min(state.current_page + 1, state.total_pages)
    : 1;
  $: hasNext = !!state && state.current_page < state.total_pages;
  $: notesSaving = notesSaveHandle !== null && notes !== lastSavedNotes;
</script>

<div class="control-root">
  <header class="topbar">
    <div class="timer" aria-label="Elapsed time">{timerLabel}</div>
    {#if state}
      <div class="meta">
        <span class="page-info">Page {state.current_page} of {state.total_pages}</span>
        <span class="dot">·</span>
        <span class="path" title={state.path}>{shortName(state.path)}</span>
      </div>
    {/if}
    <div class="spacer"></div>
    <button class="end-btn" on:click={doEnd}>End session (Esc)</button>
  </header>

  {#if busyMsg}
    <div class="banner err" role="alert">{busyMsg}</div>
  {/if}

  {#if !state}
    <div class="empty">
      <div class="empty-title">No active Theater session</div>
      <div class="empty-body">
        Open a PDF, click <b>Present</b> in the sidebar, and the audience
        + control windows will spawn here.
      </div>
      {#if initErr}<div class="empty-err">{initErr}</div>{/if}
    </div>
  {:else}
    <main class="grid">
      <section class="current">
        <div class="label">Current slide</div>
        <div class="canvas-wrap">
          <TheaterCanvas path={state.path} page={state.current_page} />
          {#if state.blackout}
            <div class="overlay-tag" data-kind="blackout">BLACKOUT</div>
          {:else if state.whiteout}
            <div class="overlay-tag" data-kind="whiteout">WHITEOUT</div>
          {/if}
        </div>
      </section>

      <section class="next">
        <div class="label">{hasNext ? "Next slide" : "End of deck"}</div>
        <div class="canvas-wrap dim">
          {#if hasNext}
            <TheaterCanvas path={state.path} page={nextPage} />
          {:else}
            <div class="end-of-deck">Last slide</div>
          {/if}
        </div>
      </section>

      <section class="notes">
        <div class="label-row">
          <span class="label">Speaker notes — page {state.current_page}</span>
          <span class="save-state" aria-live="polite">
            {notesSaving ? "saving…" : notes ? "saved" : ""}
          </span>
        </div>
        <textarea
          class="notes-area"
          bind:value={notes}
          on:input={scheduleNotesSave}
          on:blur={saveNotesNow}
          placeholder="Speaker notes for this slide. Auto-saved."
          spellcheck="true"
        ></textarea>
      </section>

      <section class="toolbar">
        <div class="btn-row">
          <button on:click={doPrev} disabled={state.current_page <= 1}>‹ Prev</button>
          <button class="primary" on:click={doNext} disabled={!hasNext}
            >Next ›</button
          >
          <form class="jump-form" on:submit={onJumpSubmit}>
            <input
              type="number"
              min="1"
              max={state.total_pages}
              placeholder={`1–${state.total_pages}`}
              bind:value={jumpInput}
            />
            <button type="submit">Jump</button>
          </form>
          {#if jumpErr}
            <span class="err-inline">{jumpErr}</span>
          {/if}
        </div>

        <div class="chip-row" role="group" aria-label="Overlay toggles">
          <button
            class="chip"
            class:on={state.blackout}
            on:click={doBlackout}
            title="B"
          >
            ⬛ Blackout
          </button>
          <button
            class="chip"
            class:on={state.whiteout}
            on:click={doWhiteout}
            title="W"
          >
            ⬜ Whiteout
          </button>
          <button
            class="chip"
            class:on={state.laser_on}
            on:click={doLaser}
            title="L"
          >
            🔴 Laser
          </button>
          <button
            class="chip"
            class:on={state.ink_mode}
            on:click={doInk}
            title="I"
          >
            ✏️ Ink
          </button>
          <button
            class="chip"
            class:on={state.spotlight_on}
            on:click={doSpotlight}
            title="."
          >
            ◌ Spotlight
          </button>
          <button class="chip" on:click={doUndo} title="U">↶ Undo ink</button>
          <button class="chip" on:click={doClear} title="C">⌫ Clear page</button>
        </div>

        <details class="cheatsheet">
          <summary>Keyboard shortcuts</summary>
          <ul>
            <li><kbd>→</kbd> <kbd>Space</kbd> <kbd>PgDn</kbd> — next slide</li>
            <li><kbd>←</kbd> <kbd>PgUp</kbd> — previous slide</li>
            <li><kbd>Home</kbd> / <kbd>End</kbd> — first / last slide</li>
            <li><kbd>B</kbd> — blackout · <kbd>W</kbd> — whiteout</li>
            <li><kbd>L</kbd> — laser · <kbd>I</kbd> — ink · <kbd>.</kbd> — spotlight</li>
            <li><kbd>U</kbd> — undo ink · <kbd>C</kbd> — clear page</li>
            <li><kbd>Esc</kbd> — end session and close audience window</li>
          </ul>
        </details>
      </section>
    </main>
  {/if}
</div>

<script context="module" lang="ts">
  /** Strip directory parts and trailing .pdf for the topbar readout. */
  export function shortName(p: string): string {
    const i = Math.max(p.lastIndexOf("/"), p.lastIndexOf("\\"));
    const base = i >= 0 ? p.slice(i + 1) : p;
    return base.length > 64 ? base.slice(0, 61) + "…" : base;
  }
</script>

<style>
  :global(html),
  :global(body) {
    margin: 0;
    height: 100%;
    background: #0e1014;
    color: #e6e8ee;
    font-family:
      -apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif;
    overflow: hidden;
  }
  .control-root {
    position: fixed;
    inset: 0;
    display: grid;
    grid-template-rows: auto auto 1fr;
    gap: 0;
    background:
      radial-gradient(1200px 600px at 10% -10%, #1e2230 0, transparent 50%),
      radial-gradient(900px 500px at 110% 120%, #1a2030 0, transparent 50%),
      #0e1014;
  }

  /* ---- Topbar ---- */
  .topbar {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 14px 20px 10px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.06);
  }
  .timer {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 36px;
    font-weight: 600;
    color: #fafbff;
    letter-spacing: 0.02em;
    line-height: 1;
    font-variant-numeric: tabular-nums;
  }
  .meta {
    color: rgba(230, 232, 238, 0.7);
    font-size: 13px;
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .meta .dot { opacity: 0.4; }
  .meta .path {
    color: rgba(230, 232, 238, 0.55);
    max-width: 40ch;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .spacer { flex: 1; }
  .end-btn {
    appearance: none;
    background: rgba(255, 88, 88, 0.12);
    color: #ff8d8d;
    border: 1px solid rgba(255, 88, 88, 0.4);
    padding: 8px 14px;
    border-radius: 8px;
    font-weight: 600;
    cursor: pointer;
    transition: background 120ms ease, transform 80ms ease;
  }
  .end-btn:hover { background: rgba(255, 88, 88, 0.22); }
  .end-btn:active { transform: translateY(1px); }

  .banner {
    padding: 8px 20px;
    font-size: 13px;
    background: rgba(255, 88, 88, 0.1);
    color: #ff9b9b;
    border-bottom: 1px solid rgba(255, 88, 88, 0.2);
  }

  /* ---- Empty state ---- */
  .empty {
    grid-row: 3;
    display: grid;
    place-items: center;
    text-align: center;
    padding: 64px;
  }
  .empty-title { font-size: 22px; font-weight: 600; margin-bottom: 8px; }
  .empty-body { color: rgba(230, 232, 238, 0.7); max-width: 52ch; }
  .empty-err {
    margin-top: 16px;
    color: #ffb4b4;
    font-size: 12px;
  }

  /* ---- Grid ---- */
  .grid {
    display: grid;
    grid-template-columns: 1.4fr 1fr;
    grid-template-rows: 1fr 1fr;
    grid-template-areas:
      "current notes"
      "next toolbar";
    gap: 14px;
    padding: 14px 20px 20px;
    min-height: 0;
  }
  .current { grid-area: current; }
  .next { grid-area: next; }
  .notes { grid-area: notes; }
  .toolbar { grid-area: toolbar; }
  .current,
  .next,
  .notes,
  .toolbar {
    background: rgba(255, 255, 255, 0.03);
    border: 1px solid rgba(255, 255, 255, 0.06);
    border-radius: 12px;
    padding: 12px;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .label {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: rgba(230, 232, 238, 0.55);
    margin-bottom: 8px;
  }
  .label-row {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    margin-bottom: 8px;
  }
  .save-state {
    font-size: 11px;
    color: rgba(230, 232, 238, 0.45);
  }
  .canvas-wrap {
    position: relative;
    flex: 1;
    min-height: 0;
    border-radius: 8px;
    overflow: hidden;
    background: rgba(0, 0, 0, 0.4);
  }
  .canvas-wrap.dim {
    filter: brightness(0.75) saturate(0.85);
  }
  .end-of-deck {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    color: rgba(230, 232, 238, 0.5);
    font-size: 14px;
  }
  .overlay-tag {
    position: absolute;
    top: 8px;
    left: 8px;
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 11px;
    letter-spacing: 0.1em;
    padding: 4px 8px;
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.85);
    color: #111;
  }
  .overlay-tag[data-kind="blackout"] {
    background: #111;
    color: #fff;
    border: 1px solid rgba(255, 255, 255, 0.25);
  }

  .notes-area {
    flex: 1;
    background: rgba(0, 0, 0, 0.25);
    border: 1px solid rgba(255, 255, 255, 0.06);
    border-radius: 8px;
    color: #e6e8ee;
    padding: 10px 12px;
    font: inherit;
    font-size: 13px;
    line-height: 1.45;
    resize: none;
    outline: none;
  }
  .notes-area:focus {
    border-color: rgba(100, 160, 255, 0.5);
    box-shadow: 0 0 0 3px rgba(100, 160, 255, 0.18);
  }

  /* ---- Toolbar ---- */
  .toolbar {
    gap: 10px;
  }
  .btn-row,
  .chip-row {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
  }
  .toolbar button {
    appearance: none;
    background: rgba(255, 255, 255, 0.06);
    color: #e6e8ee;
    border: 1px solid rgba(255, 255, 255, 0.08);
    padding: 8px 12px;
    border-radius: 8px;
    font-size: 13px;
    cursor: pointer;
    transition: background 120ms ease, border-color 120ms ease;
  }
  .toolbar button:hover { background: rgba(255, 255, 255, 0.12); }
  .toolbar button:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  .toolbar .primary {
    background: rgba(100, 160, 255, 0.18);
    color: #cfe1ff;
    border-color: rgba(100, 160, 255, 0.5);
  }
  .toolbar .primary:hover { background: rgba(100, 160, 255, 0.28); }
  .toolbar .chip {
    padding: 6px 10px;
    font-size: 12px;
    border-radius: 999px;
  }
  .toolbar .chip.on {
    background: rgba(100, 220, 130, 0.2);
    border-color: rgba(100, 220, 130, 0.5);
    color: #c8f5d4;
  }
  .jump-form {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .jump-form input {
    width: 78px;
    padding: 7px 8px;
    background: rgba(0, 0, 0, 0.3);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 6px;
    color: #e6e8ee;
    font-size: 13px;
    font-variant-numeric: tabular-nums;
  }
  .err-inline {
    color: #ff9b9b;
    font-size: 12px;
  }

  .cheatsheet {
    margin-top: 4px;
    font-size: 12px;
    color: rgba(230, 232, 238, 0.7);
  }
  .cheatsheet summary {
    cursor: pointer;
    user-select: none;
  }
  .cheatsheet ul {
    margin: 8px 0 0;
    padding: 0;
    list-style: none;
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 4px 16px;
  }
  .cheatsheet kbd {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.14);
    border-bottom-width: 2px;
    padding: 0 6px;
    border-radius: 4px;
    font-size: 11px;
  }
</style>
