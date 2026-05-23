<script lang="ts">
  // PressPanel — v3.8.0 Press surface.
  //
  // Drives `slab_press_convert` (src-tauri/src/lib.rs:1066). Three tabs:
  //
  //   1. Inspect  — pick a PDF, see file selected + pipeline preview.
  //   2. Convert  — pick intent (FOGRA51 / GRACoL2013), toggle 3mm bleed,
  //                 optional title, fire the conversion. On success the
  //                 magenta press-roller wipe sweeps across the report
  //                 card and reveals the "PDF/X-4 ✓ <intent>" badge.
  //   3. Validate — stub. The Slice 5 PDF/X-4 conformance grader ships
  //                 in v3.8.1.
  //
  // Buy-button frame: Adobe Acrobat Pro is the only mainstream tool that
  // does PDF/X-4 export at click level; it costs $239/yr and ships your
  // file to their cloud for any "service" pipeline. Slab does it offline,
  // free, cross-platform, in one click.

  import { invoke } from "@tauri-apps/api/core";
  import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
  import { onMount, onDestroy } from "svelte";
  import { isInTauri } from "$lib/tauri";
  import { matches, prettyBindingFor } from "$lib/keymap";

  type Intent = "fogra51" | "gracol2013";
  type Tab = "inspect" | "convert" | "validate";

  type CmdResult<T> =
    | { kind: "ok"; value: T }
    | { kind: "err"; message: string };

  type PressConvertReport = {
    output_path: string;
    elapsed_ms: number;
    fonts_embedded: number;
    javascript_stripped: number;
    annotations_sanitized: number;
    color_pages_touched: number;
    color_default_entries_added: number;
    trimbox_synthesized: number;
    trimbox_preserved: number;
    bleed_added: number;
    intent_label: string;
  };

  const INTENT_INFO: Record<Intent, { label: string; sub: string }> = {
    fogra51: {
      label: "FOGRA51",
      sub: "PSO Coated v3 — European coated offset (ISO 12647-2)",
    },
    gracol2013: {
      label: "GRACoL2013",
      sub: "CRPC6 — North American sheet-fed coated",
    },
  };

  let tab: Tab = $state("inspect");
  let inputPath = $state("");
  let outputPath = $state("");
  let intent: Intent = $state("fogra51");
  let addBleed = $state(false);
  let title = $state("");

  let busy = $state(false);
  let error = $state("");
  let report: PressConvertReport | null = $state(null);
  let badgeRevealed = $state(false);
  let prefersReducedMotion = $state(false);

  let mediaQuery: MediaQueryList | null = null;
  function syncMotion() {
    if (mediaQuery) prefersReducedMotion = mediaQuery.matches;
  }

  onMount(() => {
    if (typeof window !== "undefined" && "matchMedia" in window) {
      mediaQuery = window.matchMedia("(prefers-reduced-motion: reduce)");
      syncMotion();
      mediaQuery.addEventListener("change", syncMotion);
    }
    window.addEventListener("keydown", onKeydown);
  });
  onDestroy(() => {
    if (mediaQuery) mediaQuery.removeEventListener("change", syncMotion);
    if (typeof window !== "undefined") {
      window.removeEventListener("keydown", onKeydown);
    }
  });

  function onKeydown(ev: KeyboardEvent) {
    // When user re-presses the panel shortcut while already on the panel,
    // jump to the Convert tab. Global activation is owned by +page.svelte.
    if (matches(ev, "press.open")) {
      ev.preventDefault();
      tab = "convert";
    }
  }

  async function pickInput() {
    if (!isInTauri()) {
      error = "File picker requires the desktop app.";
      return;
    }
    const sel = await openDialog({
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof sel === "string") {
      inputPath = sel;
      report = null;
      badgeRevealed = false;
      // Default output: foo.pdf -> foo-pdfx4.pdf
      outputPath = inputPath.replace(/\.pdf$/i, "") + "-pdfx4.pdf";
    }
  }

  async function pickOutput() {
    if (!isInTauri()) return;
    const sel = await saveDialog({
      defaultPath: outputPath || "output-pdfx4.pdf",
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof sel === "string") outputPath = sel;
  }

  async function runConvert() {
    if (!inputPath || !outputPath) {
      error = "Pick an input PDF and an output destination first.";
      return;
    }
    error = "";
    report = null;
    badgeRevealed = false;
    busy = true;
    try {
      const res = (await invoke("slab_press_convert", {
        input: inputPath,
        output: outputPath,
        intent,
        addBleed,
        title: title.trim() ? title.trim() : null,
      })) as CmdResult<PressConvertReport>;
      if (res.kind === "ok") {
        report = res.value;
        // Trigger the magenta wipe + badge reveal. Reduced motion users
        // get an instant fade — still readable, no surprise motion.
        const delay = prefersReducedMotion ? 0 : 380;
        setTimeout(() => {
          badgeRevealed = true;
        }, delay);
      } else {
        error = res.message;
      }
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  function formatMs(ms: number): string {
    if (ms < 1000) return `${ms} ms`;
    return `${(ms / 1000).toFixed(2)} s`;
  }
</script>

<div class="press-panel">
  <header>
    <h1>Press</h1>
    <p class="tagline">
      One-click PDF/X-4 conversion (ISO 15930-7). Print-ready, offline,
      free. Adobe Acrobat Pro charges $239/yr for the same thing — and
      sends your file to their cloud.
    </p>
    <div class="tabs" role="tablist">
      <button
        role="tab"
        class:active={tab === "inspect"}
        on:click={() => (tab = "inspect")}
      >Inspect</button>
      <button
        role="tab"
        class:active={tab === "convert"}
        on:click={() => (tab = "convert")}
      >Convert <span class="kbd">{prettyBindingFor("press.open") || "⌘⇧X"}</span></button>
      <button
        role="tab"
        class:active={tab === "validate"}
        on:click={() => (tab = "validate")}
      >Validate</button>
    </div>
  </header>

  {#if tab === "inspect"}
    <section class="card">
      <h2>Inspect input</h2>
      <div class="row">
        <button class="primary" on:click={pickInput}>Pick PDF…</button>
        <span class="path">{inputPath || "No file selected."}</span>
      </div>
      {#if inputPath}
        <p class="hint">
          Ready. Switch to <strong>Convert</strong> to choose an output
          intent and write a PDF/X-4 file.
        </p>
      {:else}
        <div class="empty">
          <p>
            Pick a PDF to inspect. The Convert tab will then handle the
            full PDF/X-4 pipeline:
          </p>
          <ol>
            <li>Sanitize (strip JavaScript, OpenAction, encryption).</li>
            <li>Embed missing Standard-14 fonts.</li>
            <li>Install ICC default colour spaces.</li>
            <li>Synthesize TrimBox + optional 3 mm bleed.</li>
            <li>Write PDF/X-4 XMP packet (pdfxid namespace).</li>
            <li>Add /Catalog /OutputIntents with vendored ICC profile.</li>
          </ol>
        </div>
      {/if}
    </section>
  {:else if tab === "convert"}
    <section class="card">
      <h2>Convert to PDF/X-4</h2>

      <div class="row">
        <button on:click={pickInput}>Input…</button>
        <span class="path">{inputPath || "—"}</span>
      </div>
      <div class="row">
        <button on:click={pickOutput}>Output…</button>
        <span class="path">{outputPath || "—"}</span>
      </div>

      <fieldset>
        <legend>Output intent</legend>
        {#each (["fogra51", "gracol2013"] as Intent[]) as opt (opt)}
          <label class="intent-opt" class:selected={intent === opt}>
            <input type="radio" name="intent" value={opt} bind:group={intent} />
            <span class="label">{INTENT_INFO[opt].label}</span>
            <span class="sub">{INTENT_INFO[opt].sub}</span>
          </label>
        {/each}
      </fieldset>

      <label class="bleed">
        <input type="checkbox" bind:checked={addBleed} />
        Add 3 mm BleedBox (recommended for offset printing)
      </label>

      <label class="title-input">
        <span>Document title (optional, written to XMP <code>dc:title</code>)</span>
        <input type="text" bind:value={title} placeholder="e.g. Spring 2026 Catalogue" />
      </label>

      <div class="actions">
        <button class="primary" on:click={runConvert} disabled={busy || !inputPath || !outputPath}>
          {busy ? "Converting…" : "Convert"}
        </button>
        {#if error}
          <p class="error">{error}</p>
        {/if}
      </div>

      {#if report}
        <div
          class="report"
          class:reveal={badgeRevealed}
          class:reduced={prefersReducedMotion}
        >
          <div class="wipe" aria-hidden="true">
            <span class="ink c"></span>
            <span class="ink m"></span>
            <span class="ink y"></span>
            <span class="ink k"></span>
          </div>
          <div class="badge">
            <span class="check">✓</span>
            PDF/X-4 · {report.intent_label}
          </div>
          <ul class="stats">
            <li><span>Output</span><code>{report.output_path}</code></li>
            <li><span>Elapsed</span>{formatMs(report.elapsed_ms)}</li>
            <li><span>Fonts embedded</span>{report.fonts_embedded}</li>
            <li><span>JavaScript stripped</span>{report.javascript_stripped}</li>
            <li><span>Annotations sanitized</span>{report.annotations_sanitized}</li>
            <li><span>Pages touched (color)</span>{report.color_pages_touched}</li>
            <li><span>Default colour entries</span>{report.color_default_entries_added}</li>
            <li><span>TrimBox synthesized</span>{report.trimbox_synthesized}</li>
            <li><span>TrimBox preserved</span>{report.trimbox_preserved}</li>
            <li><span>BleedBox added</span>{report.bleed_added}</li>
          </ul>
        </div>
      {/if}
    </section>
  {:else}
    <section class="card stub">
      <h2>Validate</h2>
      <p>
        PDF/X-4 conformance grader lands in <strong>v3.8.1</strong>. It will
        mirror Loom's Validate tab — drop in any PDF and get a per-clause
        pass/fail card backed by ISO 15930-7. For now, conversion writes
        the OutputIntent + XMP atomically, so files produced by the
        Convert tab are conformant by construction.
      </p>
    </section>
  {/if}
</div>

<style>
  .press-panel {
    padding: 24px 28px 80px;
    max-width: 920px;
    margin: 0 auto;
    color: var(--text, #1a1a1a);
  }
  h1 {
    font-size: 22px;
    font-weight: 600;
    letter-spacing: -0.01em;
    margin: 0 0 6px;
  }
  .tagline {
    color: var(--muted, #6a6a6a);
    margin: 0 0 18px;
    font-size: 13px;
    line-height: 1.5;
  }
  .tabs {
    display: flex;
    gap: 4px;
    border-bottom: 1px solid var(--border, #e5e5e5);
    margin-bottom: 20px;
  }
  .tabs button {
    background: transparent;
    border: none;
    padding: 8px 14px;
    font-size: 13px;
    cursor: pointer;
    border-bottom: 2px solid transparent;
    color: var(--muted, #6a6a6a);
  }
  .tabs button.active {
    color: var(--text, #1a1a1a);
    border-bottom-color: #e91e63;
  }
  .kbd {
    margin-left: 6px;
    font-size: 11px;
    color: var(--muted, #888);
    border: 1px solid var(--border, #ddd);
    padding: 1px 5px;
    border-radius: 3px;
  }
  .card {
    background: var(--surface, #fff);
    border: 1px solid var(--border, #e5e5e5);
    border-radius: 10px;
    padding: 20px 22px;
  }
  h2 {
    margin: 0 0 14px;
    font-size: 15px;
    font-weight: 600;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 12px;
    margin: 8px 0;
  }
  .path {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 12px;
    color: var(--muted, #555);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
  }
  button {
    background: var(--surface-2, #f4f4f4);
    border: 1px solid var(--border, #ddd);
    border-radius: 6px;
    padding: 6px 12px;
    font-size: 12px;
    cursor: pointer;
  }
  button.primary {
    background: linear-gradient(180deg, #ec4899, #e91e63);
    color: #fff;
    border-color: #c2185b;
    font-weight: 600;
  }
  button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  fieldset {
    border: 1px solid var(--border, #e5e5e5);
    border-radius: 8px;
    padding: 12px;
    margin: 16px 0;
  }
  legend {
    padding: 0 6px;
    font-size: 12px;
    color: var(--muted, #666);
  }
  .intent-opt {
    display: grid;
    grid-template-columns: 24px auto 1fr;
    gap: 8px;
    align-items: center;
    padding: 8px;
    border-radius: 6px;
    cursor: pointer;
  }
  .intent-opt.selected {
    background: rgba(233, 30, 99, 0.06);
  }
  .intent-opt .label {
    font-weight: 600;
    font-size: 13px;
  }
  .intent-opt .sub {
    color: var(--muted, #666);
    font-size: 12px;
  }
  .bleed,
  .title-input {
    display: flex;
    gap: 8px;
    align-items: center;
    margin: 10px 0;
    font-size: 13px;
  }
  .title-input {
    flex-direction: column;
    align-items: stretch;
  }
  .title-input input {
    padding: 6px 10px;
    border: 1px solid var(--border, #ddd);
    border-radius: 6px;
    font-size: 13px;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 14px;
    margin-top: 18px;
  }
  .error {
    color: #d32f2f;
    font-size: 12px;
    margin: 0;
  }
  .hint,
  .empty {
    font-size: 13px;
    color: var(--muted, #555);
    line-height: 1.5;
  }
  .empty ol {
    margin: 8px 0 0 18px;
    padding: 0;
  }
  .empty li {
    margin: 4px 0;
  }

  /* ── Wow: magenta press-roller wipe + badge reveal ─────────────── */
  .report {
    position: relative;
    margin-top: 22px;
    padding: 22px 18px 14px;
    border: 1px solid var(--border, #e5e5e5);
    border-radius: 10px;
    overflow: hidden;
  }
  .wipe {
    position: absolute;
    inset: 0;
    pointer-events: none;
    z-index: 1;
    display: grid;
    grid-template-rows: 1fr 1fr 1fr 1fr;
    transform: translateX(-100%);
    animation: roll 380ms cubic-bezier(0.65, 0, 0.35, 1) 0ms forwards;
  }
  .ink {
    display: block;
    width: 100%;
    height: 100%;
    opacity: 0.92;
  }
  .ink.c { background: #00b3d9; }
  .ink.m { background: #e91e63; }
  .ink.y { background: #ffd400; }
  .ink.k { background: #1a1a1a; }
  @keyframes roll {
    0%   { transform: translateX(-100%); }
    100% { transform: translateX(120%); }
  }
  .report .badge {
    position: relative;
    z-index: 2;
    display: inline-flex;
    align-items: center;
    gap: 8px;
    font-weight: 700;
    color: #fff;
    background: linear-gradient(135deg, #e91e63, #ad1457);
    padding: 6px 14px;
    border-radius: 999px;
    box-shadow: 0 4px 12px rgba(233, 30, 99, 0.35);
    opacity: 0;
    transform: translateY(6px) scale(0.96);
    transition:
      opacity 220ms ease-out 380ms,
      transform 320ms cubic-bezier(0.2, 0.8, 0.2, 1) 380ms;
  }
  .report .badge .check {
    background: #fff;
    color: #c2185b;
    width: 20px;
    height: 20px;
    border-radius: 50%;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-weight: 800;
    font-size: 13px;
  }
  .report.reveal .badge {
    opacity: 1;
    transform: translateY(0) scale(1);
  }
  .report .stats {
    position: relative;
    z-index: 2;
    list-style: none;
    margin: 16px 0 0;
    padding: 0;
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 6px 18px;
    font-size: 12px;
    color: var(--text, #1a1a1a);
  }
  .report .stats li {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    padding: 4px 0;
    border-bottom: 1px dashed var(--border, #eee);
  }
  .report .stats span {
    color: var(--muted, #666);
  }
  .report .stats code {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 11px;
  }

  /* Reduced-motion override: no wipe, no slide. Badge fades only. */
  .report.reduced .wipe { animation: none; display: none; }
  .report.reduced .badge {
    transition: opacity 160ms linear 0ms;
    transform: none;
  }

  .stub p {
    font-size: 13px;
    line-height: 1.55;
    color: var(--muted, #555);
  }

  /* Dark mode parity */
  :global(.dark) .press-panel {
    color: #ececec;
  }
  :global(.dark) .card,
  :global(.dark) .report {
    background: #1c1c1e;
    border-color: #2c2c2e;
  }
  :global(.dark) .tabs {
    border-bottom-color: #2c2c2e;
  }
  :global(.dark) button {
    background: #2a2a2c;
    border-color: #3a3a3c;
    color: #ececec;
  }
  :global(.dark) .intent-opt.selected {
    background: rgba(233, 30, 99, 0.14);
  }
  :global(.dark) .report .stats li {
    border-bottom-color: #2c2c2e;
  }
</style>
