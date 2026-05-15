<script lang="ts">
  import MergePanel from "$lib/panels/MergePanel.svelte";
  import SplitPanel from "$lib/panels/SplitPanel.svelte";
  import PagesPanel from "$lib/panels/PagesPanel.svelte";
  import CompressPanel from "$lib/panels/CompressPanel.svelte";
  import ExtractPanel from "$lib/panels/ExtractPanel.svelte";
  import EncryptPanel from "$lib/panels/EncryptPanel.svelte";
  import WatermarkPanel from "$lib/panels/WatermarkPanel.svelte";

  type Feature = {
    id: string;
    label: string;
    icon: string;
    ready: boolean;
  };

  const features: Feature[] = [
    { id: "merge", label: "Merge", icon: "⧉", ready: true },
    { id: "split", label: "Split", icon: "⎯", ready: true },
    { id: "pages", label: "Pages", icon: "▦", ready: true },
    { id: "compress", label: "Compress", icon: "▼", ready: true },
    { id: "extract", label: "Extract", icon: "❡", ready: true },
    { id: "encrypt", label: "Encrypt", icon: "▣", ready: true },
    { id: "watermark", label: "Watermark", icon: "○", ready: true },
    { id: "ocr", label: "OCR", icon: "✦", ready: false },
    { id: "convert", label: "Convert", icon: "↔", ready: false },
    { id: "sign", label: "Sign", icon: "✍", ready: false },
  ];

  let active = $state("merge");
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

  <div class="footer">
    <span class="version">v0.1.0</span>
  </div>
</aside>

<main class="content">
  {#if active === "merge"}
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
  {/if}
</main>

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
    overflow-y: auto;
    padding: 28px 36px 36px;
  }
</style>
