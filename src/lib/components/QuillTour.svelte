<script lang="ts">
  /**
   * v3.29.0 "Forms Tour" — 5-step coachmark overlay for the Forms Hub.
   *
   * Why this exists: v3.28.0 unified four Acrobat-killer features into one
   * tabbed workspace, but a first-time visitor has no clue which tab solves
   * their problem. This component fires once on first visit and walks the
   * user through Detect → Design → Fill → Batch in 30 seconds.
   *
   * Design:
   *   - Full-screen scrim + cut-out "spotlight" on the anchor element.
   *   - Tooltip bubble auto-positions based on placement + viewport bounds.
   *   - Keyboard-first: ←/→ navigate, Enter advances, Esc skips.
   *   - Pagination dots are click-targets for direct jumps.
   *   - "Center" placement renders the bubble dead-center with no spotlight
   *     (used for the welcome step where there's nothing specific to point at).
   *
   * Plays nicely with the existing Hub: it never reads or writes the Quill
   * store, only its own `quill-tour` store. The Hub is responsible for
   * calling `startTour()` if `shouldAutoStart()` returns true.
   */
  import { onMount, onDestroy } from "svelte";
  import {
    tour,
    TOUR_STEPS,
    nextStep,
    prevStep,
    gotoStep,
    skipTour,
    finishTour,
  } from "$lib/quill-tour";

  type Rect = { top: number; left: number; width: number; height: number };

  let tourState = $derived($tour);
  let rect: Rect | null = $state(null);
  let bubbleEl: HTMLDivElement | null = $state(null);

  // Recompute the spotlight + bubble position whenever the step changes,
  // and on window resize / scroll. We poll via `requestAnimationFrame`
  // for the first paint after the Hub renders, then settle into resize
  // listeners only.
  function measure() {
    if (tourState.step === null) {
      rect = null;
      return;
    }
    const step = TOUR_STEPS[tourState.step];
    if (step.placement === "center") {
      rect = null;
      return;
    }
    const el = document.querySelector(step.anchor) as HTMLElement | null;
    if (!el) {
      // Anchor not in DOM yet — retry next frame. This handles the case
      // where the tour fires before the Hub finishes mounting.
      rect = null;
      requestAnimationFrame(measure);
      return;
    }
    const r = el.getBoundingClientRect();
    rect = { top: r.top, left: r.left, width: r.width, height: r.height };
  }

  $effect(() => {
    // Re-run whenever the active step changes.
    void tourState.step;
    measure();
  });

  function onResize() {
    measure();
  }

  function onKey(e: KeyboardEvent) {
    if (tourState.step === null) return;
    if (e.key === "Escape") {
      e.preventDefault();
      skipTour();
    } else if (e.key === "ArrowRight" || e.key === "Enter") {
      e.preventDefault();
      nextStep();
    } else if (e.key === "ArrowLeft") {
      e.preventDefault();
      prevStep();
    }
  }

  onMount(() => {
    window.addEventListener("resize", onResize);
    window.addEventListener("scroll", onResize, true);
    window.addEventListener("keydown", onKey);
  });

  onDestroy(() => {
    if (typeof window === "undefined") return;
    window.removeEventListener("resize", onResize);
    window.removeEventListener("scroll", onResize, true);
    window.removeEventListener("keydown", onKey);
  });

  // Pick a tooltip position adjacent to the anchor. We clamp to viewport
  // so the bubble never hangs off the screen.
  function bubbleStyle(): string {
    if (tourState.step === null) return "display:none";
    const step = TOUR_STEPS[tourState.step];
    if (step.placement === "center" || !rect) {
      return "top:50%;left:50%;transform:translate(-50%,-50%)";
    }
    const W = window.innerWidth;
    const H = window.innerHeight;
    const BW = 360; // bubble width — matches CSS
    const GAP = 14;
    let top = 0;
    let left = 0;
    switch (step.placement) {
      case "bottom":
        top = rect.top + rect.height + GAP;
        left = rect.left + rect.width / 2 - BW / 2;
        break;
      case "top":
        top = rect.top - GAP - 180; // approximate bubble height
        left = rect.left + rect.width / 2 - BW / 2;
        break;
      case "right":
        top = rect.top + rect.height / 2 - 90;
        left = rect.left + rect.width + GAP;
        break;
      case "left":
        top = rect.top + rect.height / 2 - 90;
        left = rect.left - GAP - BW;
        break;
    }
    // Clamp to viewport with 12px margin.
    left = Math.max(12, Math.min(left, W - BW - 12));
    top = Math.max(12, Math.min(top, H - 200));
    return `top:${top}px;left:${left}px`;
  }

  function spotlightStyle(): string {
    if (!rect) return "display:none";
    const PAD = 6;
    return `top:${rect.top - PAD}px;left:${rect.left - PAD}px;width:${rect.width + PAD * 2}px;height:${rect.height + PAD * 2}px`;
  }

  let isLast = $derived(
    tourState.step !== null && tourState.step >= TOUR_STEPS.length - 1,
  );
