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

  type LoomClassifyHeading = {
    page: number;
    level: number;
    text: string;
  };

  type LoomClassifyPage = {
    page_number: number;
    headings: number;
    paragraphs: number;
    list_items: number;
    figures: number;
    artifacts: number;
  };

  type LoomClassifySummary = {
    total_pages: number;
    total_nodes: number;
    heading_count: number;
    paragraph_count: number;
    list_count: number;
    list_item_count: number;
    figure_count: number;
    artifact_count: number;
    headings: LoomClassifyHeading[];
    pages: LoomClassifyPage[];
  };

  type LoomReadingOrderPage = {
    page_number: number;
    column_count: number;
    spanner_count: number;
    artifact_count: number;
    reading_node_count: number;
  };

  type LoomReadingOrderFlowEntry = {
    page: number;
    tag: string;
    text: string;
  };

  type LoomReadingOrderSummary = {
    total_pages: number;
    multi_column_pages: number;
    total_reading_nodes: number;
    total_spanners: number;
    pages: LoomReadingOrderPage[];
    flow_preview: LoomReadingOrderFlowEntry[];
  };

  type LoomAltTextSample = {
    page: number;
    x: number;
    y: number;
    width: number;
    height: number;
    alt_text: string;
  };

  type LoomAltTextSummary = {
    figures_total: number;
    generated: number;
    cache_hits: number;
    skipped_tiny: number;
    skipped_preexisting: number;
    errors: number;
    elapsed_ms: number;
    samples: LoomAltTextSample[];
  };

  type Tab = "layout" | "outline" | "reading" | "alt-text" | "tag" | "validate" | "conformance" | "about";
  let tab: Tab = "layout";

  let inputPath = "";
  let summary: LoomLayoutSummary | null = null;
  let digest: LoomMatterhornDigest | null = null;
  let classifySummary: LoomClassifySummary | null = null;
  let classifyStatus: Status = idle;
  let readingSummary: LoomReadingOrderSummary | null = null;
  let readingStatus: Status = idle;
  let altSummary: LoomAltTextSummary | null = null;
  let altStatus: Status = idle;
  type LoomCheckResult = {
    id: string;
    title: string;
    passed: boolean;
    detail: string | null;
  };
  type LoomValidateReport = {
    checks: LoomCheckResult[];
    passed: number;
    failed: number;
    overall: boolean;
  };
  type LoomMetadataStats = {
    xmp_bytes: number;
    title_set: boolean;
    lang_set: boolean;
    viewer_prefs_set: boolean;
  };
  type LoomTagResult = {
    output_path: string;
    elapsed_ms: number;
    pages_processed: number;
    pages_skipped: number;
    bdc_pairs_injected: number;
    struct_elems_created: number;
    figures_with_alt_text: number;
    validation: LoomValidateReport;
    metadata: LoomMetadataStats;
  };
  let tagResult: LoomTagResult | null = null;
  let tagStatus: Status = idle;
  let tagBadgeReveal = false;
  let tagValidateReveal = false;
  let validateReport: LoomValidateReport | null = null;
  let validateStatus: Status = idle;
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

  async function runClassify() {
    if (!inputPath) return;
    classifyStatus = { kind: "working", msg: "Classifying structure…" };
    classifySummary = null;
    try {
      const r = await invoke<CmdResult<LoomClassifySummary>>(
        "slab_loom_classify_summary",
        { input: inputPath },
      );
      if (r.kind === "ok") {
        classifySummary = r.value;
        const v = r.value;
        classifyStatus = {
          kind: "ok",
          msg: `Detected ${v.heading_count} heading${v.heading_count === 1 ? "" : "s"}, ${v.paragraph_count.toLocaleString()} paragraphs, ${v.list_item_count} list item${v.list_item_count === 1 ? "" : "s"}, ${v.figure_count} figure${v.figure_count === 1 ? "" : "s"}, ${v.artifact_count} artifact${v.artifact_count === 1 ? "" : "s"}.`,
        };
      } else {
        classifyStatus = { kind: "err", msg: r.message };
      }
    } catch (e) {
      classifyStatus = { kind: "err", msg: String(e) };
    }
  }

  async function runReading() {
    if (!inputPath) return;
    readingStatus = { kind: "working", msg: "Computing reading order…" };
    readingSummary = null;
    try {
      const r = await invoke<CmdResult<LoomReadingOrderSummary>>(
        "slab_loom_reading_order_summary",
        { input: inputPath },
      );
      if (r.kind === "ok") {
        readingSummary = r.value;
        const v = r.value;
        readingStatus = {
          kind: "ok",
          msg:
            v.multi_column_pages > 0
              ? `Found multi-column content on ${v.multi_column_pages} of ${v.total_pages} page${v.total_pages === 1 ? "" : "s"} — re-ordered ${v.total_reading_nodes.toLocaleString()} reading-flow node${v.total_reading_nodes === 1 ? "" : "s"} into screen-reader order.`
              : `Single-column document — ${v.total_reading_nodes.toLocaleString()} reading-flow node${v.total_reading_nodes === 1 ? "" : "s"} in natural top-to-bottom order.`,
        };
      } else {
        readingStatus = { kind: "err", msg: r.message };
      }
    } catch (e) {
      readingStatus = { kind: "err", msg: String(e) };
    }
  }

  async function runAltText() {
    if (!inputPath) return;
    altStatus = {
      kind: "working",
      msg: "Generating alt-text via Beacon (this may take a moment per figure)…",
    };
    altSummary = null;
    try {
      const r = await invoke<CmdResult<LoomAltTextSummary>>(
        "slab_loom_alt_text_summary",
        { input: inputPath },
      );
      if (r.kind === "ok") {
        altSummary = r.value;
        const v = r.value;
        const sec = (v.elapsed_ms / 1000).toFixed(1);
        if (v.figures_total === 0) {
          altStatus = {
            kind: "ok",
            msg: "No figures detected — nothing to describe.",
          };
        } else {
          altStatus = {
            kind: "ok",
            msg: `Generated ${v.generated} · ${v.cache_hits} cached · ${v.errors} error${v.errors === 1 ? "" : "s"} across ${v.figures_total} figure${v.figures_total === 1 ? "" : "s"} in ${sec}s.`,
          };
        }
      } else {
        altStatus = { kind: "err", msg: r.message };
      }
    } catch (e) {
      altStatus = { kind: "err", msg: String(e) };
    }
  }

  async function runTag() {
    if (!inputPath) return;
    tagStatus = {
      kind: "working",
      msg: "Tagging document for PDF/UA-1 — layout, classify, alt-text, weave…",
    };
    tagResult = null;
    tagBadgeReveal = false;
    tagValidateReveal = false;
    try {
      const r = await invoke<CmdResult<LoomTagResult>>(
        "slab_loom_tag_document",
        { input: inputPath },
      );
      if (r.kind === "ok") {
        tagResult = r.value;
        const sec = (r.value.elapsed_ms / 1000).toFixed(1);
        tagStatus = {
          kind: "ok",
          msg: `Tagged ${r.value.pages_processed} page${r.value.pages_processed === 1 ? "" : "s"} in ${sec}s · ${r.value.struct_elems_created.toLocaleString()} StructElems · ${r.value.bdc_pairs_injected.toLocaleString()} marked-content sequences.`,
        };
        // Trigger badge reveal animation on next tick.
        setTimeout(() => (tagBadgeReveal = true), 50);
        // Slice 6: stagger the "Validated ✓ ISO 14289-1" sub-badge so it
        // pops in just after the main pill — gives the screenshot a beat.
        setTimeout(() => (tagValidateReveal = true), 380);
      } else {
        tagStatus = { kind: "err", msg: r.message };
      }
    } catch (e) {
      tagStatus = { kind: "err", msg: String(e) };
    }
  }

  async function runValidate() {
    if (!inputPath) return;
    validateStatus = {
      kind: "working",
      msg: "Validating PDF against ISO 14289-1 …",
    };
    validateReport = null;
    try {
      const r = await invoke<CmdResult<LoomValidateReport>>(
        "slab_loom_validate",
        { input: inputPath },
      );
      if (r.kind === "ok") {
        validateReport = r.value;
        validateStatus = {
          kind: r.value.overall ? "ok" : "err",
          msg: r.value.overall
            ? `Conforms to ISO 14289-1 · ${r.value.passed}/${r.value.checks.length} checks passed.`
            : `Non-conforming · ${r.value.failed} of ${r.value.checks.length} checks failed.`,
        };
      } else {
        validateStatus = { kind: "err", msg: r.message };
      }
    } catch (e) {
      validateStatus = { kind: "err", msg: String(e) };
    }
  }

  function onKeydown(e: KeyboardEvent) {
    // Cmd/Ctrl+Shift+A → generate alt-text on the currently-loaded file.
    const isMod = e.metaKey || e.ctrlKey;
    if (isMod && e.shiftKey && e.key.toLowerCase() === "a") {
      if (inputPath && altStatus.kind !== "working") {
        e.preventDefault();
        tab = "alt-text";
        runAltText();
      }
    }
    // Cmd/Ctrl+Shift+T → tag the loaded PDF for PDF/UA-1.
    if (isMod && e.shiftKey && e.key.toLowerCase() === "t") {
      if (inputPath && tagStatus.kind !== "working") {
        e.preventDefault();
        tab = "tag";
        runTag();
      }
    }
    // Cmd/Ctrl+Shift+V → validate the loaded PDF against ISO 14289-1.
    if (isMod && e.shiftKey && e.key.toLowerCase() === "v") {
      if (inputPath && validateStatus.kind !== "working") {
        e.preventDefault();
        tab = "validate";
        runValidate();
      }
    }
  }
