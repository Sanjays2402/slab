<script lang="ts">
  import { toasts, dismiss, dismissAll, type Toast } from "$lib/notify";
  import {
    partitionToasts,
    describeToastOverflow,
    describeToastCount,
    describeClearAll,
    shouldShowClearAll,
  } from "$lib/toastStack";
  import { fly } from "svelte/transition";

  // Mount once near the root layout. Renders the newest few toasts in the
  // bottom-right corner; a burst beyond the cap collapses the oldest into
  // a "+N more" pill (slice 1) so the stack never fills the viewport.
  // Dismiss on click of × or via the auto-timer set by notify.ts.

  // Newest TOAST_MAX_VISIBLE render; older collapse behind the pill. The
  // pill sits ABOVE the visible toasts (older = higher in the corner).
  const part = $derived(partitionToasts($toasts));
  const overflowCopy = $derived(describeToastOverflow(part.hiddenCount));
  // Clear-all header (slice 3) tops the stack once 2+ toasts are live,
  // wiring the bulk dismissAll the notify store has always exposed.
  const showClearAll = $derived(shouldShowClearAll($toasts.length));
  const clearAllCopy = $derived(describeClearAll($toasts.length));

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
</script>

<div class="stack" role="region" aria-live="polite" aria-label="Notifications">
  {#if showClearAll}
    <div class="clear-all-row">
      <button class="clear-all" onclick={() => dismissAll()}>{clearAllCopy}</button>
    </div>
  {/if}
  {#if overflowCopy}
    <div class="overflow-pill" aria-hidden="true">{overflowCopy}</div>
  {/if}
  {#each part.visible as t (t.id)}
    <div
      class="toast {t.kind}"
      transition:fly={{ x: 20, duration: 180 }}
    >
      <span class="icon">{icon(t.kind)}</span>
      <div class="body">
        <div class="msg">
          <span class="msg-text">{t.message}</span>
          {#if describeToastCount(t.count)}
            <span class="count" aria-label="repeated {t.count} times">{describeToastCount(t.count)}</span>
          {/if}
        </div>
        {#if t.detail}<div class="detail">{t.detail}</div>{/if}
      </div>
      <button class="close" onclick={() => dismiss(t.id)} aria-label="Dismiss">×</button>
    </div>
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
  .overflow-pill {
    pointer-events: auto;
    align-self: flex-end;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.02em;
    color: var(--text-3);
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: 999px;
    padding: 2px 10px;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.25);
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
</style>
