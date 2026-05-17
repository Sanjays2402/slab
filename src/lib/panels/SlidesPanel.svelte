<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { idle, basename, type Status } from "$lib/types";
  import { analyzeSlides, type SlideReport, type SlidePage } from "$lib/slides";
  import PresenterOverlay from "$lib/components/PresenterOverlay.svelte";

  // ---------- State ----------
  let inputPath = $state<string | null>(null);
  let status = $state<Status>(idle);
  let report = $state<SlideReport | null>(null);
  let activePage = $state<number | null>(null);
  let layout = $state<"grid" | "list">("grid");
  let showOnlyNoted = $state(false);
  let presenting = $state(false);

  async function pickInput() {
    const picked = await open({
      multiple: false,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof picked !== "string") return;
    inputPath = picked;
    report = null;
    activePage = null;
    status = idle;
    await runAnalyze();
  }

  function clearInput() {
    inputPath = null;
    report = null;
    activePage = null;
    status = idle;
  }

  async function runAnalyze() {
    if (!inputPath) return;
    status = { kind: "working", msg: "Analyzing…" };
    try {
      const r = await analyzeSlides(inputPath);
      report = r;
      activePage = r.pages[0]?.page ?? null;
      const label = r.is_slides ? "Slides view" : "Document";
      status = {
        kind: "ok",
        msg: `${label} · ${r.page_count} pages · confidence ${r.confidence}/100`,
      };
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }

  // ---------- Derived ----------
  const visiblePages = $derived.by((): SlidePage[] => {
    if (!report) return [];
    if (!showOnlyNoted) return report.pages;
    return report.pages.filter((p) => p.notes.trim().length > 0);
  });

  const activeRecord = $derived.by((): SlidePage | null => {
    if (!report || activePage == null) return null;
    return report.pages.find((p) => p.page === activePage) ?? null;
  });

  /** Per-page aspect, normalized so the thumbnail box keeps the slide ratio.
   * 16:9 → wide rectangle; 4:3 → squarer. Falls back to 16:9 if missing. */
  function aspectStyle(p: SlidePage): string {
    const ratio = p.aspect > 0 ? p.aspect : 16 / 9;
    // CSS aspect-ratio with `/` separator is well-supported.
    return `aspect-ratio: ${ratio.toFixed(4)};`;
  }

  function orientationGlyph(o: SlidePage["orientation"]): string {
    if (o === "portrait") return "▯";
    if (o === "square") return "▢";
    return "▭";
  }

  function confidenceTone(c: number): "good" | "ok" | "low" {
    if (c >= 80) return "good";
    if (c >= 65) return "ok";
    return "low";
  }

  function notesPreview(notes: string): string {
    const flat = notes.replace(/\s+/g, " ").trim();
    if (flat.length <= 110) return flat;
    return flat.slice(0, 107) + "…";
  }
</script>

<header class="content-header">
  <h1>Slides</h1>
  <p class="subtitle">
    Treat PDF decks like slides. Slab auto-detects PowerPoint, Keynote, Google Slides,
    Beamer, Marp and friends — then shows you the aspect-true thumbnail grid plus
    any speaker notes embedded as PDF text annotations.
  </p>
</header>

<section class="panel">
  <div class="picker-row">
    {#if !inputPath}
      <button class="dropzone" onclick={pickInput}>
        <span class="dz-icon">▷</span>
        <span class="dz-title">Pick a PDF deck</span>
        <span class="dz-hint">PowerPoint / Keynote / Google Slides exports</span>
      </button>
    {:else}
      <div class="file-card">
        <div>
          <div class="file-name">{basename(inputPath)}</div>
          <div class="file-meta">{inputPath}</div>
        </div>
        <div class="card-actions">
          <button class="ghost" onclick={pickInput}>Change</button>
          <button class="ghost" onclick={clearInput}>Clear</button>
        </div>
      </div>
    {/if}
  </div>

  {#if status.kind === "working"}
    <div class="status working">⏳ {status.msg}</div>
  {:else if status.kind === "ok"}
    <div class="status ok">✓ {status.msg}</div>
  {:else if status.kind === "err"}
    <div class="status err">✕ {status.msg}</div>
  {/if}

  {#if report}
    <div class="meta-row">
      <div class="meta-card">
        <div class="meta-label">Verdict</div>
        <div class="meta-value verdict {report.is_slides ? 'yes' : 'no'}">
          {report.is_slides ? "Slide deck" : "Document"}
        </div>
        <div class="meta-sub conf {confidenceTone(report.confidence)}">
          confidence {report.confidence}/100
        </div>
      </div>
      <div class="meta-card">
        <div class="meta-label">Page size</div>
        <div class="meta-value">{report.dominant_label}</div>
        <div class="meta-sub">
          {report.dominant_size} pt · consistency {(report.consistency * 100).toFixed(0)}%
        </div>
      </div>
      <div class="meta-card">
        <div class="meta-label">Orientation</div>
        <div class="meta-value">
          {(report.landscape_fraction * 100).toFixed(0)}% landscape
        </div>
        <div class="meta-sub">{report.page_count} pages total</div>
      </div>
      <div class="meta-card">
        <div class="meta-label">Speaker notes</div>
        <div class="meta-value">
          {report.pages_with_notes}/{report.page_count}
        </div>
        <div class="meta-sub">
          {#if report.producer_hint && report.producer}
            from {report.producer}
          {:else if report.producer}
            producer: {report.producer}
          {:else}
            no producer metadata
          {/if}
        </div>
      </div>
    </div>

    <div class="toolbar">
      <div class="toggle">
        <button class:active={layout === "grid"} onclick={() => (layout = "grid")}>Grid</button>
        <button class:active={layout === "list"} onclick={() => (layout = "list")}>List</button>
      </div>
      <label class="toggle-row">
        <input type="checkbox" bind:checked={showOnlyNoted} />
        Only pages with notes
      </label>
      <button
        class="primary"
        onclick={() => (presenting = true)}
        disabled={!inputPath || report.page_count === 0}
        title="Start presenter mode"
      >
        ▷ Present from {activePage ?? 1}
      </button>
      {#if !report.is_slides}
        <div class="hint">
          Heuristic says this isn't a deck — but you can still use the grid view if you want.
        </div>
      {/if}
    </div>

    <div class="page-area">
      <div class="page-list" class:list-mode={layout === "list"}>
        {#each visiblePages as p (p.page)}
          <button
            type="button"
            class="thumb"
            class:active={activePage === p.page}
            onclick={() => (activePage = p.page)}
          >
            <div class="thumb-frame" style={aspectStyle(p)}>
              <span class="thumb-num">{p.page}</span>
              {#if p.notes.trim().length > 0}
                <span class="thumb-notes-pill" title="Has speaker notes">✎</span>
              {/if}
            </div>
            <div class="thumb-meta">
              <span class="thumb-dim">{p.width_pt}×{p.height_pt}</span>
              <span class="thumb-glyph">{orientationGlyph(p.orientation)}</span>
            </div>
            {#if layout === "list" && p.notes.trim().length > 0}
              <div class="thumb-notes">{notesPreview(p.notes)}</div>
            {/if}
          </button>
        {/each}
        {#if visiblePages.length === 0}
          <div class="empty">
            {#if showOnlyNoted}
              No pages with speaker notes — toggle the filter off to see the whole deck.
            {:else}
              Empty deck.
            {/if}
          </div>
        {/if}
      </div>

      <aside class="notes-pane">
        {#if activeRecord}
          <header class="notes-head">
            <div>
              <div class="notes-title">Slide {activeRecord.page}</div>
              <div class="notes-sub">
                {activeRecord.width_pt}×{activeRecord.height_pt} pt ·
                {activeRecord.orientation}
              </div>
            </div>
          </header>
          {#if activeRecord.notes.trim().length > 0}
            <pre class="notes-body">{activeRecord.notes}</pre>
          {:else}
            <div class="notes-empty">
              No speaker notes embedded on this slide.
            </div>
          {/if}
        {:else}
          <div class="notes-empty">Select a slide to read its speaker notes.</div>
        {/if}
      </aside>
    </div>
  {/if}
</section>

{#if presenting && inputPath && report}
  <PresenterOverlay
    inputPath={inputPath}
    pages={report.pages}
    startPage={activePage ?? 1}
    onClose={() => (presenting = false)}
  />
{/if}

<style>
  .picker-row {
    display: grid;
    grid-template-columns: 1fr;
    gap: 12px;
  }
  .dropzone {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 28px;
    border: 1.5px dashed var(--border);
    border-radius: var(--r-md);
    background: var(--surface);
    color: var(--text-2);
    cursor: pointer;
    transition: background 0.12s ease, border-color 0.12s ease;
  }
  .dropzone:hover {
    background: var(--surface-hover);
    border-color: var(--text-3);
  }
  .dz-icon {
    font-size: 28px;
    color: var(--text-3);
  }
  .dz-title {
    font-weight: 600;
    color: var(--text-1);
  }
  .dz-hint {
    font-size: 12px;
    color: var(--text-3);
  }
  .file-card {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 14px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
  }
  .file-name {
    font-weight: 600;
  }
  .file-meta {
    font-size: 12px;
    color: var(--text-3);
    margin-top: 2px;
  }
  .card-actions {
    display: flex;
    gap: 6px;
  }
  .status {
    margin-top: 10px;
    padding: 8px 12px;
    border-radius: var(--r-sm);
    font-size: 13px;
  }
  .status.working {
    background: var(--surface);
    color: var(--text-2);
  }
  .status.ok {
    background: rgba(34, 197, 94, 0.12);
    color: rgb(34, 197, 94);
  }
  .status.err {
    background: rgba(239, 68, 68, 0.12);
    color: rgb(239, 68, 68);
  }
  .meta-row {
    margin-top: 14px;
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
    gap: 10px;
  }
  .meta-card {
    padding: 10px 12px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
  }
  .meta-label {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-3);
    margin-bottom: 4px;
  }
  .meta-value {
    font-size: 14px;
    font-weight: 600;
    color: var(--text-1);
  }
  .meta-value.verdict.yes {
    color: rgb(34, 197, 94);
  }
  .meta-value.verdict.no {
    color: var(--text-2);
  }
  .meta-sub {
    margin-top: 2px;
    font-size: 11px;
    color: var(--text-3);
  }
  .meta-sub.conf.good {
    color: rgb(34, 197, 94);
  }
  .meta-sub.conf.ok {
    color: rgb(234, 179, 8);
  }
  .meta-sub.conf.low {
    color: rgb(239, 68, 68);
  }
  .toolbar {
    margin-top: 16px;
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
  }
  .toggle {
    display: inline-flex;
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    overflow: hidden;
  }
  .toggle button {
    padding: 6px 12px;
    background: transparent;
    border: none;
    color: var(--text-2);
    cursor: pointer;
    font-size: 12px;
  }
  .toggle button.active {
    background: var(--surface-hover);
    color: var(--text-1);
  }
  .toggle-row {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--text-2);
  }
  .hint {
    font-size: 12px;
    color: var(--text-3);
    margin-left: auto;
  }
  .page-area {
    margin-top: 14px;
    display: grid;
    grid-template-columns: minmax(0, 1fr) 320px;
    gap: 14px;
    align-items: start;
  }
  .page-list {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
    gap: 10px;
  }
  .page-list.list-mode {
    grid-template-columns: 1fr;
  }
  .thumb {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 6px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    cursor: pointer;
    text-align: left;
    transition: border-color 0.12s ease, background 0.12s ease;
  }
  .thumb:hover {
    border-color: var(--text-3);
  }
  .thumb.active {
    border-color: rgb(99, 102, 241);
    background: rgba(99, 102, 241, 0.06);
  }
  .thumb-frame {
    position: relative;
    width: 100%;
    background: var(--surface-hover);
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-3);
    font-size: 18px;
    font-weight: 600;
  }
  .thumb-num {
    user-select: none;
  }
  .thumb-notes-pill {
    position: absolute;
    top: 4px;
    right: 4px;
    font-size: 11px;
    background: rgba(99, 102, 241, 0.18);
    color: rgb(99, 102, 241);
    padding: 1px 5px;
    border-radius: var(--r-sm);
  }
  .thumb-meta {
    display: flex;
    justify-content: space-between;
    font-size: 11px;
    color: var(--text-3);
    padding: 0 2px;
  }
  .thumb-notes {
    font-size: 11px;
    color: var(--text-2);
    padding: 0 2px;
    line-height: 1.35;
  }
  .empty {
    grid-column: 1 / -1;
    padding: 20px;
    text-align: center;
    color: var(--text-3);
    background: var(--surface);
    border: 1px dashed var(--border);
    border-radius: var(--r-md);
  }
  .notes-pane {
    position: sticky;
    top: 12px;
    padding: 12px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    max-height: 70vh;
    overflow-y: auto;
  }
  .notes-head {
    margin-bottom: 8px;
  }
  .notes-title {
    font-weight: 600;
    color: var(--text-1);
  }
  .notes-sub {
    font-size: 11px;
    color: var(--text-3);
  }
  .notes-body {
    white-space: pre-wrap;
    word-break: break-word;
    font-family: inherit;
    font-size: 13px;
    line-height: 1.45;
    color: var(--text-1);
    margin: 0;
  }
  .notes-empty {
    color: var(--text-3);
    font-size: 12px;
  }
  .ghost {
    background: transparent;
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    padding: 4px 8px;
    color: var(--text-2);
    cursor: pointer;
  }
  .ghost:hover {
    background: var(--surface-hover);
    color: var(--text-1);
  }
  .primary {
    padding: 6px 12px;
    border-radius: var(--r-md);
    border: 1px solid rgb(99, 102, 241);
    background: rgb(99, 102, 241);
    color: #fff;
    cursor: pointer;
    font-size: 12px;
    font-weight: 600;
  }
  .primary:hover {
    background: rgb(79, 82, 221);
  }
  .primary:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  @media (max-width: 860px) {
    .page-area {
      grid-template-columns: 1fr;
    }
    .notes-pane {
      position: static;
      max-height: none;
    }
  }
</style>
