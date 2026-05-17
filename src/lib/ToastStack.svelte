<script lang="ts">
  import { toasts, dismiss, type Toast } from "$lib/notify";
  import { fly } from "svelte/transition";

  // Mount once near the root layout. Stacks up to 5 toasts in the
  // bottom-right corner; older toasts get pushed down. Dismiss on
  // click of × or via the auto-timer set by notify.ts.

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
  {#each $toasts as t (t.id)}
    <div
      class="toast {t.kind}"
      transition:fly={{ x: 20, duration: 180 }}
    >
      <span class="icon">{icon(t.kind)}</span>
      <div class="body">
        <div class="msg">{t.message}</div>
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
