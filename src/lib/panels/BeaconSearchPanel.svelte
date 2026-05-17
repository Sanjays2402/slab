<script lang="ts">
  // Beacon Search panel — semantic search across all PDFs the user
  // has indexed. Workflow:
  //
  //   1. User picks a PDF and clicks "Index this PDF". We fire
  //      `slab_beacon_index_pdf`, which embeds every chunk and writes
  //      to `~/.slab/beacon-index.sqlite`. Idempotent — same content
  //      hash is a no-op.
  //   2. The footer shows running stats: "X PDFs · Y chunks indexed".
  //   3. User types a query in the search bar. We fire
  //      `slab_beacon_search` and render top-K hits as cards: page
  //      number + a snippet + similarity score. Clicking a card fires
  //      a `slab:beacon-goto-page` custom event so the Reader can jump.
  //
  // Design notes:
  // - We deliberately keep the index ops blocking the UI with a clear
  //   spinner; embedding 200 chunks takes a few seconds on local
  //   Ollama and we don't want to confuse the user with phantom rows.
  // - Same friendly-error mapping as BeaconChatPanel: "provider
  //   unavailable" → hint to start Ollama or switch provider.
  // - All-PDFs vs. this-PDF-only is a toggle once the user has
  //   indexed at least one PDF and picked one in the panel.

  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { onMount } from "svelte";
  import { basename, idle, type CmdResult, type Status } from "$lib/types";

  type SearchHit = {
    pdf_path: string;
    page: number;
    idx_in_page: number;
    text: string;
    score: number;
  };
  type IndexStats = { pdfs: number; chunks: number };
  type IndexReport = {
    pdf_hash: string;
    chunks_indexed: number;
    was_cached: boolean;
  };

  let pdfPath = $state<string | null>(null);
  let pdfHash = $state<string | null>(null);
  let chunksIndexed = $state<number | null>(null);
  let stats = $state<IndexStats | null>(null);
  let query = $state("");
  let scope = $state<"all" | "this">("all");
  let hits = $state<SearchHit[]>([]);
  let status = $state<Status>(idle);

  let searchInput: HTMLInputElement | null = null;

  onMount(() => {
    void refreshStats();
    // Pre-fill PDF from the global open-recent channel.
    const onOpenRecent = (e: Event) => {
      const d = (e as CustomEvent).detail as { path: string } | undefined;
      if (d?.path) {
        pdfPath = d.path;
        pdfHash = null;
        chunksIndexed = null;
      }
    };
    window.addEventListener("slab:open-recent", onOpenRecent);
    return () => window.removeEventListener("slab:open-recent", onOpenRecent);
  });

  async function refreshStats() {
    try {
      const res = await invoke<CmdResult<IndexStats>>("slab_beacon_index_stats");
      if (res.kind === "ok") stats = res.value;
    } catch {
      /* ignore — empty index is the default */
    }
  }

  async function pickPdf() {
    const picked = await open({
      multiple: false,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof picked !== "string") return;
    pdfPath = picked;
    pdfHash = null;
    chunksIndexed = null;
    status = idle;
  }

  async function indexPdf(force = false) {
    if (!pdfPath) {
      status = { kind: "err", msg: "Pick a PDF first." };
      return;
    }
    status = {
      kind: "working",
      msg: force ? "Re-indexing… embedding every chunk." : "Indexing… embedding every chunk.",
    };
    try {
      const res = await invoke<CmdResult<IndexReport>>("slab_beacon_index_pdf", {
        pdfPath,
        forceReindex: force,
      });
      if (res.kind === "ok") {
        pdfHash = res.value.pdf_hash;
        chunksIndexed = res.value.chunks_indexed;
        status = res.value.was_cached
          ? { kind: "ok", msg: "Already indexed — using cached vectors." }
          : { kind: "ok", msg: `Indexed ${res.value.chunks_indexed} chunks.` };
        await refreshStats();
      } else {
        status = { kind: "err", msg: friendlyError(res.message) };
      }
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }

  async function runSearch() {
    const q = query.trim();
    if (!q) return;
    status = { kind: "working", msg: "Searching…" };
    hits = [];
    try {
      const res = await invoke<CmdResult<SearchHit[]>>("slab_beacon_search", {
        query: q,
        topK: 12,
        onlyPdfHash: scope === "this" ? pdfHash : null,
      });
      if (res.kind === "ok") {
        hits = res.value;
        status =
          hits.length === 0
            ? { kind: "ok", msg: "No matches. Try a different phrasing or index more PDFs." }
            : { kind: "ok", msg: `${hits.length} hit${hits.length === 1 ? "" : "s"}.` };
      } else {
        status = { kind: "err", msg: friendlyError(res.message) };
      }
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }

  function onSearchKeydown(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      void runSearch();
    }
  }

  function gotoHit(h: SearchHit) {
    // Two events: ask the Reader to open the PDF, then jump page.
    window.dispatchEvent(
      new CustomEvent("slab:open-recent", { detail: { path: h.pdf_path } }),
    );
    window.dispatchEvent(
      new CustomEvent("slab:beacon-goto-page", { detail: { page: h.page } }),
    );
  }

  function scorePct(s: number): string {
    const pct = Math.max(0, Math.min(1, s)) * 100;
    return pct.toFixed(0) + "%";
  }

  function friendlyError(msg: string): string {
    const m = msg.toLowerCase();
    if (m.includes("provider unavailable")) {
      if (m.includes("missing api key")) return msg;
      return (
        "Embedding provider unavailable. " +
        "Start Ollama (ollama.com) or switch provider in settings."
      );
    }
    if (m.includes("provider returned no embedding")) {
      return "The provider didn't return a query embedding. Check the embed model is set.";
    }
    return msg;
  }
</script>

<section class="search">
  <header class="header">
    <div>
      <h2>
        ✦ Beacon Search
        <span class="beta-tag">beta</span>
      </h2>
      <p class="muted">
        Search by meaning, not just keywords. Local embeddings, never uploaded.
      </p>
    </div>
  </header>

  <div class="card">
    <div class="card-row">
      <label>
        <span class="lbl">PDF to index</span>
        <div class="picker">
          <input
            type="text"
            readonly
            value={pdfPath ? basename(pdfPath) : ""}
            placeholder="Pick a PDF to add to the index…"
          />
          <button class="secondary" onclick={pickPdf}>Browse</button>
        </div>
      </label>
    </div>
    <div class="card-row card-actions-row">
      <button
        class="primary"
        onclick={() => indexPdf(false)}
        disabled={!pdfPath || status.kind === "working"}
      >
        {status.kind === "working" && status.msg.startsWith("Indexing") ? "Indexing…" : "Index this PDF"}
      </button>
      <button
        class="secondary"
        onclick={() => indexPdf(true)}
        disabled={!pdfPath || status.kind === "working"}
        title="Force re-embedding even if this content is already cached"
      >
        Re-index
      </button>
      {#if chunksIndexed !== null}
        <span class="indexed-tag">
          {chunksIndexed === 0 ? "cached" : `${chunksIndexed} chunks added`}
        </span>
      {/if}
    </div>
  </div>

  <div class="search-bar">
    <input
      bind:this={searchInput}
      bind:value={query}
      onkeydown={onSearchKeydown}
      placeholder="Search across the index (Enter to run)"
      type="search"
    />
    <div class="scope-toggle" role="tablist">
      <button
        class:active={scope === "all"}
        onclick={() => (scope = "all")}
        role="tab"
        aria-selected={scope === "all"}
      >All</button>
      <button
        class:active={scope === "this"}
        onclick={() => (scope = "this")}
        disabled={!pdfHash}
        role="tab"
        aria-selected={scope === "this"}
        title={!pdfHash ? "Index a PDF first" : "Only search the currently-picked PDF"}
      >This PDF</button>
    </div>
    <button
      class="primary"
      onclick={runSearch}
      disabled={!query.trim() || status.kind === "working"}
    >
      Search
    </button>
  </div>

  {#if status.kind === "err"}
    <div class="status err">✕ {status.msg}</div>
  {:else if status.kind === "ok" && hits.length === 0}
    <div class="status done">{status.msg}</div>
  {:else if status.kind === "working"}
    <div class="status working">{status.msg}</div>
  {/if}

  {#if hits.length > 0}
    <ol class="hits">
      {#each hits as h, i (i)}
        <li>
          <button class="hit-card" onclick={() => gotoHit(h)}>
            <div class="hit-head">
              <span class="hit-page">page {h.page}</span>
              <span class="hit-path">{basename(h.pdf_path)}</span>
              <span class="hit-score">{scorePct(h.score)}</span>
            </div>
            <div class="hit-snippet">{h.text}</div>
          </button>
        </li>
      {/each}
    </ol>
  {/if}

  {#if stats && stats.chunks > 0}
    <footer class="stats">
      <span>{stats.pdfs} PDF{stats.pdfs === 1 ? "" : "s"}</span>
      <span class="dot">·</span>
      <span>{stats.chunks.toLocaleString()} chunks indexed</span>
    </footer>
  {:else if stats && stats.chunks === 0}
    <footer class="stats muted">
      Index is empty. Pick a PDF and click "Index this PDF" above.
    </footer>
  {/if}
</section>

<style>
  .search {
    display: flex;
    flex-direction: column;
    gap: 14px;
    min-height: 0;
    flex: 1;
  }
  .header h2 {
    margin: 0;
  }
  .header .muted {
    margin: 4px 0 0;
    color: var(--text-3);
    font-size: 13px;
  }
  .beta-tag {
    font-size: 10px;
    color: var(--accent);
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 1px 6px;
    margin-left: 8px;
    font-weight: 500;
    letter-spacing: 0.4px;
    text-transform: uppercase;
    vertical-align: middle;
  }

  .card {
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    padding: 14px 16px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .card-row {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .card-actions-row {
    flex-direction: row;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .lbl {
    font-size: 12px;
    color: var(--text-3);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }
  .picker {
    display: flex;
    gap: 6px;
  }
  .picker input {
    flex: 1;
  }
  .indexed-tag {
    font-size: 12px;
    color: var(--accent);
    background: var(--bg-3);
    border: 1px solid var(--border);
    border-radius: 12px;
    padding: 3px 10px;
  }

  .search-bar {
    display: flex;
    gap: 8px;
    align-items: stretch;
  }
  .search-bar input {
    flex: 1;
  }
  .scope-toggle {
    display: inline-flex;
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    overflow: hidden;
  }
  .scope-toggle button {
    background: transparent;
    border: 0;
    color: var(--text-2);
    padding: 0 12px;
    font-size: 12px;
    cursor: pointer;
  }
  .scope-toggle button.active {
    background: var(--bg-3);
    color: var(--accent);
  }
  .scope-toggle button:disabled {
    color: var(--text-3);
    cursor: not-allowed;
  }

  .status {
    padding: 8px 12px;
    border-radius: var(--r-sm);
    font-size: 13px;
  }
  .status.err {
    background: rgba(220, 38, 38, 0.08);
    color: #fca5a5;
    border: 1px solid rgba(220, 38, 38, 0.3);
  }
  .status.working {
    background: var(--bg-2);
    color: var(--text-2);
    border: 1px solid var(--border);
  }
  .status.done {
    background: var(--bg-2);
    color: var(--text-3);
    border: 1px solid var(--border);
  }

  .hits {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
    overflow-y: auto;
    flex: 1;
    min-height: 0;
  }
  .hit-card {
    width: 100%;
    text-align: left;
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    padding: 10px 12px;
    cursor: pointer;
    color: var(--text-1);
    transition: border-color 80ms ease, background 80ms ease;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .hit-card:hover {
    background: var(--bg-3);
    border-color: var(--accent);
  }
  .hit-head {
    display: flex;
    gap: 10px;
    align-items: baseline;
    font-size: 12px;
    color: var(--text-3);
  }
  .hit-page {
    color: var(--accent);
    font-weight: 600;
  }
  .hit-path {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .hit-score {
    font-variant-numeric: tabular-nums;
    color: var(--text-2);
  }
  .hit-snippet {
    font-size: 13px;
    line-height: 1.5;
    color: var(--text-2);
    /* Clamp to a few lines */
    display: -webkit-box;
    -webkit-line-clamp: 3;
    line-clamp: 3;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .stats {
    font-size: 12px;
    color: var(--text-3);
    display: flex;
    gap: 8px;
    padding-top: 4px;
    border-top: 1px solid var(--border);
  }
  .stats .dot {
    opacity: 0.5;
  }
  .muted {
    color: var(--text-3);
  }
</style>
