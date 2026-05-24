<script lang="ts">
  import { getWallCopy } from "./wallCopy";

  export let feature: string = "default";
  export let open = false;

  $: copy = getWallCopy(feature);
  $: latestUrl = "https://github.com/Sanjays2402/slab/releases/latest";
  // Direct deep-links so we can pre-fill the right download.
  $: macUrl = latestUrl + "#user-content-macos";
  $: winUrl = latestUrl + "#user-content-windows";
  $: linUrl = latestUrl + "#user-content-linux";

  function close() {
    open = false;
  }
  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") close();
  }
</script>

<svelte:window on:keydown={onKey} />

{#if open}
  <!-- svelte-ignore a11y-click-events-have-key-events -->
  <!-- svelte-ignore a11y-no-static-element-interactions -->
  <div class="scrim" on:click={close}>
    <div class="wall" role="dialog" aria-modal="true"
         aria-labelledby="dw-title"
         on:click|stopPropagation>
      <button class="x" type="button" on:click={close} aria-label="Close">×</button>
      <div class="badge">Desktop only</div>
      <h2 id="dw-title">{copy.headline}</h2>
      <p class="body">{copy.body}</p>

      <div class="cta-row">
        <a class="primary" href={macUrl} rel="noopener" target="_blank">Download for macOS</a>
        <a class="primary" href={winUrl} rel="noopener" target="_blank">Windows</a>
        <a class="primary" href={linUrl} rel="noopener" target="_blank">Linux</a>
      </div>

      <p class="why">{copy.whyNot}</p>

      <div class="meta">
        <span>Free · MIT · open source</span>
        <a href="https://github.com/Sanjays2402/slab" rel="noopener" target="_blank">View source →</a>
      </div>
    </div>
  </div>
{/if}

<style>
  .scrim {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.6);
    backdrop-filter: blur(8px);
    -webkit-backdrop-filter: blur(8px);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 200;
    animation: scrim-in 0.2s ease-out;
  }
  @keyframes scrim-in { from { opacity: 0 } to { opacity: 1 } }

  .wall {
    width: min(520px, calc(100vw - 32px));
    background: linear-gradient(160deg, #1d1d24, #15151c);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 18px;
    padding: 32px;
    color: #f3f3f5;
    box-shadow: 0 40px 100px rgba(0, 0, 0, 0.7);
    position: relative;
    animation: wall-in 0.25s ease-out;
  }
  @keyframes wall-in {
    from { transform: translateY(8px); opacity: 0; }
    to   { transform: translateY(0);   opacity: 1; }
  }
  .x {
    position: absolute;
    top: 16px; right: 16px;
    background: transparent;
    color: rgba(243, 243, 245, 0.5);
    border: 0;
    font-size: 24px;
    cursor: pointer;
    line-height: 1;
  }
  .x:hover { color: #fff; }
  .badge {
    display: inline-block;
    font-size: 11px;
    background: rgba(255, 191, 0, 0.12);
    color: #ffd866;
    padding: 4px 10px;
    border-radius: 999px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    margin-bottom: 12px;
  }
  h2 {
    font-size: 24px;
    line-height: 1.15;
    letter-spacing: -0.02em;
    margin: 0 0 12px;
    font-weight: 700;
  }
  .body {
    font-size: 15px;
    line-height: 1.55;
    color: rgba(243, 243, 245, 0.78);
    margin: 0 0 20px;
  }
  .cta-row { display: flex; gap: 8px; flex-wrap: wrap; margin-bottom: 18px; }
  .primary {
    padding: 10px 14px;
    border-radius: 10px;
    background: linear-gradient(135deg, #ffbf00, #ff8b00);
    color: #1a1a1a;
    text-decoration: none;
    font-weight: 600;
    font-size: 13px;
    box-shadow: 0 4px 14px rgba(255, 140, 0, 0.25);
    transition: transform 0.1s;
  }
  .primary:hover { transform: translateY(-1px); }
  .why {
    font-size: 12px;
    color: rgba(243, 243, 245, 0.55);
    margin: 0 0 18px;
    line-height: 1.5;
  }
  .meta {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-size: 12px;
    color: rgba(243, 243, 245, 0.5);
    padding-top: 14px;
    border-top: 1px solid rgba(255, 255, 255, 0.08);
  }
  .meta a { color: rgba(255, 191, 0, 0.9); text-decoration: none; }
  .meta a:hover { text-decoration: underline; }
</style>
