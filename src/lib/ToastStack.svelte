<script lang="ts">
  import { toasts, dismiss, dismissAll, pauseToast, resumeToast, runToastAction, type Toast } from "$lib/notify";
  import {
    describeToastCount,
    describeClearAll,
    shouldShowClearAll,
    splitToastsByPoliteness,
    announceToast,
    normalizeToastAction,
    createToastSwipe,
    moveToastSwipe,
    toastSwipeShouldDismiss,
    toastSwipeOpacity,
    resolveToastFocusHotkey,
    resolveFocusedToastKey,
    pickToastFocusIndex,
    newestToastFocusIndex,
    hasToastAction,
    resolveToastStackView,
    describeOverflowToggleAria,
    type ToastSwipe,
  } from "$lib/toastStack";
  import { fly } from "svelte/transition";
  import { onMount, tick } from "svelte";

  // Mount once near the root layout. Renders the newest few toasts in the
  // bottom-right corner; a burst beyond the cap collapses the oldest into
  // a "+N more" pill (slice 1) so the stack never fills the viewport.
  // Dismiss on click of × or via the auto-timer set by notify.ts.

  // Expand/collapse state (round-36 slice 5): the overflow "+N more" is
  // now a real toggle. Collapsed -> newest TOAST_MAX_VISIBLE render, older
  // hidden; expanded -> every toast renders, toggle reads "Show less".
  let expanded = $state(false);
  const view = $derived(resolveToastStackView($toasts, expanded));
  // Auto-collapse once the overflow drains so the toggle can't strand the
  // stack in an expanded state with nothing extra to show.
  $effect(() => {
    if (expanded && view.overflowCount === 0) expanded = false;
  });
  const overflowAria = $derived(
    describeOverflowToggleAria(view.overflowCount, view.expanded),
  );
  // Clear-all header (slice 3) tops the stack once 2+ toasts are live,
  // wiring the bulk dismissAll the notify store has always exposed.
  const showClearAll = $derived(shouldShowClearAll($toasts.length));
  const clearAllCopy = $derived(describeClearAll($toasts.length));
  // a11y: the visual stack reorders + coalesces toasts, a poor live
  // region. Mirror every toast into one of two dedicated hidden regions
  // by politeness — errors/warnings assertive, the rest polite — so
  // screen readers announce each exactly once at the right urgency.
  const announce = $derived(splitToastsByPoliteness($toasts));

  function icon(kind: Toast["kind"]): string {
    switch (kind) {
      case "success":
        return "✓";
      case "error":
        return "✕";
      case "warning":
        return "!";
      default:
        return "i";
    }
  }

  // Swipe / drag-to-dismiss (slice 2). One active drag at a time; we track
  // the toast id + pure ToastSwipe model and translate the row by its dx
  // (fading via toastSwipeOpacity). On release a past-threshold / flicked
  // drag dismisses, otherwise the row snaps back. Pointer Events so it
  // works for mouse, trackpad and touch; capture keeps move/up events
  // flowing even if the pointer leaves the toast.
  let swipeId = $state<number | null>(null);
  let swipe = $state<ToastSwipe | null>(null);

  function onPointerDown(e: PointerEvent, id: number) {
    // Primary button / touch / pen only; ignore the action & close buttons
    // (let their own click fire) by checking the event target.
    if (e.button !== 0) return;
    const el = e.target as HTMLElement | null;
    if (el && el.closest("button")) return;
    swipeId = id;
    swipe = createToastSwipe(e.clientX, e.timeStamp);
    // Pause the auto-dismiss while the user is interacting.
    pauseToast(id);
    (e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId);
  }

  function onPointerMove(e: PointerEvent, id: number) {
    if (swipeId !== id || !swipe) return;
    swipe = moveToastSwipe(swipe, e.clientX, e.timeStamp);
  }

  function endSwipe(id: number) {
    if (swipeId !== id || !swipe) return;
    const shouldDismiss = toastSwipeShouldDismiss(swipe);
    swipeId = null;
    swipe = null;
    if (shouldDismiss) dismiss(id);
    else resumeToast(id);
  }

  function onPointerUp(e: PointerEvent, id: number) {
    endSwipe(id);
  }

  // Live transform/opacity for the row currently being dragged.
  const swipeDx = $derived(swipe ? swipe.dx : 0);
  function rowStyle(id: number): string {
    if (swipeId !== id || !swipe) return "";
    return `transform: translateX(${swipeDx}px); opacity: ${toastSwipeOpacity(swipeDx)};`;
  }

  // Keyboard focus + dismiss (slice 4). Alt+T jumps focus to the newest
  // visible toast so a keyboard / screen-reader user can reach the stack
  // without a mouse. While a toast row holds focus, Escape dismisses it
  // (focus then slides to a sibling) and Enter/Space fires its action.
  let toastEls = $state<Record<number, HTMLElement | null>>({});

  function focusToastAt(index: number) {
    if (index < 0) return;
    const list = view.rendered;
    const t = list[index];
    if (!t) return;
    toastEls[t.id]?.focus();
  }

  async function onGlobalKeydown(e: KeyboardEvent) {
    if (!resolveToastFocusHotkey(e)) return;
    if (view.rendered.length === 0) return;
    e.preventDefault();
    await tick();
    focusToastAt(newestToastFocusIndex(view.rendered.length));
  }

  async function onToastKeydown(e: KeyboardEvent, id: number, index: number) {
    // Only handle keys when the toast ROW itself holds focus; if a child
    // button (action / close) is focused, let its own handler win so we
    // don't double-fire on Enter/Space.
    if (e.target !== e.currentTarget) return;
    const t = view.rendered.find((x) => x.id === id);
    const intent = resolveFocusedToastKey(e, t ? hasToastAction(t) : false);
    if (intent === "none") return;
    e.preventDefault();
    if (intent === "action") {
      runToastAction(id);
      return;
    }
    // dismiss + move focus to the sibling that takes the freed slot.
    const remaining = view.rendered.length - 1;
    dismiss(id);
    await tick();
    focusToastAt(pickToastFocusIndex(remaining, index));
  }

  onMount(() => {
    window.addEventListener("keydown", onGlobalKeydown);
    return () => window.removeEventListener("keydown", onGlobalKeydown);
  });
