<script lang="ts">
  // PluginDetailDrawer — v1.4.0 "Bench" Slice 8a.
  //
  // Right-side slide-in drawer triggered by clicking a Browse-tab
  // marketplace card. Surfaces every piece of metadata the user might
  // want before committing to an install: identity, description,
  // version pill, install state, integrity hash, signature, size,
  // compatibility requirement, and the canonical download URL.
  //
  // The drawer is purely presentational — every action (install /
  // update / uninstall) is delegated to the parent via `onAction`.
  // This keeps the wiring (busy flags, refreshPlugins, toasts) where
  // it already lives in PluginsPanel and means the drawer can be
  // dropped into other surfaces (e.g. the future "what's new" flow)
  // without rewiring.
  //
  // ESC + backdrop click close the drawer. The whole element is
  // mounted with `role="dialog"` + `aria-modal="true"` and consumes a
  // svelte:window onkeydown so keyboard users can dismiss without
  // hunting for the close button.

  import type { IndexEntry } from "$lib/marketplace";
  import { formatBytes } from "$lib/marketplace";
  import { t, tStore } from "$lib/i18n";

  type Status = "install" | "installed" | "update";
  type Props = {
    entry: IndexEntry;
    status: Status;
    installedVersion: string | null;
    inFlight: boolean;
    onClose: () => void;
    onAction: () => void;
  };

  let { entry, status, installedVersion, inFlight, onClose, onAction }: Props = $props();

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      onClose();
    }
  }

  /** Collapse a 64-char hex hash or 88-char base64 sig down to head…tail. */
  function shortHash(h: string): string {
    return h.length > 18 ? `${h.slice(0, 9)}…${h.slice(-9)}` : h;
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div
  class="backdrop"
  role="dialog"
  aria-modal="true"
  aria-labelledby="drawer-title"
  onclick={(e) => {
    if (e.target === e.currentTarget) onClose();
  }}
  onkeydown={(e) => {
    if (e.key === "Escape") onClose();
  }}
  tabindex="-1"
>
  <aside class="drawer" role="document">
    <header>
      <div class="title-block">
        <h1 id="drawer-title">{entry.name}</h1>
        <p class="sub">
          {t("plugins.detail.byAuthor", { author: entry.author })}
          · <span class="mono">{entry.id}</span>
        </p>
      </div>
      <button
        type="button"
        class="icon-btn"
        aria-label={t("plugins.detail.close")}
        onclick={onClose}
        disabled={inFlight}>✕</button
      >
    </header>

    <section class="version-row">
      <span class="chip ver-pill">v{entry.version}</span>
      {#if status === "installed" && installedVersion}
        <span class="chip status-installed-pill">{$tStore("plugins.detail.installed")}</span>
      {:else if status === "update" && installedVersion}
        <span class="chip status-update-pill">
          {t("plugins.detail.updateFrom", { from: installedVersion, to: entry.version })}
        </span>
      {/if}
    </section>

    <section class="desc-block">
      <p>{entry.description}</p>
    </section>

    <dl class="meta-grid">
      <dt>{$tStore("plugins.detail.size")}</dt>
      <dd>{formatBytes(entry.size_bytes)}</dd>
      <dt>{$tStore("plugins.detail.compat")}</dt>
      <dd><code>{entry.slab_compat}</code></dd>
      <dt>{$tStore("plugins.detail.sha256")}</dt>
      <dd class="mono" title={entry.sha256}>{shortHash(entry.sha256)}</dd>
      <dt>{$tStore("plugins.detail.signature")}</dt>
      <dd class="mono" title={entry.signature}>{shortHash(entry.signature)}</dd>
      <dt>{$tStore("plugins.detail.downloadUrl")}</dt>
      <dd class="mono break">{entry.download_url}</dd>
    </dl>

    <footer class="drawer-foot">
      <button type="button" class="ghost" onclick={onClose} disabled={inFlight}>
        {$tStore("plugins.detail.close")}
      </button>
      {#if status === "installed"}
        <button type="button" class="ghost danger" onclick={onAction} disabled={inFlight}>
          {inFlight ? $tStore("plugins.browse.uninstalling") : $tStore("plugins.browse.uninstall")}
        </button>
      {:else if status === "update"}
        <button type="button" class="primary" onclick={onAction} disabled={inFlight}>
          {inFlight
            ? $tStore("plugins.browse.installing")
            : t("plugins.browse.update", { version: entry.version })}
        </button>
      {:else}
        <button type="button" class="primary" onclick={onAction} disabled={inFlight}>
          {inFlight ? $tStore("plugins.browse.installing") : $tStore("plugins.browse.install")}
        </button>
      {/if}
    </footer>
  </aside>
</div>

<style>
  /* Backdrop drops a wash over the app so the drawer reads as modal,
   * but leaves space for the slide-in so the user still sees context. */
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 110;
    background: rgba(0, 0, 0, 0.42);
    backdrop-filter: blur(2px);
    display: flex;
    justify-content: flex-end;
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

  .drawer {
    background: var(--bg-1);
    border-left: 1px solid var(--border);
    box-shadow: -8px 0 30px rgba(0, 0, 0, 0.32);
    width: min(420px, 100%);
    height: 100%;
    display: flex;
    flex-direction: column;
    padding: 22px 24px 18px;
    gap: 14px;
    overflow-y: auto;
    animation: drawer-in 0.18s ease-out;
  }
  @keyframes drawer-in {
    from {
      transform: translateX(20px);
      opacity: 0;
    }
    to {
      transform: translateX(0);
      opacity: 1;
    }
  }

  header {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 10px;
    align-items: start;
  }
  .title-block h1 {
    margin: 0;
    font-size: 17px;
    font-weight: 600;
    line-height: 1.3;
    color: var(--text);
  }
  .title-block .sub {
    margin: 4px 0 0;
    color: var(--text-3);
    font-size: 11.5px;
    line-height: 1.4;
  }
  .icon-btn {
    background: transparent;
    border: 1px solid transparent;
    color: var(--text-3);
    width: 28px;
    height: 28px;
    border-radius: 6px;
    cursor: pointer;
    font: inherit;
    font-size: 13px;
    line-height: 1;
  }
  .icon-btn:hover:not(:disabled) {
    background: var(--bg-2);
    border-color: var(--border);
    color: var(--text);
  }

  .version-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }
  .chip {
    display: inline-block;
    font-size: 11px;
    border: 1px solid var(--border);
    background: var(--bg-3);
    color: var(--text-2);
    padding: 2px 8px;
    border-radius: 999px;
    font-family: var(--font-mono);
  }
  .ver-pill {
    font-family: var(--font-mono);
    font-size: 10.5px;
    color: var(--text-2);
  }
  .status-installed-pill {
    color: color-mix(in oklab, var(--accent) 80%, var(--text));
    border-color: color-mix(in oklab, var(--accent) 35%, var(--border));
    background: color-mix(in oklab, var(--accent) 8%, var(--bg-3));
    font-family: inherit;
  }
  .status-update-pill {
    color: var(--text);
    border-color: rgba(255, 180, 60, 0.45);
    background: rgba(255, 180, 60, 0.12);
    font-family: inherit;
  }

  .desc-block p {
    margin: 0;
    color: var(--text);
    font-size: 13px;
    line-height: 1.55;
    white-space: pre-wrap;
  }

  /* Metadata grid: label column kept compact, value column free to
   * wrap. Long hashes/URLs use `break: anywhere` so the layout never
   * pushes past 420px wide. */
  .meta-grid {
    display: grid;
    grid-template-columns: minmax(110px, auto) minmax(0, 1fr);
    column-gap: 14px;
    row-gap: 8px;
    margin: 4px 0 8px;
    font-size: 12px;
    border-top: 1px solid var(--border);
    padding-top: 14px;
  }
  .meta-grid dt {
    color: var(--text-3);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    line-height: 1.6;
  }
  .meta-grid dd {
    margin: 0;
    color: var(--text-2);
    line-height: 1.5;
  }
  .meta-grid dd code {
    background: var(--bg-3);
    border: 1px solid var(--border);
    padding: 1px 5px;
    border-radius: 4px;
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text);
  }
  .meta-grid .mono {
    font-family: var(--font-mono);
    font-size: 11px;
  }
  .meta-grid .break {
    word-break: break-all;
    overflow-wrap: anywhere;
  }

  .drawer-foot {
    margin-top: auto;
    padding-top: 14px;
    border-top: 1px solid var(--border);
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  .drawer-foot button {
    font: inherit;
    padding: 7px 14px;
    border-radius: 6px;
    border: 1px solid var(--border);
    cursor: pointer;
  }
  .drawer-foot .ghost {
    background: transparent;
    color: var(--text-2);
  }
  .drawer-foot .ghost:hover:not(:disabled) {
    background: var(--bg-2);
    color: var(--text);
  }
  .drawer-foot .ghost.danger:hover:not(:disabled) {
    color: var(--danger);
    border-color: rgba(255, 90, 90, 0.45);
  }
  .drawer-foot .primary {
    background: var(--accent);
    color: var(--accent-fg, white);
    border-color: var(--accent);
  }
  .drawer-foot .primary:disabled,
  .drawer-foot .ghost:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
</style>