</script>

<svelte:window on:keydown={onKeydown} />

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
      aria-selected={tab === "outline"}
      class:active={tab === "outline"}
      on:click={() => (tab = "outline")}>Outline</button
    >
    <button
      role="tab"
      aria-selected={tab === "reading"}
      class:active={tab === "reading"}
      on:click={() => (tab = "reading")}>Reading order</button
    >
    <button
      role="tab"
      aria-selected={tab === "alt-text"}
      class:active={tab === "alt-text"}
      on:click={() => (tab = "alt-text")}>Alt-text</button
    >
    <button
      role="tab"
      aria-selected={tab === "tag"}
      class:active={tab === "tag"}
      on:click={() => (tab = "tag")}>Tag PDF</button
    >
    <button
      role="tab"
      aria-selected={tab === "validate"}
      class:active={tab === "validate"}
      on:click={() => (tab = "validate")}>Validate</button
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
  {:else if tab === "outline"}
    <div class="outline">
      <div class="picker">
        <button class="primary" on:click={pickPdf}>Pick PDF…</button>
        <div class="path" title={inputPath}>
          {inputPath ? basename(inputPath) : "no file selected"}
        </div>
        <button
          class="primary"
          disabled={!inputPath || classifyStatus.kind === "working"}
          on:click={runClassify}
        >
          {classifyStatus.kind === "working" ? "Classifying…" : "Detect structure"}
        </button>
      </div>

      {#if classifyStatus.kind !== "idle"}
        <p class="status" data-kind={classifyStatus.kind}>{classifyStatus.msg}</p>
      {/if}

      {#if classifySummary}
        <div class="totals totals--four">
          <div class="totals__item">
            <div class="n">{classifySummary.heading_count.toLocaleString()}</div>
            <div class="k">headings</div>
          </div>
          <div class="totals__item">
            <div class="n">{classifySummary.paragraph_count.toLocaleString()}</div>
            <div class="k">paragraphs</div>
          </div>
          <div class="totals__item">
            <div class="n">{classifySummary.list_item_count.toLocaleString()}</div>
            <div class="k">list items</div>
          </div>
          <div class="totals__item">
            <div class="n">{classifySummary.figure_count.toLocaleString()}</div>
            <div class="k">figures</div>
          </div>
          <div class="totals__item">
            <div class="n">{classifySummary.artifact_count.toLocaleString()}</div>
            <div class="k">artifacts</div>
          </div>
          <div class="totals__item">
            <div class="n">{classifySummary.total_nodes.toLocaleString()}</div>
            <div class="k">total nodes</div>
          </div>
        </div>

        {#if classifySummary.headings.length > 0}
          <h3 class="sub-h">Detected outline</h3>
          <ul class="outline-list">
            {#each classifySummary.headings as h}
              <li class="outline-item" data-level={h.level}>
                <span class="lvl">H{h.level}</span>
                <span class="ot">{h.text || "(empty)"}</span>
                <span class="pg">p.{h.page}</span>
              </li>
            {/each}
          </ul>
          {#if classifySummary.heading_count > classifySummary.headings.length}
            <p class="dim small">
              Showing first {classifySummary.headings.length} of
              {classifySummary.heading_count} headings.
            </p>
          {/if}
        {:else}
          <p class="dim">
            No headings detected. The classifier uses font-size buckets relative to
            the document body size — a uniformly-sized document will show every
            run as a paragraph.
          </p>
        {/if}

        <h3 class="sub-h">Per-page breakdown</h3>
        <div class="page-table page-table--outline" role="table">
          <div class="row head row--outline" role="row">
            <div role="columnheader">Page</div>
            <div role="columnheader">H</div>
            <div role="columnheader">P</div>
            <div role="columnheader">LI</div>
            <div role="columnheader">Figs</div>
            <div role="columnheader">Artifacts</div>
          </div>
          {#each classifySummary.pages as p (p.page_number)}
            <div class="row row--outline" role="row">
              <div role="cell">#{p.page_number}</div>
              <div role="cell">{p.headings}</div>
              <div role="cell">{p.paragraphs}</div>
              <div role="cell">{p.list_items}</div>
              <div role="cell">{p.figures}</div>
              <div role="cell">{p.artifacts}</div>
            </div>
          {/each}
        </div>
      {:else if classifyStatus.kind === "idle"}
        <div class="empty">
          <h2>Detect document structure</h2>
          <p>
            Loom's classifier infers PDF/UA logical structure from layout:
            heading levels from font-size buckets, lists from bullet/number
            markers, figures from image placements (with nearby captions),
            and page chrome (folio + repeating header/footer) tagged as
            artifacts so screen readers skip them.
          </p>
          <p class="dim">
            This is the heuristic pass — Slice 5 ships the StructTreeRoot
            writer that turns these decisions into a real tagged PDF.
          </p>
        </div>
      {/if}
    </div>
  {:else if tab === "reading"}
    <div class="reading">
      <div class="picker">
        <button class="primary" on:click={pickPdf}>Pick PDF…</button>
        <div class="path" title={inputPath}>
          {inputPath ? basename(inputPath) : "no file selected"}
        </div>
        <button
          class="primary"
          disabled={!inputPath || readingStatus.kind === "working"}
          on:click={runReading}
        >
          {readingStatus.kind === "working"
            ? "Computing…"
            : "Compute reading order"}
        </button>
      </div>

      {#if readingStatus.kind !== "idle"}
        <p class="status" data-kind={readingStatus.kind}>{readingStatus.msg}</p>
      {/if}

      {#if readingSummary}
        <div class="totals totals--four">
          <div class="totals__item">
            <div class="n">{readingSummary.total_pages.toLocaleString()}</div>
            <div class="k">pages</div>
          </div>
          <div class="totals__item">
            <div class="n">
              {readingSummary.multi_column_pages.toLocaleString()}
            </div>
            <div class="k">multi-column</div>
          </div>
          <div class="totals__item">
            <div class="n">
              {readingSummary.total_reading_nodes.toLocaleString()}
            </div>
            <div class="k">reading nodes</div>
          </div>
          <div class="totals__item">
            <div class="n">{readingSummary.total_spanners.toLocaleString()}</div>
            <div class="k">page spanners</div>
          </div>
        </div>

        <h3 class="sub-h">Reading-flow preview</h3>
        <p class="dim small">
          What a screen reader would emit, in correct order. Multi-column pages
          are walked left column top-to-bottom, then right column —
          <em>not</em> physical content-stream order.
        </p>
        {#if readingSummary.flow_preview.length > 0}
          <ol class="flow-list">
            {#each readingSummary.flow_preview as f, i}
              <li class="flow-item" data-tag={f.tag}>
                <span class="flow-idx">{i + 1}</span>
                <span class="flow-tag">{f.tag}</span>
                <span class="flow-text">{f.text || "(figure)"}</span>
                <span class="flow-pg">p.{f.page}</span>
              </li>
            {/each}
          </ol>
          {#if readingSummary.total_reading_nodes > readingSummary.flow_preview.length}
            <p class="dim small">
              Showing first {readingSummary.flow_preview.length} of
              {readingSummary.total_reading_nodes.toLocaleString()} reading-flow
              nodes.
            </p>
          {/if}
        {:else}
          <p class="dim">
            No reading-flow nodes detected — the page is likely empty or
            consists entirely of artifacts.
          </p>
        {/if}

        <h3 class="sub-h">Per-page columns</h3>
        <div class="page-table page-table--outline" role="table">
          <div class="row head row--outline" role="row">
            <div role="columnheader">Page</div>
            <div role="columnheader">Cols</div>
            <div role="columnheader">Spanners</div>
            <div role="columnheader">Artifacts</div>
            <div role="columnheader">Read nodes</div>
            <div role="columnheader"></div>
          </div>
          {#each readingSummary.pages as p (p.page_number)}
            <div class="row row--outline" role="row">
              <div role="cell">#{p.page_number}</div>
              <div role="cell">
                {p.column_count === 0 ? "—" : p.column_count}
              </div>
              <div role="cell">{p.spanner_count}</div>
              <div role="cell">{p.artifact_count}</div>
              <div role="cell">{p.reading_node_count}</div>
              <div role="cell" class="dim small">
                {p.column_count >= 2 ? "multi-column" : ""}
              </div>
            </div>
          {/each}
        </div>
      {:else if readingStatus.kind === "idle"}
        <div class="empty">
          <h2>Reading order — column-aware</h2>
          <p>
            PDF/UA requires screen readers to walk the document in
            <em>logical</em> reading order, not the order operators happen to
            appear in the content stream. On two-column research papers,
            magazine layouts, and legal briefs that means: left column
            top-to-bottom, <em>then</em> right column top-to-bottom — never
            jumping back and forth mid-sentence.
          </p>
          <p class="dim">
            Slab detects column bands by clustering text run midpoints,
            promotes page-spanning headings and figures, and parks page chrome
            (folio, repeating header/footer) as artifacts so screen readers
            skip them. This pass alone covers Matterhorn checkpoint 09-001.
          </p>
        </div>
      {/if}
    </div>
  {:else if tab === "alt-text"}
    <div class="alt-text">
      <div class="picker">
        <button class="primary" on:click={pickPdf}>Pick PDF…</button>
        <div class="path" title={inputPath}>
          {inputPath ? basename(inputPath) : "no file selected"}
        </div>
        <button
          class="primary"
          disabled={!inputPath || altStatus.kind === "working"}
          on:click={runAltText}
          title="Generate alt-text for every figure (Cmd/Ctrl+Shift+A)"
        >
          {altStatus.kind === "working"
            ? "Generating…"
            : "Generate alt-text"}
        </button>
        <kbd class="shortcut" aria-hidden="true">⌘⇧A</kbd>
      </div>

      {#if altStatus.kind !== "idle"}
        <p class="status" data-kind={altStatus.kind}>{altStatus.msg}</p>
      {/if}

      {#if altSummary}
        <div class="totals totals--four">
          <div class="totals__item">
            <div class="n">{altSummary.figures_total.toLocaleString()}</div>
            <div class="k">figures</div>
          </div>
          <div class="totals__item">
            <div class="n">{altSummary.generated.toLocaleString()}</div>
            <div class="k">generated</div>
          </div>
          <div class="totals__item">
            <div class="n">{altSummary.cache_hits.toLocaleString()}</div>
            <div class="k">cached</div>
          </div>
          <div class="totals__item">
            <div class="n">
              {(altSummary.elapsed_ms / 1000).toFixed(1)}s
            </div>
            <div class="k">elapsed</div>
          </div>
        </div>

        {#if altSummary.errors > 0 || altSummary.skipped_tiny > 0 || altSummary.skipped_preexisting > 0}
          <p class="dim small">
            {#if altSummary.skipped_preexisting > 0}
              {altSummary.skipped_preexisting} preserved (had existing alt-text)·
            {/if}
            {#if altSummary.skipped_tiny > 0}
              {altSummary.skipped_tiny} skipped (too small)·
            {/if}
            {#if altSummary.errors > 0}
              <span class="err-inline">{altSummary.errors} failed</span>
            {/if}
          </p>
        {/if}

        <h3 class="sub-h">Figure descriptions</h3>
        <p class="dim small">
          Generated locally via Beacon — your file never leaves this machine.
          Adobe Acrobat Pro charges extra for this and uploads to Adobe's
          servers; Slab does it free, offline, with a content-addressed
          cache so reruns are instant.
        </p>
        {#if altSummary.samples.length > 0}
          <ul class="alt-list">
            {#each altSummary.samples as s, i}
              <li class="alt-item">
                <div class="alt-meta">
                  <span class="alt-idx">{i + 1}</span>
                  <span class="alt-page">p.{s.page}</span>
                  <span class="alt-dim"
                    >{Math.round(s.width)}×{Math.round(s.height)}pt</span
                  >
                </div>
                <blockquote class="alt-quote">{s.alt_text}</blockquote>
              </li>
            {/each}
          </ul>
          {#if altSummary.figures_total > altSummary.samples.length}
            <p class="dim small">
              Showing first {altSummary.samples.length} of {altSummary.figures_total}
              figures.
            </p>
          {/if}
        {:else}
          <p class="dim">
            No figure descriptions to display — either no figures were found
            or every one was skipped or errored.
          </p>
        {/if}
      {:else if altStatus.kind === "idle"}
        <div class="empty">
          <h2>AI alt-text — for every figure, fully offline</h2>
          <p>
            PDF/UA §7.3 and WCAG 2.2 SC 1.1.1 require every meaningful image
            in a PDF to carry an <code>/Alt</code> description so screen
            readers can convey it to blind users. Most teams skip this step
            because it's tedious; the few that don't hand-write captions or
            use Adobe Sensei (which uploads your file to Adobe's servers).
          </p>
          <p>
            Slab generates alt-text locally via the Beacon vision provider
            (Ollama llava by default). Your file stays on this machine. The
            result is cached by image content hash, so re-running on the
            same document is instant. Press
            <kbd>⌘⇧A</kbd> on the loaded PDF to start.
          </p>
        </div>
      {/if}
    </div>
  {:else if tab === "tag"}
    <div class="tag">
      <div class="picker">
        <button class="primary" on:click={pickPdf}>Pick PDF…</button>
        <div class="path" title={inputPath}>
          {inputPath ? basename(inputPath) : "no file selected"}
        </div>
        <button
          class="primary tag-cta"
          disabled={!inputPath || tagStatus.kind === "working"}
          on:click={runTag}
          title="Tag document for PDF/UA-1 (Cmd/Ctrl+Shift+T)"
        >
          {tagStatus.kind === "working"
            ? "Tagging…"
            : "Tag Document for PDF/UA"}
        </button>
        <kbd class="shortcut" aria-hidden="true">⌘⇧T</kbd>
      </div>

      {#if tagStatus.kind !== "idle"}
        <p class="status" data-kind={tagStatus.kind}>{tagStatus.msg}</p>
      {/if}

      {#if tagResult}
        <div class="tag-result">
          <div class="tag-badge" class:reveal={tagBadgeReveal}>
            <svg viewBox="0 0 24 24" width="22" height="22" aria-hidden="true">
              <path
                d="M9 16.2l-3.5-3.5L4 14.2l5 5L20 8.2l-1.5-1.5z"
                fill="currentColor"
              />
            </svg>
            <span>PDF/UA-1 emitted</span>
          </div>
          {#if tagResult.validation}
            <div
              class="tag-subbadge"
              class:reveal={tagValidateReveal}
              data-overall={tagResult.validation.overall ? "ok" : "fail"}
            >
              {#if tagResult.validation.overall}
                ✓ Validated · ISO 14289-1 · {tagResult.validation.passed}/{tagResult.validation.checks.length} checks
              {:else}
                ✕ Validation: {tagResult.validation.failed} of {tagResult.validation.checks.length} checks failed
              {/if}
            </div>
          {/if}
          <div class="totals totals--four">
            <div class="totals__item">
              <div class="n">{tagResult.pages_processed.toLocaleString()}</div>
              <div class="k">pages tagged</div>
            </div>
            <div class="totals__item">
              <div class="n">{tagResult.struct_elems_created.toLocaleString()}</div>
              <div class="k">StructElems</div>
            </div>
            <div class="totals__item">
              <div class="n">{tagResult.bdc_pairs_injected.toLocaleString()}</div>
              <div class="k">marked-content</div>
            </div>
            <div class="totals__item">
              <div class="n">
                {(tagResult.elapsed_ms / 1000).toFixed(1)}s
              </div>
              <div class="k">elapsed</div>
            </div>
          </div>
          <p class="dim small">
            Output: <code>{tagResult.output_path}</code>
            {#if tagResult.figures_with_alt_text > 0}
              · {tagResult.figures_with_alt_text} figure{tagResult.figures_with_alt_text === 1 ? "" : "s"} carry
              <code>/Alt</code>
            {/if}
            {#if tagResult.pages_skipped > 0}
              · {tagResult.pages_skipped} page{tagResult.pages_skipped === 1 ? "" : "s"} skipped (multi-stream)
            {/if}
          </p>
          <p class="dim small">
            Open the tagged PDF in NVDA, VoiceOver, or JAWS — the screen
            reader will now walk headings, paragraphs, lists, and figure
            descriptions in logical order instead of guessing from the page
            grid.
          </p>
        </div>
      {:else if tagStatus.kind === "idle"}
        <div class="empty">
          <h2>Tag your PDF for screen readers — locally, free</h2>
          <p>
            ISO 14289-1 (PDF/UA-1) requires every PDF to carry a
            <code>/StructTreeRoot</code> so assistive technology can read it.
            Untagged PDFs read as raw coordinate-ordered text — headings,
            lists, and figures collapse into noise.
          </p>
          <p>
            Adobe Acrobat Pro's "Auto-tag" feature does this for $239/yr.
            CommonLook sells per-seat licenses starting at $1,200. veraPDF
            tags but won't generate alt-text. Slab does the whole pipeline
            in one click — layout extraction, structure classification,
            multi-column reading order, Beacon-generated alt-text, and the
            marked-content rewrite — without your file leaving this Mac.
          </p>
          <p>
            Press <kbd>⌘⇧T</kbd> on the loaded PDF to tag it. The result
            lands next to the original as <code>&lt;name&gt;.tagged.pdf</code>.
          </p>
        </div>
      {/if}
    </div>
  {:else if tab === "validate"}
    <div class="validate">
      <div class="picker">
        <button class="primary" on:click={pickPdf}>Pick PDF…</button>
        <div class="path" title={inputPath}>
          {inputPath ? basename(inputPath) : "no file selected"}
        </div>
        <button
          class="primary tag-cta"
          disabled={!inputPath || validateStatus.kind === "working"}
          on:click={runValidate}
          title="Validate against ISO 14289-1 (Cmd/Ctrl+Shift+V)"
        >
          {validateStatus.kind === "working"
            ? "Validating…"
            : "Validate against ISO 14289-1"}
        </button>
        <kbd class="shortcut" aria-hidden="true">⌘⇧V</kbd>
      </div>

      {#if validateStatus.kind !== "idle"}
        <p class="status" data-kind={validateStatus.kind}>{validateStatus.msg}</p>
      {/if}

      {#if validateReport}
        <div
          class="validate-verdict"
          data-overall={validateReport.overall ? "ok" : "fail"}
        >
          {#if validateReport.overall}
            <span class="verdict-icon">✓</span>
            <div>
              <strong>Conforms to ISO 14289-1 (PDF/UA-1)</strong>
              <div class="verdict-sub">
                {validateReport.passed} of {validateReport.checks.length} auto-decidable
                Matterhorn conditions pass. Hand-review the remaining
                human-decidable items in the Conformance tab.
              </div>
            </div>
          {:else}
            <span class="verdict-icon">✕</span>
            <div>
              <strong>Non-conforming</strong>
              <div class="verdict-sub">
                {validateReport.failed} of {validateReport.checks.length} checks failed —
                expand the list to see what's missing.
              </div>
            </div>
          {/if}
        </div>
        <ul class="check-list" aria-label="Per-condition results">
          {#each validateReport.checks as c (c.id)}
            <li class="check-row" data-passed={c.passed ? "1" : "0"}>
              <span class="check-dot" aria-hidden="true">{c.passed ? "✓" : "✕"}</span>
              <div class="check-body">
                <div class="check-title">
                  <code>{c.id}</code> · {c.title}
                </div>
                {#if !c.passed && c.detail}
                  <div class="check-detail">{c.detail}</div>
                {/if}
              </div>
            </li>
          {/each}
        </ul>
      {:else if validateStatus.kind === "idle"}
        <div class="empty">
          <h2>Grade any PDF against ISO 14289-1 — instantly, locally, free</h2>
          <p>
            Drop a tagged PDF — your own, Acrobat's output, a vendor's — and
            we'll run the eight auto-decidable failure conditions from the
            <strong>Matterhorn Protocol 1.1</strong> against the file. You get a
            green/red verdict and a per-condition list in under a second,
            without your document ever leaving this Mac.
          </p>
          <p>
            Commercial PDF/UA validators (PAC 2024, CommonLook Validator,
            veraPDF Enterprise) start at a few hundred dollars per seat or
            require a JVM and a CLI. Slab's runs in this panel.
          </p>
          <p>
            Press <kbd>⌘⇧V</kbd> on the loaded PDF to validate. After a Tag
            run, the verdict already shows next to the success badge —
            you don't need to come here for tagged output.
          </p>
        </div>
      {/if}
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
        <li><strong>Structure classification</strong> — heuristic detection of headings (H1–H6), paragraphs, lists, figures + captions, and page artifacts.</li>
        <li><strong>Reading order</strong> — column-aware serpentine traversal so multi-column papers read left column top-to-bottom then right column, not row-by-row across both. Covers Matterhorn 09-001.</li>
      </ul>
      <h3>What's next (v3.1.0)</h3>
      <ol>
        <li>Alt text — Beacon-generated descriptions for figures (cached per image hash).</li>
        <li>Tag — emit StructTreeRoot + Alt text + Lang into a real tagged PDF.</li>
        <li>Validate — run the auto-decidable Matterhorn checks against the tagged output.</li>
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
  .sub-h {
    margin: 22px 2px 8px;
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--text-dim, #9aa3b3);
    font-weight: 600;
  }
  .outline-list {
    list-style: none;
    padding: 0;
    margin: 0 0 6px;
    background: rgba(255, 255, 255, 0.02);
    border: 1px solid rgba(255, 255, 255, 0.06);
    border-radius: 10px;
    overflow: hidden;
  }
  .flow-list {
    list-style: none;
    padding: 0;
    margin: 0 0 6px;
    background: rgba(255, 255, 255, 0.02);
    border: 1px solid rgba(255, 255, 255, 0.06);
    border-radius: 10px;
    overflow: hidden;
    counter-reset: flow;
  }
  .flow-item {
    display: grid;
    grid-template-columns: 36px 56px 1fr 50px;
    align-items: center;
    gap: 12px;
    padding: 7px 14px;
    font-size: 13px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.04);
  }
  .flow-item:last-child {
    border-bottom: 0;
  }
  .flow-item .flow-idx {
    font-variant-numeric: tabular-nums;
    color: var(--text-dim, #9aa3b3);
    font-size: 11px;
    text-align: right;
    font-weight: 600;
  }
  .flow-item .flow-tag {
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.04em;
    padding: 2px 6px;
    border-radius: 6px;
    background: rgba(160, 160, 255, 0.14);
    color: #c0c0ff;
    text-align: center;
  }
  .flow-item[data-tag^="H"] .flow-tag {
    background: rgba(180, 140, 255, 0.18);
    color: #d3bcff;
  }
  .flow-item[data-tag="Figure"] .flow-tag,
  .flow-item[data-tag="Caption"] .flow-tag {
    background: rgba(140, 220, 180, 0.18);
    color: #a4e3c2;
  }
  .flow-item[data-tag="Artifact"] .flow-tag {
    background: rgba(255, 200, 120, 0.16);
    color: #ffd49a;
  }
  .flow-item .flow-text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .flow-item .flow-pg {
    font-variant-numeric: tabular-nums;
    color: var(--text-dim, #9aa3b3);
    font-size: 12px;
    text-align: right;
  }
  .outline-item {
    display: grid;
    grid-template-columns: 44px 1fr 50px;
    align-items: center;
    gap: 12px;
    padding: 8px 14px;
    font-size: 13px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.04);
  }
  .outline-item:last-child {
    border-bottom: 0;
  }
  .outline-item .lvl {
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.04em;
    padding: 2px 6px;
    border-radius: 6px;
    background: rgba(120, 180, 255, 0.16);
    color: #9bbcff;
    text-align: center;
  }
  .outline-item[data-level="2"] .lvl {
    background: rgba(180, 140, 255, 0.16);
    color: #c3a8ff;
  }
  .outline-item[data-level="3"] .lvl {
    background: rgba(140, 220, 180, 0.16);
    color: #99d9ba;
  }
  .outline-item[data-level="4"] .lvl,
  .outline-item[data-level="5"] .lvl,
  .outline-item[data-level="6"] .lvl {
    background: rgba(255, 200, 140, 0.16);
    color: #ffc792;
  }
  .outline-item[data-level="2"] {
    padding-left: 32px;
  }
  .outline-item[data-level="3"] {
    padding-left: 52px;
  }
  .outline-item[data-level="4"] {
    padding-left: 72px;
  }
  .outline-item[data-level="5"] {
    padding-left: 92px;
  }
  .outline-item[data-level="6"] {
    padding-left: 112px;
  }
  .outline-item .ot {
    color: var(--text, #e6e9ef);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .outline-item .pg {
    color: var(--text-dim, #9aa3b3);
    font-size: 11px;
    text-align: right;
  }
  .row--outline {
    grid-template-columns: 70px 1fr 1fr 1fr 1fr 1fr;
  }
  .small {
    font-size: 11px;
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
  .alt-list {
    list-style: none;
    margin: 12px 0 16px;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .alt-item {
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 10px 12px;
    background: var(--bg-2);
  }
  .alt-meta {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 11px;
    color: var(--text-dim);
    margin-bottom: 6px;
  }
  .alt-idx {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 22px;
    height: 22px;
    padding: 0 6px;
    border-radius: 999px;
    background: var(--accent);
    color: white;
    font-weight: 600;
    font-size: 11px;
  }
  .alt-page {
    padding: 2px 6px;
    border-radius: 6px;
    background: var(--bg-3);
    color: var(--text);
  }
  .alt-dim {
    font-variant-numeric: tabular-nums;
  }
  .alt-quote {
    margin: 0;
    padding: 0;
    border: 0;
    font-style: italic;
    font-size: 13px;
    line-height: 1.55;
    color: var(--text);
  }
  .err-inline {
    color: var(--err, #dc2626);
    font-weight: 600;
  }
  .shortcut {
    display: inline-flex;
    align-items: center;
    padding: 2px 6px;
    border-radius: 6px;
    background: var(--bg-3);
    color: var(--text-dim);
    font-size: 11px;
    font-family: ui-monospace, SFMono-Regular, monospace;
    border: 1px solid var(--border);
    margin-left: 4px;
  }
  .tag-cta {
    background: linear-gradient(135deg, #7c3aed 0%, #a855f7 100%);
    color: white;
    border: none;
    font-weight: 600;
    box-shadow: 0 1px 2px rgba(124, 58, 237, 0.2);
  }
  .tag-cta:hover:not([disabled]) {
    background: linear-gradient(135deg, #6d28d9 0%, #9333ea 100%);
    box-shadow: 0 4px 14px rgba(124, 58, 237, 0.35);
  }
  .tag-cta[disabled] {
    background: linear-gradient(135deg, #7c3aed 0%, #a855f7 100%);
    opacity: 0.55;
    cursor: not-allowed;
  }
  .tag-result {
    margin-top: 16px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .tag-badge {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 8px 14px;
    border-radius: 999px;
    background: linear-gradient(135deg, #f5f3ff 0%, #ede9fe 100%);
    color: #6d28d9;
    font-weight: 600;
    font-size: 0.92rem;
    width: max-content;
    border: 1px solid #ddd6fe;
    box-shadow: 0 0 0 0 rgba(124, 58, 237, 0);
    opacity: 0;
    transform: translateY(4px) scale(0.96);
    transition:
      opacity 320ms cubic-bezier(0.16, 1, 0.3, 1),
      transform 320ms cubic-bezier(0.16, 1, 0.3, 1),
      box-shadow 480ms ease-out;
  }
  .tag-badge.reveal {
    opacity: 1;
    transform: translateY(0) scale(1);
    box-shadow: 0 0 0 6px rgba(124, 58, 237, 0.12);
  }
  @media (prefers-color-scheme: dark) {
    .tag-badge {
      background: linear-gradient(135deg, #2e1065 0%, #4c1d95 100%);
      color: #ddd6fe;
      border-color: #5b21b6;
    }
  }

  /* Slice 6 — Validate sub-badge + Validate tab styles ------------------- */
  .tag-subbadge {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 5px 11px;
    border-radius: 999px;
    font-size: 0.8rem;
    font-weight: 600;
    letter-spacing: 0.01em;
    width: max-content;
    opacity: 0;
    transform: translateY(4px);
    transition:
      opacity 280ms cubic-bezier(0.16, 1, 0.3, 1),
      transform 280ms cubic-bezier(0.16, 1, 0.3, 1);
  }
  .tag-subbadge.reveal {
    opacity: 1;
    transform: translateY(0);
  }
  .tag-subbadge[data-overall="ok"] {
    background: linear-gradient(135deg, #ecfdf5 0%, #d1fae5 100%);
    color: #065f46;
    border: 1px solid #a7f3d0;
  }
  .tag-subbadge[data-overall="fail"] {
    background: linear-gradient(135deg, #fef2f2 0%, #fee2e2 100%);
    color: #991b1b;
    border: 1px solid #fecaca;
  }
  @media (prefers-color-scheme: dark) {
    .tag-subbadge[data-overall="ok"] {
      background: linear-gradient(135deg, #064e3b 0%, #065f46 100%);
      color: #a7f3d0;
      border-color: #047857;
    }
    .tag-subbadge[data-overall="fail"] {
      background: linear-gradient(135deg, #7f1d1d 0%, #991b1b 100%);
      color: #fecaca;
      border-color: #b91c1c;
    }
  }

  .validate {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .validate-verdict {
    display: flex;
    gap: 14px;
    align-items: flex-start;
    padding: 14px 18px;
    border-radius: 14px;
    border: 1px solid var(--border, #e5e7eb);
  }
  .validate-verdict[data-overall="ok"] {
    background: linear-gradient(135deg, #ecfdf5 0%, #d1fae5 100%);
    color: #065f46;
    border-color: #a7f3d0;
  }
  .validate-verdict[data-overall="fail"] {
    background: linear-gradient(135deg, #fef2f2 0%, #fee2e2 100%);
    color: #991b1b;
    border-color: #fecaca;
  }
  .verdict-icon {
    font-size: 1.6rem;
    line-height: 1;
    flex-shrink: 0;
  }
  .verdict-sub {
    margin-top: 4px;
    font-size: 0.86rem;
    opacity: 0.85;
  }
  .check-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .check-row {
    display: flex;
    gap: 10px;
    align-items: flex-start;
    padding: 8px 12px;
    border-radius: 10px;
    border: 1px solid var(--border, #e5e7eb);
    background: var(--bg-elevated, #fafafa);
  }
  .check-row[data-passed="1"] .check-dot {
    color: #16a34a;
  }
  .check-row[data-passed="0"] .check-dot {
    color: #dc2626;
  }
  .check-dot {
    font-weight: 700;
    width: 1em;
  }
  .check-title {
    font-size: 0.88rem;
  }
  .check-title code {
    background: rgba(0, 0, 0, 0.06);
    padding: 1px 5px;
    border-radius: 5px;
    margin-right: 4px;
  }
  .check-detail {
    margin-top: 3px;
    font-size: 0.8rem;
    color: var(--text-dim, #6b7280);
  }
  @media (prefers-color-scheme: dark) {
    .validate-verdict[data-overall="ok"] {
      background: linear-gradient(135deg, #064e3b 0%, #065f46 100%);
      color: #a7f3d0;
      border-color: #047857;
    }
    .validate-verdict[data-overall="fail"] {
      background: linear-gradient(135deg, #7f1d1d 0%, #991b1b 100%);
      color: #fecaca;
      border-color: #b91c1c;
    }
    .check-row {
      background: rgba(255, 255, 255, 0.04);
      border-color: rgba(255, 255, 255, 0.08);
    }
    .check-title code {
      background: rgba(255, 255, 255, 0.1);
    }
  }
</style>
