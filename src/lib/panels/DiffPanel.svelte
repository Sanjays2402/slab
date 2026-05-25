<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { idle, basename, stripExt, type CmdResult, type Status } from "$lib/types";
  import { isInTauri } from "$lib/tauri";

  const inTauri = isInTauri();

  // --- Backend DTOs (mirror src-tauri/src/pdf/diff.rs) ---
  type DiffOp = "equal" | "insert" | "delete";
  type WordOp = "equal" | "insert" | "delete";
  type WordDiff = { op: WordOp; text: string };
  type LineDiff = {
    op: DiffOp;
    old_line: number | null;
    new_line: number | null;
    text: string;
    words?: WordDiff[] | null;
  };
  type DiffSummary = { added: number; removed: number; changed: number };
  type PageDiff = {
    old_page: number | null;
    new_page: number | null;
    lines: LineDiff[];
    summary: DiffSummary;
  };
  type DocDiff = {
    old_path: string;
    new_path: string;
    old_page_count: number;
    new_page_count: number;
    pages: PageDiff[];
    total: DiffSummary;
  };

  let oldPath = $state<string | null>(null);
  let newPath = $state<string | null>(null);
  let status = $state<Status>(idle);
  let diff = $state<DocDiff | null>(null);
  let filter = $state<"all" | "changed">("changed");
  let showEqualContext = $state(true);

  type BeaconDiffSummary = {
    content: string;
    model: string;
    truncated: boolean;
    pages_included: number;
    pages_total: number;
  };
  let aiSummary = $state<BeaconDiffSummary | null>(null);
  let aiBusy = $state(false);
  let aiError = $state<string | null>(null);

  // v3.23.0 — shareable redline PDF export.
  type StackRedlineSummary = { pages: number; inserts: number; deletes: number };
  let redlineBusy = $state(false);

  async function pickOld() {
    const picked = await open({
      multiple: false,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof picked !== "string") return;
    oldPath = picked;
    diff = null;
    status = idle;
  }

  async function pickNew() {
    const picked = await open({
      multiple: false,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof picked !== "string") return;
    newPath = picked;
    diff = null;
    status = idle;
  }

  function clearOld() {
    oldPath = null;
    diff = null;
    status = idle;
  }

  function clearNew() {
    newPath = null;
    diff = null;
    status = idle;
  }

  async function runCompare() {
    if (!oldPath || !newPath) {
      status = { kind: "err", msg: "Pick both an old and a new PDF first." };
      return;
    }
    if (oldPath === newPath) {
      status = {
        kind: "err",
        msg: "The same file is selected for both sides — diff will be empty.",
      };
    }
    status = { kind: "working", msg: "Comparing…" };
    diff = null;
    aiSummary = null;
    aiError = null;
    try {
      const res = await invoke<CmdResult<DocDiff>>("slab_diff_pdfs", {
        old: oldPath,
        new: newPath,
      });
      if (res.kind === "ok") {
        diff = res.value;
        const t = res.value.total;
        const parts: string[] = [];
        if (t.added > 0) parts.push(`+${t.added}`);
        if (t.removed > 0) parts.push(`−${t.removed}`);
        if (t.changed > 0) parts.push(`~${t.changed}`);
        const summary = parts.length === 0 ? "no differences" : parts.join(" · ");
        status = {
          kind: "ok",
          msg: `${res.value.pages.length} page(s) · ${summary}`,
        };
      } else {
        status = { kind: "err", msg: res.message };
      }
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }

  function changedOnly(p: PageDiff): boolean {
    return p.summary.added + p.summary.removed + p.summary.changed > 0;
  }

  function visiblePages(d: DocDiff): PageDiff[] {
    if (filter === "all") return d.pages;
    return d.pages.filter(changedOnly);
  }

  function visibleLines(p: PageDiff): LineDiff[] {
    if (showEqualContext) return p.lines;
    return p.lines.filter((l) => l.op !== "equal");
  }

  function pageLabel(p: PageDiff): string {
    if (p.old_page && p.new_page) {
      return p.old_page === p.new_page
        ? `Page ${p.old_page}`
        : `Old p.${p.old_page} ↔ New p.${p.new_page}`;
    }
    if (p.old_page) return `Old p.${p.old_page} · removed page`;
    if (p.new_page) return `New p.${p.new_page} · added page`;
    return "—";
  }

  async function exportReport() {
    if (!oldPath || !newPath) return;
    const base = `${stripExt(basename(oldPath))}-vs-${stripExt(basename(newPath))}-diff`;
    const output = await save({
      defaultPath: `${base}.pdf`,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof output !== "string") return;
    const prev = status;
    status = { kind: "working", msg: "Building report…" };
    try {
      const res = await invoke<CmdResult<number>>("slab_diff_export_report", {
        old: oldPath,
        new: newPath,
        output,
      });
      if (res.kind === "ok") {
        status = {
          kind: "ok",
          msg: `Wrote ${basename(output)} (${res.value} page${res.value === 1 ? "" : "s"})`,
        };
      } else {
        status = { kind: "err", msg: res.message };
      }
    } catch (e) {
      status = { kind: "err", msg: String(e) };
      // Don't clobber the previous summary on transient failures.
      void prev;
    }
  }

  async function exportRedline() {
    if (!oldPath || !newPath || redlineBusy) return;
    const base = `${stripExt(basename(oldPath))}-vs-${stripExt(basename(newPath))}-redline`;
    const output = await save({
      defaultPath: `${base}.pdf`,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof output !== "string") return;
    redlineBusy = true;
    const prev = status;
    status = { kind: "working", msg: "Building shareable redline…" };
    try {
      const res = await invoke<CmdResult<StackRedlineSummary>>("slab_stack_export_redline", {
        old: oldPath,
        new: newPath,
        output,
      });
      if (res.kind === "ok") {
        const r = res.value;
        const parts: string[] = [];
        if (r.inserts > 0) parts.push(`+${r.inserts}`);
        if (r.deletes > 0) parts.push(`−${r.deletes}`);
        const detail = parts.length ? ` · ${parts.join(" ")}` : "";
        status = {
          kind: "ok",
          msg: `Wrote ${basename(output)} (${r.pages} page${r.pages === 1 ? "" : "s"})${detail}`,
        };
      } else {
        status = { kind: "err", msg: res.message };
      }
    } catch (e) {
      status = { kind: "err", msg: String(e) };
      void prev;
    } finally {
      redlineBusy = false;
    }
  }

  async function explainChanges() {
    if (!oldPath || !newPath || aiBusy) return;
    aiBusy = true;
    aiError = null;
    aiSummary = null;
    try {
      const res = await invoke<CmdResult<BeaconDiffSummary>>("slab_beacon_diff_summary", {
        old: oldPath,
        new: newPath,
        maxDiffChars: null,
      });
      if (res.kind === "ok") {
        aiSummary = res.value;
      } else {
        aiError = res.message;
      }
    } catch (e) {
      aiError = String(e);
    } finally {
      aiBusy = false;
    }
  }

  function summaryPills(s: DiffSummary): { label: string; cls: string }[] {
    const out: { label: string; cls: string }[] = [];
    if (s.added > 0) out.push({ label: `+${s.added}`, cls: "ins" });
    if (s.removed > 0) out.push({ label: `−${s.removed}`, cls: "del" });
    if (s.changed > 0) out.push({ label: `~${s.changed}`, cls: "chg" });
    if (out.length === 0) out.push({ label: "no change", cls: "eq" });
    return out;
  }

  // Stack v3.23.0 — command palette deep links. The palette dispatches a
  // CustomEvent on `window` after switching to this panel; we listen here
  // so the action runs in the correct component context.
  $effect(() => {
    function onExport() {
      void exportReport();
    }
    function onRedline() {
      void exportRedline();
    }
    function onRerun() {
      void runCompare();
    }
    window.addEventListener("slab:stack-export-report", onExport);
    window.addEventListener("slab:stack-export-redline", onRedline);
    window.addEventListener("slab:stack-rerun", onRerun);
    return () => {
      window.removeEventListener("slab:stack-export-report", onExport);
      window.removeEventListener("slab:stack-export-redline", onRedline);
      window.removeEventListener("slab:stack-rerun", onRerun);
    };
  });
</script>

<header class="content-header">
  <h1>Diff</h1>
  <p class="subtitle">
    Compare two PDFs side by side. Line-by-line text diff with added / removed / changed counts —
    great for contract revisions, paper edits, or spec evolution.
  </p>
</header>

<section class="panel">
  <div class="picker-row">
    <div class="picker">
      <div class="picker-label">Old (left)</div>
      {#if !oldPath}
        <button class="dropzone" onclick={pickOld} disabled={!inTauri}>
          <span class="dz-icon">+</span>
          <span class="dz-title">Pick the older PDF</span>
        </button>
      {:else}
        <div class="file-card">
          <div>
            <div class="file-name">{basename(oldPath)}</div>
            <div class="file-meta">{oldPath}</div>
          </div>
          <div class="card-actions">
            <button class="ghost" onclick={pickOld}>Change</button>
            <button class="ghost" onclick={clearOld}>Clear</button>
          </div>
        </div>
      {/if}
    </div>

    <div class="picker">
      <div class="picker-label">New (right)</div>
      {#if !newPath}
        <button class="dropzone" onclick={pickNew} disabled={!inTauri}>
          <span class="dz-icon">+</span>
          <span class="dz-title">Pick the newer PDF</span>
        </button>
      {:else}
        <div class="file-card">
          <div>
            <div class="file-name">{basename(newPath)}</div>
            <div class="file-meta">{newPath}</div>
          </div>
          <div class="card-actions">
            <button class="ghost" onclick={pickNew}>Change</button>
            <button class="ghost" onclick={clearNew}>Clear</button>
          </div>
        </div>
      {/if}
    </div>
  </div>

  {#if !inTauri && !oldPath && !newPath}
    <div class="note">
      Diff needs the Slab desktop app — the web preview can&rsquo;t open local
      PDFs or run the comparison engine. Install Slab to compare two PDFs
      side-by-side with word-level redline, AI summaries, and exportable
      reports.
    </div>
  {/if}

  <div class="actions">
    <button
      class="primary"
      onclick={runCompare}
      disabled={!oldPath || !newPath || status.kind === "working"}
    >
      {status.kind === "working" ? "Comparing…" : diff ? "Re-compare" : "Compare"}
    </button>
    {#if diff}
      <button onclick={exportReport} disabled={status.kind === "working"}>
        Export Report (.pdf)
      </button>
      <button
        onclick={exportRedline}
        disabled={redlineBusy || status.kind === "working"}
        title="Share a single PDF with the redline baked in — recipients don't need Slab."
      >
        {redlineBusy ? "Building redline…" : "Export Redline (.pdf)"}
      </button>
      <button onclick={explainChanges} disabled={aiBusy || status.kind === "working"}>
        {aiBusy ? "Asking Beacon…" : "Explain Changes (AI)"}
      </button>
      <div class="filter-toggle">
        <button class:active={filter === "changed"} onclick={() => (filter = "changed")}
          >Changes only</button
        >
        <button class:active={filter === "all"} onclick={() => (filter = "all")}>All pages</button>
      </div>
      <label class="ctx-toggle">
        <input type="checkbox" bind:checked={showEqualContext} />
        Show unchanged context lines
      </label>
    {/if}
  </div>

  {#if status.kind === "ok"}
    <div class="status ok">✓ {status.msg}</div>
  {:else if status.kind === "err"}
    <div class="status err">✕ {status.msg}</div>
  {/if}

  {#if diff}
    <div class="totals">
      <div class="total-pill old">{diff.old_page_count} pages (old)</div>
      <div class="total-pill arrow">→</div>
      <div class="total-pill new">{diff.new_page_count} pages (new)</div>
      {#each summaryPills(diff.total) as p (p.label)}
        <div class="total-pill {p.cls}">{p.label}</div>
      {/each}
    </div>

    {#if aiError}
      <div class="ai-card ai-err">
        <div class="ai-head">Beacon couldn't summarize the diff</div>
        <div class="ai-msg">{aiError}</div>
      </div>
    {/if}
    {#if aiSummary}
      <div class="ai-card">
        <div class="ai-head">
          <span class="ai-title">Beacon’s take on the changes</span>
          <span class="ai-meta">
            <span class="ai-model">{aiSummary.model}</span>
            <span class="ai-pages">
              {aiSummary.pages_included}/{aiSummary.pages_total} pages
            </span>
            {#if aiSummary.truncated}
              <span class="ai-trunc">truncated</span>
            {/if}
          </span>
        </div>
        <div class="ai-body">{aiSummary.content}</div>
      </div>
    {/if}

    <div class="pages">
      {#each visiblePages(diff) as page, idx (idx)}
        <details open class="page-block" class:no-change={!changedOnly(page)}>
          <summary>
            <span class="page-label">{pageLabel(page)}</span>
            <span class="page-pills">
              {#each summaryPills(page.summary) as p (p.label)}
                <span class="pill {p.cls}">{p.label}</span>
              {/each}
            </span>
          </summary>
          <div class="lines">
            {#each visibleLines(page) as line, li (li)}
              <div class="line {line.op}">
                <span class="ln old">{line.old_line ?? ""}</span>
                <span class="ln new">{line.new_line ?? ""}</span>
                <span class="marker"
                  >{line.op === "insert" ? "+" : line.op === "delete" ? "−" : " "}</span
                >
                {#if line.words && line.words.length}
                  <span class="text redline">
                    {#each line.words as w, wi (wi)}
                      {#if w.op === "insert"}<ins class="word-ins">{w.text}</ins
                        >{:else if w.op === "delete"}<del class="word-del">{w.text}</del
                        >{:else}<span class="word-eq">{w.text}</span>{/if}
                    {/each}
                  </span>
                {:else}
                  <span class="text">{line.text || " "}</span>
                {/if}
              </div>
            {/each}
            {#if visibleLines(page).length === 0}
              <div class="empty-lines">No visible lines — try turning context back on.</div>
            {/if}
          </div>
        </details>
      {/each}
      {#if visiblePages(diff).length === 0}
        <div class="empty">
          No changed pages. Switch to “All pages” to see the unchanged ones.
        </div>
      {/if}
    </div>
  {/if}
</section>

<style>
  .picker-row {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }
  .picker-label {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-3);
    margin-bottom: 6px;
  }
  .card-actions {
    display: flex;
    gap: 6px;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-top: 12px;
    flex-wrap: wrap;
  }
  .filter-toggle {
    display: inline-flex;
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    overflow: hidden;
  }
  .filter-toggle button {
    border: 0;
    border-radius: 0;
    padding: 6px 12px;
    background: var(--bg-2);
    color: var(--text-2);
    font-size: 12px;
    cursor: pointer;
  }
  .filter-toggle button.active {
    background: var(--accent);
    color: white;
  }
  .ctx-toggle {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--text-2);
    cursor: pointer;
  }
  .totals {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin: 12px 0 8px;
    align-items: center;
  }
  .total-pill {
    font-size: 12px;
    padding: 4px 10px;
    border-radius: 999px;
    background: var(--bg-2);
    color: var(--text-2);
    border: 1px solid var(--border);
  }
  .total-pill.ins,
  .pill.ins {
    background: rgba(46, 160, 67, 0.18);
    border-color: rgba(46, 160, 67, 0.4);
    color: #5fce7a;
  }
  .total-pill.del,
  .pill.del {
    background: rgba(248, 81, 73, 0.18);
    border-color: rgba(248, 81, 73, 0.4);
    color: #f57c75;
  }
  .total-pill.chg,
  .pill.chg {
    background: rgba(214, 152, 0, 0.18);
    border-color: rgba(214, 152, 0, 0.4);
    color: #e2b455;
  }
  .total-pill.eq,
  .pill.eq {
    color: var(--text-3);
  }
  .total-pill.arrow {
    background: transparent;
    border: 0;
    color: var(--text-3);
  }
  .pages {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: 8px;
  }
  .page-block {
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    padding: 10px 12px;
  }
  .page-block.no-change {
    opacity: 0.6;
  }
  .page-block > summary {
    cursor: pointer;
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
    font-size: 13px;
    font-weight: 500;
  }
  .page-pills {
    display: inline-flex;
    gap: 6px;
  }
  .pill {
    font-size: 11px;
    padding: 2px 8px;
    border-radius: 999px;
    background: var(--bg);
    border: 1px solid var(--border);
    color: var(--text-2);
  }
  .lines {
    margin-top: 10px;
    background: var(--bg);
    border-radius: var(--r-sm);
    padding: 8px 0;
    font-family: var(--font-mono);
    font-size: 12px;
    line-height: 1.55;
    overflow-x: auto;
  }
  .line {
    display: grid;
    grid-template-columns: 42px 42px 18px 1fr;
    column-gap: 6px;
    padding: 0 12px;
  }
  .line.insert {
    background: rgba(46, 160, 67, 0.08);
  }
  .line.delete {
    background: rgba(248, 81, 73, 0.08);
  }
  .line .ln {
    color: var(--text-3);
    text-align: right;
    font-variant-numeric: tabular-nums;
    user-select: none;
  }
  .line .marker {
    text-align: center;
    color: var(--text-3);
  }
  .line.insert .marker {
    color: #5fce7a;
  }
  .line.delete .marker {
    color: #f57c75;
  }
  .line .text {
    white-space: pre-wrap;
    word-break: break-word;
    color: var(--text-1);
  }
  .empty,
  .empty-lines {
    color: var(--text-3);
    font-size: 12px;
    padding: 10px 12px;
  }
  .ai-card {
    margin: 4px 0 10px;
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    padding: 12px 14px;
  }
  .ai-card.ai-err {
    border-color: rgba(248, 81, 73, 0.4);
    background: rgba(248, 81, 73, 0.06);
  }
  .ai-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
    font-size: 12px;
    color: var(--text-2);
    margin-bottom: 8px;
    flex-wrap: wrap;
  }
  .ai-title {
    font-weight: 600;
    color: var(--text-1);
  }
  .ai-meta {
    display: inline-flex;
    gap: 8px;
    font-size: 11px;
    color: var(--text-3);
  }
  .ai-model,
  .ai-pages,
  .ai-trunc {
    padding: 2px 8px;
    border-radius: 999px;
    background: var(--bg);
    border: 1px solid var(--border);
  }
  .ai-trunc {
    color: #e2b455;
    border-color: rgba(214, 152, 0, 0.4);
  }
  .ai-body {
    font-size: 13px;
    line-height: 1.55;
    color: var(--text-1);
    white-space: pre-wrap;
    word-break: break-word;
  }
  .ai-msg {
    font-size: 12px;
    color: var(--text-2);
    white-space: pre-wrap;
  }
  /* --- Stack word-level inline redline (v3.23.0) --- */
  .line .text.redline {
    /* Reset inherited line tint so per-token coloring reads cleanly. */
    background: transparent;
  }
  .redline ins.word-ins {
    background: color-mix(in oklab, var(--ok, #2ec27e) 24%, transparent);
    color: var(--text-1);
    text-decoration: underline solid 1px;
    text-underline-offset: 2px;
    border-radius: 2px;
    padding: 0 2px;
    margin: 0 0.5px;
  }
  .redline del.word-del {
    background: color-mix(in oklab, var(--err, #e25c5c) 24%, transparent);
    color: var(--text-1);
    text-decoration: line-through 1.5px;
    border-radius: 2px;
    padding: 0 2px;
    margin: 0 0.5px;
  }
  .redline .word-eq {
    opacity: 0.78;
  }
  .note {
    margin-top: 12px;
    font-size: 12px;
    color: var(--text-3);
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-left: 3px solid var(--accent);
    padding: 8px 12px;
    border-radius: var(--r-sm);
  }
</style>
