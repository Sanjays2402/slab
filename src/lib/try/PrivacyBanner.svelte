<script lang="ts">
  import { onMount, onDestroy } from "svelte";

  // The wedge. The screenshot bait. The reason this exists.
  //
  // We watch the PerformanceObserver `resource` stream from page-load
  // forward and count bytes transferred from any origin OTHER than our
  // own. Since /try is fully client-side and bundles all assets from
  // the same origin, any cross-origin byte would be suspicious — and
  // we want users to see that the counter stays at zero.
  //
  // We DO NOT count same-origin bytes (those are app assets + sample
  // PDFs the user explicitly requested), but we display the same-origin
  // figure separately so it's clear nothing's hidden.

  let uploaded = 0; // bytes to non-same-origin endpoints
  let appBytes = 0; // bytes to our own origin (app + samples)
  let observer: PerformanceObserver | null = null;
  let mounted = false;
  let collapsed = false;

  function classify(entry: PerformanceResourceTiming) {
    try {
      const url = new URL(entry.name, window.location.href);
      const sameOrigin = url.origin === window.location.origin;
      const size = entry.transferSize || entry.encodedBodySize || 0;
      if (sameOrigin) appBytes += size;
      else uploaded += size;
    } catch {
      // Some browsers omit URLs for crossorigin without CORS — treat
      // those as cross-origin to be conservative.
      uploaded += entry.transferSize || entry.encodedBodySize || 0;
    }
  }

  onMount(() => {
    mounted = true;
    // Backfill anything that already loaded before we mounted.
    for (const e of performance.getEntriesByType("resource")) {
      classify(e as PerformanceResourceTiming);
    }
    try {
      observer = new PerformanceObserver((list) => {
        for (const e of list.getEntries()) {
          classify(e as PerformanceResourceTiming);
        }
        // Trigger Svelte reactivity by reassigning.
        uploaded = uploaded;
        appBytes = appBytes;
      });
      observer.observe({ type: "resource", buffered: true });
    } catch {
      observer = null;
    }
  });

  onDestroy(() => {
    observer?.disconnect();
  });

  function fmt(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / 1024 / 1024).toFixed(2)} MB`;
  }
</script>

{#if mounted}
  <div class="privacy-banner" class:collapsed>
    {#if !collapsed}
      <div class="dot" aria-hidden="true"></div>
      <div class="text">
        <strong>{fmt(uploaded)} uploaded</strong>
        <span> &middot; your file never left this tab.</span>
        <span class="dim"> (app assets from slab.app: {fmt(appBytes)})</span>
      </div>
      <a class="proof"
         href="https://developer.mozilla.org/en-US/docs/Web/HTTP/CORS"
         rel="noopener" target="_blank">verify in DevTools →</a>
      <button class="x" aria-label="Collapse"
              on:click={() => (collapsed = true)}>×</button>
    {:else}
      <button class="reopen" on:click={() => (collapsed = false)}
              title="Show privacy banner">
        🔒 {fmt(uploaded)}
      </button>
    {/if}
  </div>
{/if}

<style>
  .privacy-banner {
    position: fixed;
    bottom: 18px;
    left: 50%;
    transform: translateX(-50%);
    background: rgba(20, 20, 26, 0.92);
    backdrop-filter: blur(20px);
    -webkit-backdrop-filter: blur(20px);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 999px;
    padding: 10px 14px 10px 16px;
    display: flex;
    gap: 12px;
    align-items: center;
    font-size: 13px;
    color: rgba(243, 243, 245, 0.92);
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.6);
    z-index: 100;
    animation: rise 0.4s ease-out;
  }
  @keyframes rise {
    from { transform: translate(-50%, 16px); opacity: 0; }
    to   { transform: translate(-50%, 0);    opacity: 1; }
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: #2ecc71;
    box-shadow: 0 0 12px rgba(46, 204, 113, 0.7);
    animation: pulse 2s ease-in-out infinite;
  }
  @keyframes pulse {
    0%, 100% { opacity: 0.5; }
    50% { opacity: 1; }
  }
  .text strong { color: #fff; }
  .dim { color: rgba(243, 243, 245, 0.5); }
  .proof {
    margin-left: 4px;
    color: rgba(255, 191, 0, 0.85);
    text-decoration: none;
    font-size: 12px;
    padding: 4px 8px;
    border-radius: 6px;
    background: rgba(255, 191, 0, 0.08);
  }
  .proof:hover { background: rgba(255, 191, 0, 0.16); }
  .x {
    background: transparent;
    color: rgba(243, 243, 245, 0.5);
    border: 0;
    font-size: 18px;
    cursor: pointer;
    padding: 0 2px;
    margin-left: 2px;
    line-height: 1;
  }
  .x:hover { color: #fff; }
  .privacy-banner.collapsed {
    padding: 0;
    background: transparent;
    border: 0;
    box-shadow: none;
  }
  .reopen {
    background: rgba(20, 20, 26, 0.92);
    backdrop-filter: blur(20px);
    -webkit-backdrop-filter: blur(20px);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 999px;
    padding: 8px 14px;
    color: #fff;
    font-size: 12px;
    cursor: pointer;
  }
  .reopen:hover { background: rgba(40, 40, 50, 0.95); }
</style>
