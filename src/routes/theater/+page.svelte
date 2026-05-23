<script lang="ts">
  /*
    /theater audience route (v2.3.0 Slice 5)

    Fullscreen, borderless, always-on-top window opened by
    `slab_theater_open_windows`. Renders ONLY the currently-projected
    slide — no UI chrome, no toolbar, no mouse cursor — and listens to
    `slab:theater-state` for live updates from the presenter control
    window. This is what the audience actually sees.

    Layers (z order, bottom to top):
      1. Black backdrop                       (#000, fullscreen)
      2. TheaterCanvas — the PDF page         (pdfjs render)
      3. Ink overlay — SVG normalised strokes (only this page)
      4. Blackout / whiteout overlay          (full screen)
      5. Spotlight mask                       (radial vignette)
      6. Discreet readout (page no + elapsed) (lower-right corner)
  */
  import { onDestroy, onMount } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { theaterSnapshot, type TheaterState } from "$lib/theater";
  import TheaterCanvas from "$lib/components/TheaterCanvas.svelte";

  // ---- State ----
  let state: TheaterState | null = null;
  let unlisten: UnlistenFn | null = null;
  let initErr: string | null = null;
  let elapsed = "00:00";
  let tickHandle: ReturnType<typeof setInterval> | null = null;

  // ---- Lifecycle ----
  onMount(async () => {
    try {
      state = await theaterSnapshot();
    } catch (e) {
      initErr = e instanceof Error ? e.message : String(e);
    }
    unlisten = await listen<TheaterState>("slab:theater-state", (ev) => {
      state = ev.payload;
    });
    tickHandle = setInterval(refreshElapsed, 1000);
    refreshElapsed();
  });

  onDestroy(() => {
    if (unlisten) unlisten();
    if (tickHandle) clearInterval(tickHandle);
  });

  function refreshElapsed() {
    if (!state) {
      elapsed = "00:00";
      return;
    }
    const ms = Date.now() - state.started_at_ms;
    const s = Math.max(0, Math.floor(ms / 1000));
    const mm = String(Math.floor(s / 60)).padStart(2, "0");
    const ss = String(s % 60).padStart(2, "0");
    elapsed = `${mm}:${ss}`;
  }

  // ---- Reactive derived (Svelte 4 style) ----
  $: strokesThisPage = (state?.ink_strokes ?? []).filter(
    (s) => s.page === (state?.current_page ?? -1),
  );
  $: showWhiteout = state?.whiteout ?? false;
  $: showBlackout = state?.blackout ?? false;
  $: showSpotlight = state?.spotlight_on ?? false;
</script>

<div
  class="audience-root"
  class:blackout={showBlackout}
  class:whiteout={showWhiteout}
>
  {#if initErr && !state}
    <div class="boot-msg" role="status">
      Waiting for presenter session…<br /><small>{initErr}</small>
    </div>
  {:else if !state}
    <div class="boot-msg" role="status">Waiting for presenter session…</div>
  {:else if !showBlackout && !showWhiteout}
    <div class="canvas-layer">
      <TheaterCanvas path={state.path} page={state.current_page} />
    </div>
    {#if strokesThisPage.length > 0}
      <svg
        class="ink-layer"
        viewBox="0 0 1 1"
        preserveAspectRatio="none"
        aria-hidden="true"
      >
        {#each strokesThisPage as stroke, i (i)}
          <polyline
            points={stroke.points
              .map((p: [number, number]) => `${p[0]},${p[1]}`)
              .join(" ")}
            stroke={stroke.color}
            stroke-width={stroke.width / 1000}
            fill="none"
            stroke-linecap="round"
            stroke-linejoin="round"
            vector-effect="non-scaling-stroke"
          />
        {/each}
      </svg>
    {/if}
    {#if showSpotlight}
      <!-- Backend cursor target lands in Slice 6; static centered for now. -->
      <div class="spotlight-mask" aria-hidden="true"></div>
    {/if}
    <div class="readout" aria-hidden="true">
      <span class="page-no">{state.current_page} / {state.total_pages}</span>
      <span class="dot">·</span>
      <span class="elapsed">{elapsed}</span>
    </div>
  {/if}
</div>

<style>
  :global(html),
  :global(body) {
    margin: 0;
    padding: 0;
    background: #000;
    overflow: hidden;
    height: 100%;
  }
  .audience-root {
    position: fixed;
    inset: 0;
    background: #000;
    color: #fff;
    cursor: none;
    user-select: none;
    -webkit-user-select: none;
    overflow: hidden;
  }
  .audience-root.blackout {
    background: #000;
  }
  .audience-root.whiteout {
    background: #fff;
  }
  .canvas-layer {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
  }
  .ink-layer {
    position: absolute;
    inset: 0;
    pointer-events: none;
    z-index: 5;
  }
  .spotlight-mask {
    position: absolute;
    inset: 0;
    pointer-events: none;
    background: radial-gradient(
      circle at center,
      rgba(0, 0, 0, 0) 0,
      rgba(0, 0, 0, 0) 12%,
      rgba(0, 0, 0, 0.85) 32%
    );
    z-index: 6;
  }
  .boot-msg {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    color: rgba(255, 255, 255, 0.55);
    font-family:
      -apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif;
    font-size: 18px;
    text-align: center;
  }
  .boot-msg small {
    color: rgba(255, 180, 180, 0.65);
    font-size: 12px;
  }
  .readout {
    position: absolute;
    right: 18px;
    bottom: 14px;
    font-size: 12px;
    color: rgba(255, 255, 255, 0.28);
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    letter-spacing: 0.04em;
    pointer-events: none;
    z-index: 7;
  }
  .readout .dot {
    margin: 0 6px;
  }
</style>
