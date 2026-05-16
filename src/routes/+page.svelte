<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import ReaderPanel from "$lib/panels/ReaderPanel.svelte";
  import MergePanel from "$lib/panels/MergePanel.svelte";
  import SplitPanel from "$lib/panels/SplitPanel.svelte";
  import PagesPanel from "$lib/panels/PagesPanel.svelte";
  import CompressPanel from "$lib/panels/CompressPanel.svelte";
  import ExtractPanel from "$lib/panels/ExtractPanel.svelte";
  import EncryptPanel from "$lib/panels/EncryptPanel.svelte";
  import WatermarkPanel from "$lib/panels/WatermarkPanel.svelte";
  import ConvertPanel from "$lib/panels/ConvertPanel.svelte";
  import MetadataPanel from "$lib/panels/MetadataPanel.svelte";
  import PageNumbersPanel from "$lib/panels/PageNumbersPanel.svelte";
  import SignPanel from "$lib/panels/SignPanel.svelte";
  import CommandPalette from "$lib/CommandPalette.svelte";
  import type { RecentFile } from "$lib/recent";

  type Feature = {
    id: string;
    label: string;
    icon: string;
    ready: boolean;
  };

  const features: Feature[] = [
    { id: "reader", label: "Reader", icon: "▥", ready: true },
    { id: "merge", label: "Merge", icon: "⧉", ready: true },
    { id: "split", label: "Split", icon: "⎯", ready: true },
    { id: "pages", label: "Pages", icon: "▦", ready: true },
    { id: "compress", label: "Compress", icon: "▼", ready: true },
    { id: "extract", label: "Extract", icon: "❡", ready: true },
    { id: "encrypt", label: "Encrypt", icon: "▣", ready: true },
    { id: "watermark", label: "Watermark", icon: "○", ready: true },
    { id: "convert", label: "Convert", icon: "↔", ready: true },
    { id: "metadata", label: "Metadata", icon: "ⓘ", ready: true },
    { id: "numbers", label: "Numbers", icon: "№", ready: true },
    { id: "sign", label: "Sign", icon: "✍", ready: true },
    { id: "ocr", label: "OCR", icon: "✦", ready: false },
  ];

  let active = $state("reader");
  let paletteOpen = $state(false);

  // Pending recent-file open request — Reader panel reads this and reacts.
  // We keep it on window so the ReaderPanel can subscribe without prop drilling.
  function requestOpenRecent(file: RecentFile) {
    active = "reader";
    queueMicrotask(() => {
      window.dispatchEvent(new CustomEvent("slab:open-recent", { detail: file }));
    });
  }

  function onGlobalKey(e: KeyboardEvent) {
    const isMod = e.metaKey || e.ctrlKey;
    if (isMod && e.key.toLowerCase() === "k") {
      e.preventDefault();
      paletteOpen = !paletteOpen;
    }
  }

  onMount(() => {
    window.addEventListener("keydown", onGlobalKey);
  });
  onDestroy(() => {
    window.removeEventListener("keydown", onGlobalKey);
  });
</script>

<aside class="sidebar">
  <div class="brand">
    <span class="logo">▤</span>
    <span class="brand-name">Slab</span>
    <span class="brand-tag">local · offline · free</span>
  </div>

  <nav>
    {#each features as f (f.id)}
      <button
        class="nav-item"
        class:active={active === f.id}
        class:locked={!f.ready}
        disabled={!f.ready}
        onclick={() => (active = f.id)}
      >
        <span class="nav-icon">{f.icon}</span>
        <span class="nav-label">{f.label}</span>
        {#if !f.ready}<span class="badge">soon</span>{/if}
      </button>
    {/each}
  </nav>

  <button class="palette-trigger" onclick={() => (paletteOpen = true)} title="Command palette">
    <span class="pt-icon">⌘</span>
    <span class="pt-label">Jump to anything</span>
    <span class="pt-kbd">⌘K</span>
  </button>

  <div class="footer">
    <span class="version">v0.5.0</span>
  </div>
</aside>

<main class="content">
  {#if active === "reader"}
    <ReaderPanel />
  {:else if active === "merge"}
    <MergePanel />
  {:else if active === "split"}
    <SplitPanel />
  {:else if active === "pages"}
    <PagesPanel />
  {:else if active === "compress"}
    <CompressPanel />
  {:else if active === "extract"}
    <ExtractPanel />
  {:else if active === "encrypt"}
    <EncryptPanel />
  {:else if active === "watermark"}
    <WatermarkPanel />
  {:else if active === "convert"}
    <ConvertPanel />
  {:else if active === "metadata"}
    <MetadataPanel />
  {:else if active === "numbers"}
    <PageNumbersPanel />
  {:else if active === "sign"}
    <SignPanel />
  {/if}
</main>

<CommandPalette
  bind:open={paletteOpen}
  panels={features}
  activePanel={active}
  onClose={() => (paletteOpen = false)}
  onSelectPanel={(id) => {
    active = id;
    paletteOpen = false;
  }}
  onOpenRecent={(file) => {
    paletteOpen = false;
    requestOpenRecent(file);
  }}
/>

<style>
  .sidebar {
    width: var(--sidebar-w);
    background: var(--bg-2);
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    padding: 14px 10px;
    flex-shrink: 0;
  }

  .brand {
    display: flex;
    align-items: baseline;
    gap: 8px;
    padding: 4px 8px 18px;
  }
  .logo {
    color: var(--accent);
    font-size: 18px;
  }
  .brand-name {
    font-weight: 700;
    font-size: 15px;
    letter-spacing: 0.2px;
  }
  .brand-tag {
    font-size: 10px;
    text-transform: uppercase;
    color: var(--text-3);
    letter-spacing: 0.5px;
  }

  nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 1;
    overflow-y: auto;
  }

  .nav-item {
    display: flex;
    align-items: center;
    gap: 10px;
    text-align: left;
    background: transparent;
    border: 1px solid transparent;
    color: var(--text-2);
    padding: 7px 10px;
    border-radius: var(--r-sm);
    font-size: 13px;
  }
  .nav-item:hover:not(:disabled) {
    background: var(--bg-3);
    color: var(--text);
  }
  .nav-item.active {
    background: var(--bg-3);
    color: var(--text);
    border-color: var(--border);
  }
  .nav-item.locked {
    opacity: 0.55;
  }
  .nav-icon {
    width: 18px;
    text-align: center;
    color: var(--accent);
    opacity: 0.9;
  }
  .nav-label {
    flex: 1;
  }
  .badge {
    font-size: 9px;
    text-transform: uppercase;
    color: var(--text-3);
    background: var(--bg);
    padding: 2px 5px;
    border-radius: 4px;
    letter-spacing: 0.5px;
  }

  .palette-trigger {
    display: flex;
    align-items: center;
    gap: 8px;
    background: var(--bg);
    color: var(--text-3);
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    padding: 7px 10px;
    font-size: 12px;
    margin: 10px 0 8px;
  }
  .palette-trigger:hover {
    color: var(--text);
    background: var(--bg-3);
  }
  .pt-icon {
    color: var(--accent);
  }
  .pt-label {
    flex: 1;
    text-align: left;
  }
  .pt-kbd {
    font-size: 10px;
    background: var(--bg-2);
    border: 1px solid var(--border);
    padding: 1px 5px;
    border-radius: 3px;
    letter-spacing: 0.5px;
  }

  .footer {
    padding: 8px 10px;
    border-top: 1px solid var(--border);
    font-size: 11px;
    color: var(--text-3);
  }

  .content {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow-y: hidden;
    padding: 28px 36px 36px;
    min-height: 0;
  }
</style>