</script>

<div class="stack" role="region" aria-label="Notifications">
  {#if showClearAll}
    <div class="clear-all-row">
      <button class="clear-all" onclick={() => dismissAll()}>{clearAllCopy}</button>
    </div>
  {/if}
  {#if view.showToggle}
    <div class="overflow-row">
      <button
        class="overflow-toggle"
        class:expanded={view.expanded}
        onclick={() => (expanded = !expanded)}
        aria-expanded={view.expanded}
        aria-label={overflowAria}
      >{view.toggleLabel}</button>
    </div>
  {/if}
  {#each view.rendered as t, i (t.id)}
    <div
      class="toast {t.kind}"
      class:swiping={swipeId === t.id}
      role="group"
      aria-label="Notification"
      tabindex="-1"
      bind:this={toastEls[t.id]}
      transition:fly={{ x: 20, duration: 180 }}
      style={rowStyle(t.id)}
      onmouseenter={() => pauseToast(t.id)}
      onmouseleave={() => resumeToast(t.id)}
      onfocusin={() => pauseToast(t.id)}
      onfocusout={() => resumeToast(t.id)}
      onkeydown={(e) => onToastKeydown(e, t.id, i)}
      onpointerdown={(e) => onPointerDown(e, t.id)}
      onpointermove={(e) => onPointerMove(e, t.id)}
      onpointerup={(e) => onPointerUp(e, t.id)}
      onpointercancel={() => endSwipe(t.id)}
    >
      {#if t.loading}
        <span class="spinner" aria-hidden="true"></span>
      {:else}
        <span class="icon" aria-hidden="true">{icon(t.kind)}</span>
      {/if}
      <div class="body" aria-hidden="true">
        <div class="msg">
          <span class="msg-text">{t.message}</span>
          {#if describeToastCount(t.count)}
            <span class="count" aria-label="repeated {t.count} times">{describeToastCount(t.count)}</span>
          {/if}
        </div>
        {#if t.detail}<div class="detail">{t.detail}</div>{/if}
      </div>
      {#if normalizeToastAction(t.action)}
        <button
          class="action"
          onclick={() => runToastAction(t.id)}
          aria-label="{normalizeToastAction(t.action)?.label}: {t.message}"
        >{normalizeToastAction(t.action)?.label}</button>
      {/if}
      <button class="close" onclick={() => dismiss(t.id)} aria-label="Dismiss notification: {t.message}">×</button>
      {#if t.duration > 0}
        <!-- Lifespan bar: depletes over t.duration; pauses with the JS
             timer on hover/focus via animation-play-state. Keyed on
             count so a coalesced repeat restarts the sweep. -->
        {#key t.count}
          <span
            class="lifespan"
            style="animation-duration: {t.duration}ms"
            aria-hidden="true"
          ></span>
        {/key}
      {/if}
    </div>
  {/each}
</div>

<!-- Dedicated screen-reader announcers (slice 5). The visual stack
     above is aria-hidden at the content level (its body text is
     decorative-for-SR because it reorders/coalesces); these two regions
     carry the spoken text instead. Errors/warnings go assertive
     (interrupt), success/info polite. Each toast announced once. -->
<div class="sr-only" role="alert" aria-live="assertive" aria-atomic="false">
  {#each announce.assertive as t (t.id)}
    {#key t.count}
      <p>{announceToast(t.kind, t.message, t.detail, t.count)}</p>
    {/key}
  {/each}
</div>
<div class="sr-only" role="status" aria-live="polite" aria-atomic="false">
  {#each announce.polite as t (t.id)}
    {#key t.count}
      <p>{announceToast(t.kind, t.message, t.detail, t.count)}</p>
    {/key}
  {/each}
</div>

<style>
  .stack {
    position: fixed;
    bottom: 16px;
    right: 16px;
    z-index: 200;
    display: flex;
    flex-direction: column;
    gap: 8px;
    pointer-events: none;
    max-width: min(380px, calc(100vw - 32px));
  }
  .overflow-row {
    pointer-events: auto;
    align-self: flex-end;
    display: flex;
  }
  .overflow-toggle {
    pointer-events: auto;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.02em;
    color: var(--text-3);
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: 999px;
    padding: 3px 11px;
    cursor: pointer;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.25);
    transition:
      color 120ms ease,
      border-color 120ms ease;
  }
  .overflow-toggle:hover {
    color: var(--text);
    border-color: var(--text-3);
  }
  .overflow-toggle.expanded {
    color: var(--text);
  }
  .overflow-toggle:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .clear-all-row {
    pointer-events: auto;
    align-self: flex-end;
    display: flex;
  }
  .clear-all {
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.02em;
    color: var(--text-3);
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: 999px;
    padding: 3px 11px;
    cursor: pointer;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.25);
    transition:
      color 120ms ease,
      border-color 120ms ease;
  }
  .clear-all:hover {
    color: var(--text);
    border-color: var(--text-3);
  }
  .clear-all:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .toast {
    pointer-events: auto;
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 10px 12px;
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-left: 3px solid var(--accent);
    border-radius: var(--r-md);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.35);
    color: var(--text);
    font-size: 13px;
    min-width: 260px;
    position: relative;
    overflow: hidden;
    touch-action: pan-y;
    cursor: grab;
    /* Snap-back glide when a sub-threshold swipe is released. Suppressed
       mid-drag (.swiping) so the row tracks the pointer 1:1. */
    transition:
      transform 200ms cubic-bezier(0.22, 1, 0.36, 1),
      opacity 200ms ease;
  }
  .toast.swiping {
    cursor: grabbing;
    transition: none;
    user-select: none;
  }
  /* Keyboard focus ring (slice 4): the row is focusable via Alt+T so it
     needs a visible focus affordance distinct from the action/close
     button rings. */
  .toast:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  @media (prefers-reduced-motion: reduce) {
    .toast {
      transition: none;
    }
  }
  .toast.success {
    border-left-color: #3fc88c;
  }
  .toast.error {
    border-left-color: #ff5d6c;
  }
  .toast.warning {
    border-left-color: #ffb648;
  }
  .toast.info {
    border-left-color: var(--accent);
  }
  .icon {
    flex-shrink: 0;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-size: 11px;
    font-weight: 700;
    background: var(--bg-3);
    color: var(--text);
    margin-top: 1px;
  }
  /* Loading spinner (slice 3): same footprint as .icon so the row doesn't
     reflow when a promise toast morphs spinner -> check/cross. */
  .spinner {
    flex-shrink: 0;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    margin-top: 1px;
    box-sizing: border-box;
    border: 2px solid var(--bg-3);
    border-top-color: var(--accent);
    animation: toast-spin 0.7s linear infinite;
  }
  @keyframes toast-spin {
    to {
      transform: rotate(360deg);
    }
  }
  /* Reduced motion: a spinning ring is exactly the kind of continuous
     motion the setting targets. Show a static dashed ring instead. */
  @media (prefers-reduced-motion: reduce) {
    .spinner {
      animation: none;
      border-style: dashed;
      border-top-color: var(--accent);
    }
  }
  .toast.success .icon {
    background: rgba(63, 200, 140, 0.18);
    color: #3fc88c;
  }
  .toast.error .icon {
    background: rgba(255, 93, 108, 0.18);
    color: #ff5d6c;
  }
  .toast.warning .icon {
    background: rgba(255, 182, 72, 0.18);
    color: #ffb648;
  }
  .body {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .msg {
    color: var(--text);
    line-height: 1.35;
    word-wrap: break-word;
    display: flex;
    align-items: baseline;
    gap: 6px;
  }
  .msg-text {
    min-width: 0;
    word-wrap: break-word;
  }
  .count {
    flex-shrink: 0;
    font-size: 10px;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
    line-height: 1;
    padding: 2px 5px;
    border-radius: 999px;
    background: var(--bg-3);
    color: var(--text-3);
    align-self: center;
  }
  .toast.success .count {
    background: rgba(63, 200, 140, 0.18);
    color: #3fc88c;
  }
  .toast.error .count {
    background: rgba(255, 93, 108, 0.18);
    color: #ff5d6c;
  }
  .toast.warning .count {
    background: rgba(255, 182, 72, 0.18);
    color: #ffb648;
  }
  .detail {
    font-size: 11px;
    color: var(--text-3);
    line-height: 1.4;
    word-wrap: break-word;
  }
  .close {
    background: transparent;
    border: 0;
    color: var(--text-3);
    font-size: 16px;
    line-height: 1;
    cursor: pointer;
    padding: 0 4px;
    border-radius: 3px;
  }
  .close:hover {
    color: var(--text);
    background: var(--bg-3);
  }
  .action {
    flex-shrink: 0;
    align-self: center;
    font-size: 12px;
    font-weight: 600;
    line-height: 1;
    color: var(--text);
    background: var(--bg-3);
    border: 1px solid var(--border);
    border-radius: var(--r-sm, 5px);
    padding: 5px 10px;
    cursor: pointer;
    white-space: nowrap;
    transition:
      background 120ms ease,
      border-color 120ms ease,
      color 120ms ease;
  }
  .action:hover {
    background: var(--bg-1);
    border-color: var(--text-3);
  }
  .action:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
  /* The action adopts the toast's severity accent so an "Undo" on a
     success reads green, a "Retry" on an error reads red — matching the
     left border + icon vocabulary. */
  .toast.success .action {
    color: #3fc88c;
    border-color: rgba(63, 200, 140, 0.4);
  }
  .toast.success .action:hover {
    background: rgba(63, 200, 140, 0.14);
  }
  .toast.error .action {
    color: #ff5d6c;
    border-color: rgba(255, 93, 108, 0.4);
  }
  .toast.error .action:hover {
    background: rgba(255, 93, 108, 0.14);
  }
  .toast.warning .action {
    color: #ffb648;
    border-color: rgba(255, 182, 72, 0.4);
  }
  .toast.warning .action:hover {
    background: rgba(255, 182, 72, 0.14);
  }
  .lifespan {
    position: absolute;
    left: 0;
    bottom: 0;
    height: 2px;
    width: 100%;
    transform-origin: left center;
    background: var(--accent);
    opacity: 0.55;
    animation-name: lifespan-deplete;
    animation-timing-function: linear;
    animation-fill-mode: forwards;
    animation-iteration-count: 1;
  }
  /* Pause the sweep while the pointer/focus is on the toast — mirrors the
     JS timer pause in notify.ts so the bar and the real clock stay in
     lockstep. */
  .toast:hover .lifespan,
  .toast:focus-within .lifespan {
    animation-play-state: paused;
  }
  .toast.success .lifespan {
    background: #3fc88c;
  }
  .toast.error .lifespan {
    background: #ff5d6c;
  }
  .toast.warning .lifespan {
    background: #ffb648;
  }
  @keyframes lifespan-deplete {
    from {
      transform: scaleX(1);
    }
    to {
      transform: scaleX(0);
    }
  }
  /* Reduced motion: a depleting bar is informative, but the continuous
     scaleX sweep is the kind of motion the setting targets. Keep the bar
     visible (state) but freeze it static rather than animating. */
  @media (prefers-reduced-motion: reduce) {
    .lifespan {
      animation: none;
      transform: scaleX(1);
      opacity: 0.3;
    }
  }
</style>