</script>

{#if tourState.step !== null}
  <div
    class="tour-root"
    role="dialog"
    aria-modal="true"
    aria-labelledby="tour-title"
    data-testid="quill-tour"
  >
    <!-- The scrim dims everything except the spotlight. -->
    <div class="scrim" onclick={() => skipTour()} role="presentation"></div>

    {#if rect}
      <!-- The spotlight: a translucent rectangle with a glowing outline
           that draws the user's eye to the anchored element. We use a
           secondary outline ring instead of a true CSS mask cutout to
           keep the implementation portable across browsers. -->
      <div
        class="spotlight"
        style={spotlightStyle()}
        aria-hidden="true"
        data-testid="quill-tour-spotlight"
      ></div>
    {/if}

    <div
      class="bubble"
      style={bubbleStyle()}
      bind:this={bubbleEl}
      data-testid="quill-tour-bubble"
    >
      <header class="bubble-head">
        {#if TOUR_STEPS[tourState.step].glyph}
          <span class="glyph" aria-hidden="true"
            >{TOUR_STEPS[tourState.step].glyph}</span
          >
        {/if}
        <h3 id="tour-title">{TOUR_STEPS[tourState.step].title}</h3>
        <button
          class="x"
          onclick={() => skipTour()}
          aria-label="Skip tour"
          title="Skip (Esc)">×</button
        >
      </header>
      <p class="body">{TOUR_STEPS[tourState.step].body}</p>
      <footer class="bubble-foot">
        <div class="dots" role="tablist" aria-label="Tour progress">
          {#each TOUR_STEPS as _, i}
            <button
              class="dot"
              class:active={i === tourState.step}
              onclick={() => gotoStep(i)}
              aria-label={`Go to step ${i + 1}`}
              aria-current={i === tourState.step ? "step" : undefined}
              role="tab"
              tabindex={i === tourState.step ? 0 : -1}
            ></button>
          {/each}
        </div>
        <div class="nav">
          {#if tourState.step > 0}
            <button class="ghost" onclick={() => prevStep()} aria-label="Back">
              ← Back
            </button>
          {/if}
          {#if isLast}
            <button
              class="primary"
              onclick={() => finishTour()}
              autofocus
              data-testid="quill-tour-finish"
            >
              Got it ✨
            </button>
          {:else}
            <button
              class="primary"
              onclick={() => nextStep()}
              autofocus
              data-testid="quill-tour-next"
            >
              Next →
            </button>
          {/if}
        </div>
      </footer>
    </div>
  </div>
{/if}

<style>
  .tour-root {
    position: fixed;
    inset: 0;
    z-index: 9000;
    pointer-events: none;
  }
  .scrim {
    position: absolute;
    inset: 0;
    background: rgba(0, 0, 0, 0.62);
    backdrop-filter: blur(1.5px);
    pointer-events: auto;
    animation: fade-in 180ms ease;
  }
  .spotlight {
    position: absolute;
    border-radius: 10px;
    box-shadow:
      0 0 0 9999px transparent,
      0 0 0 2px var(--accent-strong, #88b0ff),
      0 0 24px 4px rgba(120, 160, 255, 0.55);
    background: rgba(120, 160, 255, 0.07);
    pointer-events: none;
    animation: pulse 2.4s ease-in-out infinite;
    transition:
      top 220ms cubic-bezier(0.22, 0.61, 0.36, 1),
      left 220ms cubic-bezier(0.22, 0.61, 0.36, 1),
      width 220ms cubic-bezier(0.22, 0.61, 0.36, 1),
      height 220ms cubic-bezier(0.22, 0.61, 0.36, 1);
  }
  .bubble {
    position: absolute;
    width: 360px;
    pointer-events: auto;
    background: var(--glass-strong, rgba(22, 24, 32, 0.96));
    color: var(--text, #f0f2f8);
    border: 1px solid var(--glass-border, rgba(255, 255, 255, 0.12));
    border-radius: 14px;
    box-shadow:
      0 18px 48px rgba(0, 0, 0, 0.55),
      0 2px 6px rgba(0, 0, 0, 0.3);
    padding: 16px 18px 14px;
    animation: bubble-in 220ms cubic-bezier(0.22, 0.61, 0.36, 1);
    transition:
      top 220ms cubic-bezier(0.22, 0.61, 0.36, 1),
      left 220ms cubic-bezier(0.22, 0.61, 0.36, 1);
  }
  .bubble-head {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 8px;
  }
  .glyph {
    font-size: 22px;
    line-height: 1;
  }
  .bubble-head h3 {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
    flex: 1;
    min-width: 0;
  }
  .x {
    background: transparent;
    border: none;
    color: inherit;
    opacity: 0.55;
    font-size: 20px;
    line-height: 1;
    cursor: pointer;
    padding: 2px 6px;
    border-radius: 6px;
  }
  .x:hover {
    opacity: 1;
    background: var(--glass-hover, rgba(255, 255, 255, 0.06));
  }
  .body {
    margin: 0 0 14px;
    font-size: 13px;
    line-height: 1.5;
    opacity: 0.88;
  }
  .bubble-foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }
  .dots {
    display: flex;
    gap: 6px;
  }
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--glass-border, rgba(255, 255, 255, 0.18));
    border: none;
    padding: 0;
    cursor: pointer;
    transition: background 120ms ease, transform 120ms ease;
  }
  .dot:hover {
    transform: scale(1.25);
  }
  .dot.active {
    background: var(--accent-strong, #88b0ff);
    transform: scale(1.25);
  }
  .nav {
    display: flex;
    gap: 8px;
  }
  .primary {
    padding: 6px 14px;
    border-radius: 8px;
    border: 1px solid var(--accent-strong, #88b0ff);
    background: var(--accent-soft, rgba(120, 160, 255, 0.22));
    color: var(--accent-strong, #88b0ff);
    font-weight: 600;
    font-size: 12px;
    cursor: pointer;
  }
  .primary:hover {
    background: rgba(120, 160, 255, 0.32);
  }
  .ghost {
    padding: 6px 12px;
    border-radius: 6px;
    border: 1px solid var(--glass-border, rgba(255, 255, 255, 0.14));
    background: transparent;
    color: inherit;
    font-size: 12px;
    cursor: pointer;
  }
  .ghost:hover {
    background: var(--glass-hover, rgba(255, 255, 255, 0.04));
  }

  @keyframes fade-in {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }
  @keyframes bubble-in {
    from {
      opacity: 0;
      transform: translateY(6px) scale(0.98);
    }
    to {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  }
  @keyframes pulse {
    0%,
    100% {
      box-shadow:
        0 0 0 9999px transparent,
        0 0 0 2px var(--accent-strong, #88b0ff),
        0 0 24px 4px rgba(120, 160, 255, 0.55);
    }
    50% {
      box-shadow:
        0 0 0 9999px transparent,
        0 0 0 2px var(--accent-strong, #88b0ff),
        0 0 32px 8px rgba(120, 160, 255, 0.75);
    }
  }

  /* Respect users who prefer reduced motion. */
  @media (prefers-reduced-motion: reduce) {
    .scrim,
    .bubble,
    .spotlight {
      animation: none;
      transition: none;
    }
  }
</style>
