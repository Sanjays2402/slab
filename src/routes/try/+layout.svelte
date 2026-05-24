<script lang="ts">
  import { onMount } from "svelte";
  import PrivacyBanner from "$lib/try/PrivacyBanner.svelte";
  import "../../app.css";

  // Strip Tauri-only menus / sidebars; this is the marketing surface.
  // The minimal chrome here is intentional — sample → action → wow,
  // nothing else competing for attention.
  let mounted = false;
  onMount(() => {
    mounted = true;
    // Defensive: if someone deep-links into /try from inside the desktop
    // app, send them to the real Reader instead.
    if (typeof window !== "undefined" && (window as any).__TAURI_INTERNALS__) {
      window.location.replace("/");
    }
  });
</script>

<div class="try-shell">
  <header class="try-nav">
    <a class="brand" href="/try" aria-label="Slab — Try in browser">
      <span class="brand-mark">▰</span>
      <span class="brand-text">Slab</span>
      <span class="brand-pill">try</span>
    </a>
    <nav>
      <a href="/try/pages" data-tour="pages">Pages</a>
      <a href="/try/reader" data-tour="reader">Reader</a>
      <a href="/try/metadata" data-tour="metadata">Metadata</a>
      <a class="cta" href="https://github.com/Sanjays2402/slab/releases/latest"
         rel="noopener" target="_blank">Download Slab →</a>
    </nav>
  </header>

  <main class="try-main">
    {#if mounted}
      <slot />
    {/if}
  </main>

  <PrivacyBanner />
</div>

<style>
  .try-shell {
    min-height: 100vh;
    display: flex;
    flex-direction: column;
    background:
      radial-gradient(1200px 600px at 10% -10%, rgba(255, 191, 0, 0.08), transparent 60%),
      radial-gradient(1000px 500px at 110% 10%, rgba(0, 122, 255, 0.08), transparent 65%),
      #0c0c10;
    color: #f3f3f5;
    font-family: -apple-system, BlinkMacSystemFont, "Inter", "Segoe UI", system-ui, sans-serif;
  }
  .try-nav {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 18px 28px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.08);
    backdrop-filter: blur(20px);
    -webkit-backdrop-filter: blur(20px);
    background: rgba(12, 12, 16, 0.65);
    position: sticky;
    top: 0;
    z-index: 50;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
    text-decoration: none;
    color: inherit;
    font-weight: 600;
    font-size: 16px;
    letter-spacing: -0.01em;
  }
  .brand-mark {
    color: #ffbf00;
    font-size: 18px;
    text-shadow: 0 0 12px rgba(255, 191, 0, 0.4);
  }
  .brand-pill {
    font-size: 11px;
    padding: 2px 8px;
    border-radius: 999px;
    background: rgba(255, 191, 0, 0.16);
    color: #ffd866;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }
  nav {
    display: flex;
    gap: 18px;
    align-items: center;
  }
  nav a {
    color: rgba(243, 243, 245, 0.78);
    text-decoration: none;
    font-size: 14px;
    padding: 6px 10px;
    border-radius: 8px;
    transition: background 0.15s, color 0.15s;
  }
  nav a:hover {
    background: rgba(255, 255, 255, 0.08);
    color: #fff;
  }
  nav a.cta {
    background: linear-gradient(135deg, #ffbf00, #ff8b00);
    color: #1a1a1a;
    font-weight: 600;
    box-shadow: 0 4px 16px rgba(255, 140, 0, 0.3);
  }
  nav a.cta:hover {
    transform: translateY(-1px);
    box-shadow: 0 6px 20px rgba(255, 140, 0, 0.4);
  }
  .try-main {
    flex: 1;
    padding: 32px 28px 80px;
    max-width: 1200px;
    width: 100%;
    margin: 0 auto;
  }
</style>
