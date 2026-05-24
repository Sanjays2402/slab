<script lang="ts">
  import { SAMPLES } from "$lib/try/samples";
  import { goto } from "$app/navigation";

  function pick(slug: string) {
    // Default action: open the page-ops surface (the richest demo).
    goto(`/try/pages?sample=${encodeURIComponent(slug)}`);
  }
</script>

<svelte:head>
  <title>Try Slab in your browser — no upload, no install</title>
  <meta name="description"
        content="The free, offline PDF workstation. Edit a PDF in your browser — your file never leaves this tab." />
</svelte:head>

<section class="hero">
  <h1>Edit a PDF in your browser.<br /><span class="grad">Your file never leaves this tab.</span></h1>
  <p class="lede">
    Smallpdf and iLovePDF upload your documents to their servers.
    Slab runs entirely on your machine — desktop or right here, in this tab.
    Pick a sample below to feel the difference.
  </p>
  <div class="hero-cta">
    <a class="primary" href="/try/pages?sample=multi-chapter-report">Try with a 24-page report →</a>
    <a class="ghost" href="https://github.com/Sanjays2402/slab/releases/latest"
       rel="noopener" target="_blank">Or download Slab (free)</a>
  </div>
</section>

<section class="samples">
  <h2>Pick a sample</h2>
  <div class="grid">
    {#each SAMPLES as sample}
      <button class="card" type="button" on:click={() => pick(sample.slug)}>
        <div class="card-thumb">📄</div>
        <div class="card-body">
          <h3>{sample.label}</h3>
          <p>{sample.description}</p>
          <div class="card-meta">
            <span>{sample.pages} pp</span>
            {#each sample.tags.slice(0, 2) as tag}
              <span class="tag">{tag}</span>
            {/each}
          </div>
        </div>
      </button>
    {/each}
    <label class="card upload">
      <div class="card-thumb">⬆</div>
      <div class="card-body">
        <h3>Or drop your own PDF</h3>
        <p>Stays in this tab. Promise.</p>
      </div>
      <input
        type="file"
        accept="application/pdf"
        hidden
        on:change={(e) => {
          const file = (e.currentTarget as HTMLInputElement).files?.[0];
          if (!file) return;
          // Stash to sessionStorage so /try/pages can pick it up without
          // a server round-trip.
          const reader = new FileReader();
          reader.onload = () => {
            sessionStorage.setItem("try:user-pdf-name", file.name);
            sessionStorage.setItem(
              "try:user-pdf-bytes",
              btoa(
                new Uint8Array(reader.result as ArrayBuffer).reduce(
                  (s, b) => s + String.fromCharCode(b),
                  "",
                ),
              ),
            );
            goto(`/try/pages?source=user`);
          };
          reader.readAsArrayBuffer(file);
        }}
      />
    </label>
  </div>
</section>

<section class="why">
  <h2>Why Slab?</h2>
  <div class="why-grid">
    <div>
      <h4>Zero uploads</h4>
      <p>Every byte is processed in your browser via WebAssembly + <code>pdf-lib</code>. We can't see your file because we never receive it.</p>
    </div>
    <div>
      <h4>Free forever</h4>
      <p>Adobe charges $239/year. PDF Expert charges $79. Slab is free, open source, MIT-licensed.</p>
    </div>
    <div>
      <h4>Same UI on every OS</h4>
      <p>macOS, Windows, Linux — and now in your browser. One workflow, every platform.</p>
    </div>
    <div>
      <h4>AI without the cloud</h4>
      <p>Beacon (the AI) runs on Ollama locally. Your documents never train someone else's model.</p>
    </div>
  </div>
</section>

<style>
  .hero { padding: 48px 0 32px; }
  .hero h1 {
    font-size: clamp(34px, 5vw, 56px);
    line-height: 1.05;
    letter-spacing: -0.025em;
    margin: 0 0 18px;
    font-weight: 700;
  }
  .grad {
    background: linear-gradient(120deg, #ffbf00 20%, #ff8b00 60%, #ff5e6c 95%);
    -webkit-background-clip: text;
    background-clip: text;
    color: transparent;
  }
  .lede {
    font-size: 18px;
    line-height: 1.55;
    color: rgba(243, 243, 245, 0.74);
    max-width: 720px;
    margin: 0 0 28px;
  }
  .hero-cta { display: flex; gap: 12px; flex-wrap: wrap; }
  .primary, .ghost {
    padding: 12px 20px;
    border-radius: 10px;
    text-decoration: none;
    font-weight: 600;
    font-size: 15px;
    transition: transform 0.12s, box-shadow 0.12s, background 0.12s;
  }
  .primary {
    background: linear-gradient(135deg, #ffbf00, #ff8b00);
    color: #1a1a1a;
    box-shadow: 0 6px 24px rgba(255, 140, 0, 0.3);
  }
  .primary:hover { transform: translateY(-1px); box-shadow: 0 10px 28px rgba(255, 140, 0, 0.4); }
  .ghost {
    background: rgba(255, 255, 255, 0.06);
    color: #f3f3f5;
    border: 1px solid rgba(255, 255, 255, 0.12);
  }
  .ghost:hover { background: rgba(255, 255, 255, 0.1); }

  .samples { margin: 48px 0 32px; }
  .samples h2, .why h2 {
    font-size: 14px;
    text-transform: uppercase;
    letter-spacing: 0.12em;
    color: rgba(243, 243, 245, 0.55);
    font-weight: 600;
    margin: 0 0 16px;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
    gap: 16px;
  }
  .card {
    display: flex;
    gap: 16px;
    text-align: left;
    align-items: flex-start;
    background: rgba(255, 255, 255, 0.04);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 14px;
    padding: 18px;
    color: inherit;
    font: inherit;
    cursor: pointer;
    transition: background 0.15s, border-color 0.15s, transform 0.1s;
    position: relative;
  }
  .card:hover {
    background: rgba(255, 255, 255, 0.07);
    border-color: rgba(255, 191, 0, 0.4);
    transform: translateY(-1px);
  }
  .card-thumb {
    font-size: 28px;
    background: rgba(255, 191, 0, 0.12);
    border-radius: 10px;
    padding: 8px 10px;
  }
  .card-body h3 { font-size: 15px; margin: 0 0 4px; font-weight: 600; }
  .card-body p { font-size: 13px; margin: 0; color: rgba(243, 243, 245, 0.6); }
  .card-meta {
    margin-top: 10px;
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
    font-size: 11px;
    color: rgba(243, 243, 245, 0.5);
  }
  .tag {
    background: rgba(255, 255, 255, 0.06);
    padding: 2px 8px;
    border-radius: 999px;
  }
  .upload { border-style: dashed; cursor: pointer; }

  .why { margin: 48px 0; }
  .why-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: 16px;
  }
  .why-grid h4 { margin: 0 0 6px; font-size: 14px; font-weight: 600; }
  .why-grid p { margin: 0; font-size: 13px; line-height: 1.5; color: rgba(243, 243, 245, 0.65); }
  .why-grid code { font-family: ui-monospace, monospace; font-size: 12px; }
</style>
