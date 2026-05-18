<script lang="ts">
  // UninstallConfirmModal — v1.4.0 "Bench" Slice 9.
  //
  // Destructive-action confirmation for plugin uninstall. Pure
  // presentational component: takes the plugin metadata + busy flag +
  // confirm/cancel callbacks. No knowledge of marketplace internals.
  //
  // Mirrors `DecryptModal.svelte` for visual consistency: centered
  // modal, dark backdrop with blur, Escape/Enter keyboard handling,
  // primary action on the right.

  import { tStore } from "$lib/i18n";

  type Props = {
    /** Display name of the plugin being uninstalled. */
    name: string;
    /** Plugin version string ("1.2.0"). */
    version: string;
    /** Plugin id — also used to render the install path. */
    id: string;
    /** True while the backend uninstall call is in flight. */
    busy: boolean;
    /** User pressed the destructive button. */
    onConfirm: () => void;
    /** Esc / backdrop click / Cancel pressed. */
    onCancel: () => void;
  };

  let { name, version, id, busy, onConfirm, onCancel }: Props = $props();

  let confirmBtn: HTMLButtonElement | undefined = $state();

  // Focus the destructive action when the modal mounts. We intentionally
  // do *not* focus Cancel — accessibility convention for destructive
  // dialogs is to focus the destructive primary so keyboard users can
  // act with Enter, while making the danger visually obvious via color.
  $effect(() => {
    confirmBtn?.focus();
  });

  function onKeydown(e: KeyboardEvent) {
    if (busy) return;
    if (e.key === "Escape") {
      e.preventDefault();
      onCancel();
    } else if (e.key === "Enter") {
      e.preventDefault();
      onConfirm();
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div
  class="backdrop"
  role="dialog"
  aria-modal="true"
  aria-labelledby="uninstall-title"
  onclick={(e) => {
    if (busy) return;
    if (e.target === e.currentTarget) onCancel();
  }}
  onkeydown={(e) => {
    if (busy) return;
    if (e.key === "Escape") onCancel();
  }}
  tabindex="-1"
>
  <div class="modal" role="document">
    <header>
      <div class="warn-icon" aria-hidden="true">⚠️</div>
      <div>
        <h1 id="uninstall-title">
          {$tStore("plugins.uninstallConfirm.title").replace("{name}", name)}
        </h1>
        <p class="subtitle">{$tStore("plugins.uninstallConfirm.body")}</p>
      </div>
    </header>

    <dl class="meta">
      <dt>{$tStore("plugins.uninstallConfirm.version")}</dt>
      <dd class="mono">v{version}</dd>
      <dt>{$tStore("plugins.uninstallConfirm.path")}</dt>
      <dd class="mono break">~/.slab/plugins/{id}/</dd>
    </dl>

    <div class="actions">
      <button type="button" class="ghost" onclick={onCancel} disabled={busy}>
        {$tStore("plugins.uninstallConfirm.cancel")}
      </button>
      <button
        bind:this={confirmBtn}
        type="button"
        class="danger"
        onclick={onConfirm}
        disabled={busy}
      >
        {busy
          ? $tStore("plugins.uninstallConfirm.working")
          : $tStore("plugins.uninstallConfirm.confirm")}
      </button>
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 120;
    background: rgba(0, 0, 0, 0.55);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 24px;
    backdrop-filter: blur(2px);
  }
  .modal {
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: var(--r-md, 10px);
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.4);
    max-width: 440px;
    width: 100%;
    padding: 20px 22px 18px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  header {
    display: flex;
    gap: 12px;
    align-items: flex-start;
  }
  .warn-icon {
    font-size: 28px;
    line-height: 1;
    margin-top: 2px;
  }
  h1 {
    font-size: 16px;
    margin: 0;
    font-weight: 600;
  }
  .subtitle {
    font-size: 13px;
    color: var(--text-3);
    margin: 4px 0 0;
    line-height: 1.4;
  }
  .meta {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 4px 12px;
    margin: 0;
    padding: 10px 12px;
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: var(--r-sm, 6px);
    font-size: 12px;
  }
  .meta dt {
    color: var(--text-3);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    font-size: 10px;
    align-self: center;
  }
  .meta dd {
    margin: 0;
    color: var(--text-1);
  }
  .mono {
    font-family: var(--font-mono, ui-monospace, SFMono-Regular, monospace);
  }
  .break {
    word-break: break-all;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 4px;
  }
  button.danger,
  button.ghost {
    font: inherit;
    padding: 8px 14px;
    border-radius: var(--r-sm, 6px);
    cursor: pointer;
    border: 1px solid var(--border);
  }
  button.danger {
    background: var(--danger, #e54);
    color: var(--accent-fg, white);
    border-color: var(--danger, #e54);
  }
  button.danger:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
  button.ghost {
    background: transparent;
    color: var(--text-2);
  }
  button.ghost:hover:not(:disabled) {
    background: var(--bg-2);
  }
  button.ghost:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>
