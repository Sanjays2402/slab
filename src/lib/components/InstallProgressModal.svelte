<script lang="ts">
  // InstallProgressModal — v1.4.0 "Bench" Slice 8b.
  //
  // Centered modal shown while a marketplace install is in flight.
  // States, in order:
  //   verifying  → "Verifying signature…"   (animated bar)
  //   downloading → "Downloading…"          (animated bar)
  //   extracting → "Extracting…"            (animated bar)
  //   done       → "Installed {name}"       (green check, auto-dismiss)
  //   error      → "Install failed"         (red, with detail + Close)
  //
  // Phase transitions are driven by the parent (PluginsPanel). For
  // now the timing is a heuristic — the backend doesn't yet emit
  // mid-install progress events. The modal contract is intentionally
  // narrow so swapping in real Tauri events later is a one-liner:
  // bind `phase` to whatever the backend reports.
  //
  // We deliberately do NOT auto-dismiss the error state — the user
  // needs time to read the failure detail and decide whether to
  // retry. Success auto-dismisses (1.8s) since the toast covers the
  // confirmation already.

  import { t, tStore } from "$lib/i18n";
  import type { IndexEntry } from "$lib/marketplace";
  import { formatBytes } from "$lib/marketplace";

  type Phase = "verifying" | "downloading" | "extracting" | "done" | "error";

  type Props = {
    entry: IndexEntry;
    phase: Phase;
    error: string | null;
    onDismiss: () => void;
  };

  let { entry, phase, error, onDismiss }: Props = $props();

  function onKeydown(e: KeyboardEvent) {
    // Only ESC the modal when we're in a terminal state — interrupting
    // an in-flight install through the UI would be a lie (the backend
    // can't be cancelled today), so we lock keyboard dismiss to
    // done/error.
    if (e.key === "Escape" && (phase === "done" || phase === "error")) {
      e.preventDefault();
      onDismiss();
    }
  }

  let phaseLabel = $derived.by(() => {
    switch (phase) {
      case "verifying":
        return t("plugins.install.verifying");
      case "downloading":
        return t("plugins.install.downloading");
      case "extracting":
        return t("plugins.install.extracting");
      case "done":
        return t("plugins.install.done", { name: entry.name });
      case "error":
        return t("plugins.install.failed");
    }
  });
</script>

<svelte:window onkeydown={onKeydown} />

<div
  class="backdrop"
  role="dialog"
  aria-modal="true"
  aria-labelledby="install-title"
  aria-live="polite"
  tabindex="-1"
>
  <div class="modal" role="document">
    <header>
      <div class="icon" class:done={phase === "done"} class:err={phase === "error"} aria-hidden="true">
        {#if phase === "done"}
          ✓
        {:else if phase === "error"}
          ✕
        {:else}
          ⏳
        {/if}
      </div>
      <div class="title-block">
        <h1 id="install-title">{entry.name}</h1>
        <p class="sub">v{entry.version} · {formatBytes(entry.size_bytes)}</p>
      </div>
    </header>

    <div class="phase">
      <p class="phase-label" class:done={phase === "done"} class:err={phase === "error"}>
        {phaseLabel}
      </p>
      {#if phase !== "done" && phase !== "error"}
        <div class="bar" aria-hidden="true">
          <div class="bar-fill"></div>
        </div>
      {/if}
      {#if phase === "error" && error}
        <pre class="err-detail">{error}</pre>
      {/if}
    </div>

    {#if phase === "done" || phase === "error"}
      <div class="actions">
        <button type="button" class="primary" onclick={onDismiss}>
          {$tStore("plugins.detail.close")}
        </button>
      </div>
    {/if}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 120;
    background: rgba(0, 0, 0, 0.55);
    backdrop-filter: blur(2px);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 24px;
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

  .modal {
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: var(--r-md, 10px);
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.4);
    max-width: 420px;
    width: 100%;
    padding: 22px 24px 18px;
    display: flex;
    flex-direction: column;
    gap: 16px;
    animation: modal-in 0.18s ease-out;
  }
  @keyframes modal-in {
    from {
      transform: translateY(6px) scale(0.98);
      opacity: 0;
    }
    to {
      transform: translateY(0) scale(1);
      opacity: 1;
    }
  }

  header {
    display: flex;
    gap: 14px;
    align-items: flex-start;
  }
  .icon {
    width: 36px;
    height: 36px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-size: 18px;
    line-height: 1;
    border-radius: 50%;
    background: var(--bg-3);
    border: 1px solid var(--border);
    color: var(--text-2);
    flex-shrink: 0;
  }
  .icon.done {
    background: color-mix(in oklab, var(--accent) 18%, var(--bg-3));
    border-color: color-mix(in oklab, var(--accent) 50%, var(--border));
    color: color-mix(in oklab, var(--accent) 80%, var(--text));
  }
  .icon.err {
    background: rgba(255, 90, 90, 0.12);
    border-color: rgba(255, 90, 90, 0.4);
    color: var(--danger, #e54);
  }

  .title-block {
    min-width: 0;
    flex: 1;
  }
  .title-block h1 {
    margin: 0;
    font-size: 15.5px;
    font-weight: 600;
    line-height: 1.3;
    color: var(--text);
  }
  .title-block .sub {
    margin: 4px 0 0;
    font-size: 11.5px;
    color: var(--text-3);
    font-family: var(--font-mono);
  }

  .phase {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .phase-label {
    margin: 0;
    font-size: 13px;
    color: var(--text-2);
    line-height: 1.4;
  }
  .phase-label.done {
    color: color-mix(in oklab, var(--accent) 80%, var(--text));
    font-weight: 500;
  }
  .phase-label.err {
    color: var(--danger, #e54);
    font-weight: 500;
  }

  /* Indeterminate progress bar — a 30%-wide chip that slides across
   * the track. CSS-only so we don't spend a JS tick per frame, and
   * `prefers-reduced-motion` users get a static striped bar. */
  .bar {
    position: relative;
    width: 100%;
    height: 4px;
    background: var(--bg-3);
    border: 1px solid var(--border);
    border-radius: 999px;
    overflow: hidden;
  }
  .bar-fill {
    position: absolute;
    top: 0;
    left: 0;
    height: 100%;
    width: 30%;
    background: linear-gradient(
      90deg,
      transparent 0%,
      var(--accent) 30%,
      var(--accent) 70%,
      transparent 100%
    );
    animation: slide 1.3s ease-in-out infinite;
  }
  @keyframes slide {
    0% {
      left: -30%;
    }
    100% {
      left: 100%;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .bar-fill {
      animation: none;
      width: 100%;
      background: color-mix(in oklab, var(--accent) 30%, transparent);
    }
  }

  .err-detail {
    margin: 0;
    padding: 8px 10px;
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-left: 3px solid var(--danger, #e54);
    border-radius: 6px;
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-2);
    white-space: pre-wrap;
    word-break: break-word;
    max-height: 140px;
    overflow-y: auto;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  .actions .primary {
    font: inherit;
    padding: 7px 16px;
    border-radius: 6px;
    cursor: pointer;
    background: var(--accent);
    color: var(--accent-fg, white);
    border: 1px solid var(--accent);
  }
</style>
