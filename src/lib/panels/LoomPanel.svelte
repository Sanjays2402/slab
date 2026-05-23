<script lang="ts">
  // Loom panel — PDF/UA-1 accessibility workbench.
  //
  // Slice 1 surface: the panel exposes three tabs.
  //
  //   1. Layout      — pick a PDF, see per-page text-run + image-bbox
  //                    counts and the distinct font sizes present. This is
  //                    the diagnostic surface that proves the layout
  //                    extractor (src-tauri/src/pdf/loom/layout.rs) actually
  //                    found what's on the page. It's the input every
  //                    downstream slice (segments / classify / tags) reads.
  //
  //   2. Conformance — the live Matterhorn 1.1 registry digest. Procurement
  //                    officers auditing Slab can land here and see exactly
  //                    how many failure conditions Slab automates today —
  //                    the same numbers /accessibility.html cites, sourced
  //                    from the same generated Rust module so the marketing
  //                    page and the product can never drift apart.
  //
  //   3. About       — what Loom does and what's still in progress. Sets
  //                    expectations honestly: Slice 1 ships layout
  //                    extraction; the validate pass and StructTreeRoot
  //                    writer arrive in later v3.1.0 slices.
  //
  // No PDF rewrite happens here yet — Slice 1 is read-only. The "Tag this
  // PDF" CTA is intentionally disabled with a tooltip explaining which
  // slice will enable it.

  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { onMount } from "svelte";
  import { basename, idle, type CmdResult, type Status } from "$lib/types";

  type LoomPageSummary = {
    page_number: number;
    width: number;
    height: number;
    run_count: number;
    image_count: number;
    distinct_font_sizes: number[];
  };

  type LoomLayoutSummary = {
    pages: LoomPageSummary[];
    total_runs: number;
    total_images: number;
  };

  type LoomMatterhornDigest = {
    protocol_version: string;
    applies_to: string;
    registry_total: number;
    full_protocol_total: number;
    auto: number;
    human: number;
    out_of_scope: number;
    auto_share_of_full_protocol: number;
  };

  type Tab = "layout" | "conformance" | "about";
  let tab: Tab = "layout";

  let inputPath = "";
  let summary: LoomLayoutSummary | null = null;
  let digest: LoomMatterhornDigest | null = null;
  let status: Status = idle;

  onMount(() => {
    loadDigest();
    // Optional: prefill from the reader if the user just opened a PDF.
    const onOpen = (e: Event) => {
      const detail = (e as CustomEvent<{ path: string }>).detail;
      if (detail?.path) inputPath = detail.path;
    };
    window.addEventListener("slab:open-recent", onOpen);
    return () => window.removeEventListener("slab:open-recent", onOpen);
  });

  async function loadDigest() {
    try {
      const r = await invoke<CmdResult<LoomMatterhornDigest>>(
        "slab_loom_matterhorn_digest",
      );
      if (r.kind === "ok") digest = r.value;
    } catch {
      // Non-fatal — Conformance tab will just show a hint.
    }
  }

  async function pickPdf() {
    const sel = await open({
      multiple: false,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof sel === "string") {
      inputPath = sel;
      summary = null;
      status = idle;
    }
  }

  async function analyse() {
    if (!inputPath) return;
    status = { kind: "working", msg: "Reading content streams…" };
    summary = null;
    try {
      const r = await invoke<CmdResult<LoomLayoutSummary>>(
        "slab_loom_layout_summary",
        { input: inputPath },
      );
      if (r.kind === "ok") {
        summary = r.value;
        status = {
          kind: "ok",
          msg: `Parsed ${r.value.pages.length} page${r.value.pages.length === 1 ? "" : "s"} — ${r.value.total_runs.toLocaleString()} text runs, ${r.value.total_images.toLocaleString()} images.`,
        };
      } else {
        status = { kind: "err", msg: r.message };
      }
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }

  function autoPct(d: LoomMatterhornDigest): string {
    return `${(d.auto_share_of_full_protocol * 100).toFixed(1)}%`;
  }
</script>

<section class="loom">
  <header class="loom__head">
    <div>
      <h1>Loom <span class="badge">PDF/UA-1</span></h1>
      <p class="sub">
        Accessibility workbench. Slice 1 of v3.1.0 — layout extraction live;
        structure tagging + validation land in subsequent slices.
      </p>
    </div>
  </header>

  <nav class="tabs" role="tablist">
    <button
      role="tab"
      aria-selected={tab === "layout"}
      class:active={tab === "layout"}
      on:click={() => (tab = "layout")}>Layout</button
    >
    <button
      role="tab"
      aria-selected={tab === "conformance"}
      class:active={tab === "conformance"}
      on:click={() => (tab = "conformance")}>Conformance</button
    >
    <button
      role="tab"
      aria-selected={tab === "about"}
      class:active={tab === "about"}
      on:click={() => (tab = "about")}>About</button
    >
  </nav>

  {#if tab === "layout"}
    <div class="layout">
      <div class="picker">
        <button class="primary" on:click={pickPdf}>Pick PDF…</button>
        <div class="path" title={inputPath}>
          {inputPath ? basename(inputPath) : "no file selected"}
        </div>
        <button
          class="primary"
          disabled={!inputPath || status.kind === "working"}
          on:click={analyse}
        >
          {status.kind === "working" ? "Analysing…" : "Analyse layout"}
        </button>
      </div>

      {#if status.kind !== "idle"}
        <p class="status" data-kind={status.kind}>{status.msg}</p>
      {/if}

      {#if summary}
        <div class="totals">
          <div class="totals__item">
            <div class="n">{summary.pages.length.toLocaleString()}</div>
            <div class="k">pages</div>
          </div>
          <div class="totals__item">
            <div class="n">{summary.total_runs.toLocaleString()}</div>
            <div class="k">text runs</div>
          </div>
          <div class="totals__item">
            <div class="n">{summary.total_images.toLocaleString()}</div>
            <div class="k">images</div>
          </div>
        </div>
        <div class="page-table" role="table" aria-label="Per-page layout">
          <div class="row head" role="row">
            <div role="columnheader">Page</div>
            <div role="columnheader">Size (pt)</div>
            <div role="columnheader">Runs</div>
            <div role="columnheader">Images</div>
            <div role="columnheader">Font sizes</div>
          </div>
          {#each summary.pages as p (p.page_number)}
            <div class="row" role="row">
              <div role="cell">#{p.page_number}</div>
              <div role="cell">{p.width.toFixed(0)} × {p.height.toFixed(0)}</div>
              <div role="cell">{p.run_count.toLocaleString()}</div>
              <div role="cell">{p.image_count.toLocaleString()}</div>
              <div role="cell" class="sizes">
                {#if p.distinct_font_sizes.length === 0}
                  <span class="dim">—</span>
                {:else}
                  {#each p.distinct_font_sizes as s}
                    <span class="chip">{s}pt</span>
                  {/each}
                {/if}
              </div>
            </div>
          {/each}
        </div>
      {:else if status.kind === "idle"}
        <div class="empty">
          <h2>Pick a PDF to begin</h2>
          <p>
            Loom parses every content stream and reports the text runs, image
            placements, and font sizes the engine sees on each page. This is
            the input to the structure tagger (Slice 4).
          </p>
        </div>
      {/if}

      <div class="cta-row">
        <button class="cta" disabled title="Tag-this-PDF arrives in v3.1.0 Slice 4 (StructTreeRoot writer)">
          Tag this PDF for accessibility…
        </button>
        <span class="cta-hint">
          Enabled in v3.1.0 Slice 4 — see <a
            href="https://github.com/Sanjays2402/slab"
            target="_blank"
            rel="noreferrer">roadmap</a
          >.
        </span>
      </div>
    </div>
  {:else if tab === "conformance"}
    <div class="conformance">
      {#if digest}
        <h2>{digest.protocol_version} · {digest.applies_to}</h2>
        <p class="sub">
          Live snapshot of the Matterhorn Protocol failure-condition registry
          shipping in this build. Sourced from
          <code>docs/specs/matterhorn-1.1.json</code>, generated into
          <code>src-tauri/src/pdf/loom/matterhorn.rs</code>, audited in CI by
          <code>pnpm loom:codegen:check</code>.
        </p>
        <div class="totals totals--four">
          <div class="totals__item">
            <div class="n">{digest.registry_total}</div>
            <div class="k">in registry</div>
          </div>
          <div class="totals__item">
            <div class="n">{digest.full_protocol_total}</div>
            <div class="k">full protocol</div>
          </div>
          <div class="totals__item">
            <div class="n">{digest.auto}</div>
            <div class="k">auto-decidable</div>
          </div>
          <div class="totals__item">
            <div class="n">{autoPct(digest)}</div>
            <div class="k">auto share</div>
          </div>
        </div>
        <div class="bar" aria-label="Verdict distribution">
          <div
            class="bar__seg bar__seg--auto"
            style="width: {(digest.auto / digest.registry_total) * 100}%"
            title="{digest.auto} auto"
          ></div>
          <div
            class="bar__seg bar__seg--human"
            style="width: {(digest.human / digest.registry_total) * 100}%"
            title="{digest.human} human"
          ></div>
          <div
            class="bar__seg bar__seg--oos"
            style="width: {(digest.out_of_scope / digest.registry_total) * 100}%"
            title="{digest.out_of_scope} out of scope"
          ></div>
        </div>
        <ul class="legend">
          <li><span class="dot dot--auto"></span> Auto — checked by the engine</li>
          <li><span class="dot dot--human"></span> Human — surfaced in the Loom Review tab</li>
          <li><span class="dot dot--oos"></span> Out of scope — depends on features in a later release</li>
        </ul>
      {:else}
        <p class="dim">Loading Matterhorn registry…</p>
      {/if}
    </div>
  {:else}
    <div class="about">
      <h2>What is Loom?</h2>
      <p>
        Loom is Slab's offline accessibility workbench, targeting PDF/UA-1
        (ISO 14289-1:2014). Adobe charges $239/year for Acrobat Pro;
        CommonLook charges $1,800/seat for a tagging tool. Loom is free,
        runs entirely on your machine, and never uploads your file.
      </p>
      <h3>What ships today</h3>
      <ul>
        <li><strong>Matterhorn registry</strong> — every failure condition, with verdict, sourced from the PDF Association protocol.</li>
        <li><strong>Layout extraction</strong> — every text run + image placement, with bbox + font size, ready for downstream tagging.</li>
      </ul>
      <h3>What's next (v3.1.0)</h3>
      <ol>
        <li>Segments — group runs into lines, columns, blocks.</li>
        <li>Classify — H1..H6 / P / Figure / Table from heuristics.</li>
        <li>Tag — emit StructTreeRoot + Alt text + Lang.</li>
        <li>Validate — run the auto-decidable Matterhorn checks.</li>
        <li>Review — surface human-judgement checks in the UI.</li>
      </ol>
    </div>
  {/if}
</section>

<style>
  .loom {
    padding: 24px 28px 48px;
    max-width: 1100px;
    margin: 0 auto;
    color: var(--text, #e6e9ef);
  }
  .loom__head h1 {
    margin: 0 0 4px;
    font-size: 26px;
    font-weight: 600;
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .badge {
    font-size: 11px;
    padding: 2px 8px;
    border-radius: 999px;
    background: rgba(120, 180, 255, 0.16);
    color: #9bbcff;
    font-weight: 500;
    letter-spacing: 0.04em;
  }
  .sub {
    color: var(--text-dim, #9aa3b3);
    font-size: 13px;
    margin: 0 0 14px;
    max-width: 680px;
    line-height: 1.55;
  }
  .tabs {
    display: flex;
    gap: 2px;
    background: rgba(255, 255, 255, 0.04);
    border-radius: 8px;
    padding: 4px;
    margin: 14px 0 20px;
    width: max-content;
  }
  .tabs button {
    background: transparent;
    border: 0;
    color: var(--text-dim, #9aa3b3);
    padding: 6px 14px;
    font-size: 13px;
    border-radius: 6px;
    cursor: pointer;
    font-weight: 500;
  }
  .tabs button.active {
    background: rgba(255, 255, 255, 0.08);
    color: var(--text, #e6e9ef);
  }
  .picker {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px;
    background: rgba(255, 255, 255, 0.03);
    border-radius: 10px;
    border: 1px solid rgba(255, 255, 255, 0.06);
  }
  .primary {
    background: rgba(120, 180, 255, 0.16);
    color: #cfe0ff;
    border: 1px solid rgba(120, 180, 255, 0.28);
    border-radius: 7px;
    padding: 7px 14px;
    cursor: pointer;
    font-size: 13px;
    font-weight: 500;
  }
  .primary:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
  .path {
    flex: 1;
    font-size: 13px;
    color: var(--text-dim, #9aa3b3);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .status {
    margin: 14px 2px 0;
    font-size: 13px;
  }
  .status[data-kind="ok"] {
    color: #8fd6a4;
  }
  .status[data-kind="err"] {
    color: #ff8d8d;
  }
  .status[data-kind="working"] {
    color: #9bbcff;
  }
  .totals {
    display: flex;
    gap: 12px;
    margin: 20px 0 12px;
  }
  .totals--four {
    flex-wrap: wrap;
  }
  .totals__item {
    flex: 1;
    min-width: 130px;
    background: rgba(255, 255, 255, 0.03);
    border: 1px solid rgba(255, 255, 255, 0.06);
    border-radius: 10px;
    padding: 14px 16px;
  }
  .totals__item .n {
    font-size: 22px;
    font-weight: 600;
    color: var(--text, #e6e9ef);
  }
  .totals__item .k {
    font-size: 11px;
    color: var(--text-dim, #9aa3b3);
    text-transform: uppercase;
    letter-spacing: 0.07em;
    margin-top: 4px;
  }
  .page-table {
    margin-top: 8px;
    border: 1px solid rgba(255, 255, 255, 0.06);
    border-radius: 10px;
    overflow: hidden;
    background: rgba(255, 255, 255, 0.02);
  }
  .row {
    display: grid;
    grid-template-columns: 70px 110px 80px 80px 1fr;
    gap: 12px;
    padding: 9px 14px;
    font-size: 13px;
    align-items: center;
    border-bottom: 1px solid rgba(255, 255, 255, 0.04);
  }
  .row:last-child {
    border-bottom: 0;
  }
  .row.head {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.07em;
    color: var(--text-dim, #9aa3b3);
    background: rgba(255, 255, 255, 0.03);
  }
  .sizes {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .chip {
    display: inline-block;
    padding: 2px 7px;
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.06);
    font-size: 11px;
    color: var(--text-dim, #cfd6e3);
  }
  .dim {
    color: var(--text-dim, #6b7384);
  }
  .empty {
    margin-top: 22px;
    padding: 28px;
    background: rgba(255, 255, 255, 0.025);
    border: 1px dashed rgba(255, 255, 255, 0.08);
    border-radius: 12px;
    text-align: center;
  }
  .empty h2 {
    margin: 0 0 6px;
    font-size: 16px;
    font-weight: 500;
  }
  .empty p {
    margin: 0 auto;
    max-width: 560px;
    color: var(--text-dim, #9aa3b3);
    font-size: 13px;
    line-height: 1.55;
  }
  .cta-row {
    margin-top: 28px;
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .cta {
    background: transparent;
    border: 1px solid rgba(255, 255, 255, 0.12);
    color: var(--text-dim, #9aa3b3);
    padding: 8px 14px;
    border-radius: 7px;
    cursor: not-allowed;
    font-size: 13px;
  }
  .cta-hint {
    color: var(--text-dim, #6b7384);
    font-size: 12px;
  }
  .cta-hint a {
    color: #9bbcff;
  }
  .bar {
    display: flex;
    height: 14px;
    border-radius: 999px;
    overflow: hidden;
    margin: 12px 0 6px;
    background: rgba(255, 255, 255, 0.04);
  }
  .bar__seg--auto {
    background: #5cc88a;
  }
  .bar__seg--human {
    background: #f1c34a;
  }
  .bar__seg--oos {
    background: #8a93a5;
  }
  .legend {
    list-style: none;
    padding: 0;
    margin: 8px 0 0;
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
    color: var(--text-dim, #9aa3b3);
    font-size: 12px;
  }
  .legend li {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .dot {
    width: 10px;
    height: 10px;
    border-radius: 999px;
    display: inline-block;
  }
  .dot--auto {
    background: #5cc88a;
  }
  .dot--human {
    background: #f1c34a;
  }
  .dot--oos {
    background: #8a93a5;
  }
  .conformance h2 {
    margin: 4px 0 6px;
    font-size: 17px;
    font-weight: 500;
  }
  .about h2,
  .about h3 {
    margin: 18px 0 6px;
    font-size: 15px;
    font-weight: 500;
  }
  .about ul,
  .about ol {
    color: var(--text-dim, #cfd6e3);
    font-size: 13px;
    line-height: 1.6;
    padding-left: 22px;
  }
  .about strong {
    color: var(--text, #e6e9ef);
  }
  code {
    font-family: "JetBrains Mono", ui-monospace, monospace;
    font-size: 12px;
    color: #cfe0ff;
    background: rgba(120, 180, 255, 0.1);
    padding: 1px 5px;
    border-radius: 4px;
  }
</style>
