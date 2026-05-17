<!--
  DetachedShell — chrome for v1.1.0 "Cabinet" floating panel windows.

  When a panel is detached into its own Tauri WebviewWindow, +page.svelte
  wraps the single panel in this component instead of the normal sidebar+
  tabstrip shell. Just a thin titlebar + full-bleed body — the OS window
  chrome handles drag/close/minimize, and Tauri restores native window
  buttons on every platform.
-->
<script lang="ts">
  type Props = {
    panelId: string;
    title: string;
    children?: import("svelte").Snippet;
  };
  let { panelId, title, children }: Props = $props();
</script>

<div class="detached-shell" data-panel-id={panelId}>
  <header class="detached-header">
    <span class="detached-title">{title}</span>
    <span class="detached-badge">Slab</span>
  </header>
  <main class="detached-body">
    {@render children?.()}
  </main>
</div>

<style>
  .detached-shell {
    display: flex;
    flex-direction: column;
    height: 100vh;
    width: 100vw;
    background: var(--bg-0, #0e0f12);
    color: var(--fg-0, #e6e8eb);
    font-family: ui-sans-serif, system-ui, -apple-system, "SF Pro Text", sans-serif;
    overflow: hidden;
  }
  .detached-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 12px;
    border-bottom: 1px solid var(--border, #1c1f24);
    background: var(--bg-1, #14161a);
    height: 32px;
    flex-shrink: 0;
    user-select: none;
    -webkit-user-select: none;
  }
  .detached-title {
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.02em;
  }
  .detached-badge {
    font-size: 10px;
    opacity: 0.5;
    text-transform: uppercase;
    letter-spacing: 0.1em;
  }
  .detached-body {
    flex: 1;
    min-height: 0;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }
  /* Make sure the single hosted panel can fill the body cleanly. */
  .detached-body :global(> *) {
    flex: 1;
    min-height: 0;
  }
</style>
