<script lang="ts">
  // LibrarySearchPanel — v2.2.0 "Atlas" Slice 3.
  //
  // Cross-document full-text search over the FTS5 index built by the
  // scanner (Slice 1). The user types a natural-language query, we
  // debounce 180ms, fire `slab_library_search`, and render the hits
  // grouped by document with `<mark>`-wrapped snippets. Click a hit →
  // open that PDF in a new Reader tab at the matching page (1-based).
  //
  // Why this matters (Buy-Button #1 + #4):
  //   - Adobe + PDF Expert charge for "search across your library"
  //     (and ship indexes to the cloud). We do it offline, free,
  //     instant, with bm25 ranking + locality-preserving snippets.
  //   - The wow moment: paralegal drops 200 contracts into a folder,
  //     types "termination clause", gets every page with a yellow
  //     highlight in <100ms.
  //
  // Design notes:
  //   - All state local; no global stores.
  //   - We render snippets as HTML *only* because the backend wraps
  //     each match in `<mark>…</mark>` (FTS5 snippet()). The text
  //     content itself is PDF-extracted (user's own files) — we do not
  //     accept network data here. Still, we sanitise by allowing only
  //     `<mark>` and `</mark>` tags before injecting.
  //   - Empty / error / no-results states all explicitly designed.

  import { onMount } from "svelte";
  import { librarySearch, type SearchHit } from "$lib/library";
  import { basename } from "$lib/types";

  let query = $state("");
  let hits = $state<SearchHit[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let lastQuery = $state("");
  let inputEl: HTMLInputElement | null = $state(null);

  // 180ms debounce keeps the FTS5 query rate sane while feeling instant.
  let debounceTimer: ReturnType<typeof setTimeout> | null = null;

  /** Sanitise the snippet so only `<mark>` survives. */
  function safeSnippet(s: string): string {
    // Escape every < then re-introduce the two tags we explicitly trust.
    return s
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/&lt;mark&gt;/g, "<mark>")
      .replace(/&lt;\/mark&gt;/g, "</mark>");
  }

  async function runSearch(q: string): Promise<void> {
    const trimmed = q.trim();
    if (!trimmed) {
      hits = [];
      error = null;
      lastQuery = "";
      return;
    }
    loading = true;
    error = null;
    try {
      const res = await librarySearch(trimmed, 50, null);
      // Discard if a newer query already kicked off.
      if (trimmed !== query.trim()) return;
      hits = res;
      lastQuery = trimmed;
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
      hits = [];
    } finally {
      loading = false;
    }
  }

  function onInput(): void {
    if (debounceTimer) clearTimeout(debounceTimer);
    const q = query;
    debounceTimer = setTimeout(() => void runSearch(q), 180);
  }

  function onKey(e: KeyboardEvent): void {
    if (e.key === "Enter") {
      e.preventDefault();
      if (debounceTimer) clearTimeout(debounceTimer);
      void runSearch(query);
    } else if (e.key === "Escape") {
      query = "";
      hits = [];
      error = null;
    }
  }

  /** Open a hit: page_index is 0-based in the backend, ReaderPanel/openNewTab
   *  is path-based and the tab will jump to its last-saved page. We forward
   *  via the same `slab:open-library-doc` event LibraryPanel uses, with an
   *  optional `page` (1-based) and the raw `query` so the Reader can run
   *  pdfjs find() with `highlightAll: true` once the doc loads — every
   *  occurrence glows yellow, the matched page gets a 720 ms gold halo. */
  function openHit(h: SearchHit): void {
    window.dispatchEvent(
      new CustomEvent("slab:open-library-doc", {
        detail: {
          path: h.path,
          page: h.pageIndex + 1,
          highlight: query.trim(),
        },
      }),
    );
  }

  // Group hits by doc for compact display: doc header + N page-snippets.
  interface DocGroup {
    docId: number;
    path: string;
    title: string;
    hits: SearchHit[];
  }
  const groups: DocGroup[] = $derived.by(() => {
    const map = new Map<number, DocGroup>();
    for (const h of hits) {
      let g = map.get(h.docId);
      if (!g) {
        g = {
          docId: h.docId,
          path: h.path,
          title: h.title || basename(h.path),
          hits: [],
        };
        map.set(h.docId, g);
      }
      g.hits.push(h);
    }
    return Array.from(map.values());
  });

  onMount(() => {
    inputEl?.focus();
  });

  // Public API: when the keyboard shortcut activates the panel, the
  // route already sets `active = "library-search"`. We expose a method
  // via window event for the route to call to refocus the input on
  // re-activation.
  function focusInput(): void {
    inputEl?.focus();
    inputEl?.select();
  }

  onMount(() => {
    const handler = () => focusInput();
    window.addEventListener("slab:focus-library-search", handler);
    return () => {
      window.removeEventListener("slab:focus-library-search", handler);
    };
  });
</script>

<section class="search-panel">
  <header class="search-header">
    <div class="title-row">
      <h1>Search across your library</h1>
      <span class="kbd-hint" title="Cross-document full-text search (Cmd+Shift+F)"
        >⌘⇧F</span
      >
    </div>
    <p class="subtitle">
      Full-text search across every indexed PDF — 100% offline, ranked by bm25
      relevance.
    </p>
    <div class="search-input-wrap">
      <span class="search-icon" aria-hidden="true">⌕</span>
      <input
        bind:this={inputEl}
        bind:value={query}
        oninput={onInput}
        onkeydown={onKey}
        placeholder="Search across all your PDFs…"
        class="search-input"
        aria-label="Search library"
        autocomplete="off"
        spellcheck="false"
      />
      {#if query}
        <button
          type="button"
          class="clear-btn"
          aria-label="Clear search"
          onclick={() => {
            query = "";
            hits = [];
            error = null;
            inputEl?.focus();
          }}>×</button
        >
      {/if}
    </div>
    {#if loading}
      <div class="status">Searching…</div>
    {:else if lastQuery && hits.length > 0}
      <div class="status">
        {hits.length} match{hits.length === 1 ? "" : "es"} across {groups.length}
        document{groups.length === 1 ? "" : "s"} for
        <strong>"{lastQuery}"</strong>
      </div>
    {/if}
  </header>

  <div class="results">
    {#if error}
      <div class="state error">
        <strong>Search failed.</strong>
        <p>{error}</p>
      </div>
    {:else if !query.trim()}
      <div class="state empty">
        <div class="empty-icon">⌕</div>
        <h2>Search every PDF you've added to Slab</h2>
        <p>
          Add folders in the Library panel, then come back here to search
          inside them all at once. Tries phrases, partial words, and ranks by
          relevance. Adobe Acrobat charges $239/yr for this — Slab keeps it
          free and local.
        </p>
        <ul class="tips">
          <li><kbd>Enter</kbd> — run search immediately</li>
          <li><kbd>Esc</kbd> — clear</li>
          <li>Last word becomes a prefix match automatically</li>
        </ul>
      </div>
    {:else if !loading && hits.length === 0}
      <div class="state empty">
        <h2>No matches for "{lastQuery}"</h2>
        <p>
          Nothing in your indexed library matches that query. Try shorter
          words, a different phrase, or check that you've added the folder
          you expect this PDF to live in.
        </p>
      </div>
    {:else}
      {#each groups as g (g.docId)}
        <article class="group">
          <header class="group-header">
            <h3 class="doc-title">{g.title}</h3>
            <span class="doc-path" title={g.path}>{g.path}</span>
            <span class="hit-count"
              >{g.hits.length} hit{g.hits.length === 1 ? "" : "s"}</span
            >
          </header>
          <ul class="hit-list">
            {#each g.hits as h, i (`${h.docId}-${h.pageIndex}-${i}`)}
              <li class="hit">
                <button
                  type="button"
                  class="hit-btn"
                  onclick={() => openHit(h)}
                  title="Open at page {h.pageIndex + 1}"
                >
                  <span class="page-badge">p. {h.pageIndex + 1}</span>
                  <span class="snippet">{@html safeSnippet(h.snippet)}</span>
                </button>
              </li>
            {/each}
          </ul>
        </article>
      {/each}
    {/if}
  </div>
</section>

<style>
  .search-panel {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
    background: var(--bg-app, #fafafa);
    color: var(--fg, #111);
  }

  .search-header {
    padding: 24px 32px 16px;
    border-bottom: 1px solid var(--border, rgba(0, 0, 0, 0.08));
    background: var(--bg-panel, #fff);
  }

  .title-row {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
  }

  .search-header h1 {
    font-size: 18px;
    font-weight: 600;
    margin: 0 0 4px 0;
    letter-spacing: -0.01em;
  }

  .kbd-hint {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 11px;
    padding: 2px 6px;
    border-radius: 4px;
    background: var(--bg-subtle, rgba(0, 0, 0, 0.05));
    color: var(--fg-muted, #666);
  }

  .subtitle {
    font-size: 13px;
    color: var(--fg-muted, #666);
    margin: 0 0 16px 0;
  }

  .search-input-wrap {
    position: relative;
    display: flex;
    align-items: center;
  }

  .search-icon {
    position: absolute;
    left: 14px;
    font-size: 16px;
    color: var(--fg-muted, #888);
    pointer-events: none;
  }

  .search-input {
    width: 100%;
    padding: 12px 40px 12px 38px;
    font-size: 15px;
    border: 1px solid var(--border, rgba(0, 0, 0, 0.12));
    border-radius: 10px;
    background: var(--bg-input, #fff);
    color: var(--fg, #111);
    outline: none;
    transition: border-color 100ms, box-shadow 100ms;
  }

  .search-input:focus {
    border-color: var(--accent, #4a72ff);
    box-shadow: 0 0 0 3px var(--accent-fade, rgba(74, 114, 255, 0.15));
  }

  .clear-btn {
    position: absolute;
    right: 8px;
    width: 26px;
    height: 26px;
    border: none;
    background: transparent;
    color: var(--fg-muted, #888);
    font-size: 20px;
    line-height: 1;
    cursor: pointer;
    border-radius: 6px;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .clear-btn:hover {
    background: var(--bg-subtle, rgba(0, 0, 0, 0.05));
    color: var(--fg, #111);
  }

  .status {
    margin-top: 10px;
    font-size: 12px;
    color: var(--fg-muted, #666);
  }

  .results {
    flex: 1;
    overflow-y: auto;
    padding: 16px 32px 32px;
  }

  .group {
    margin-bottom: 28px;
  }

  .group-header {
    display: flex;
    align-items: baseline;
    gap: 10px;
    margin-bottom: 8px;
    padding-bottom: 6px;
    border-bottom: 1px solid var(--border, rgba(0, 0, 0, 0.06));
  }

  .doc-title {
    font-size: 14px;
    font-weight: 600;
    margin: 0;
    color: var(--fg, #111);
  }

  .doc-path {
    font-size: 11px;
    color: var(--fg-muted, #888);
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    direction: rtl;
    text-align: left;
  }

  .hit-count {
    font-size: 11px;
    color: var(--fg-muted, #888);
    padding: 1px 8px;
    border-radius: 999px;
    background: var(--bg-subtle, rgba(0, 0, 0, 0.05));
  }

  .hit-list {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .hit-btn {
    display: flex;
    width: 100%;
    text-align: left;
    gap: 12px;
    padding: 10px 12px;
    background: var(--bg-panel, #fff);
    border: 1px solid var(--border, rgba(0, 0, 0, 0.06));
    border-radius: 8px;
    cursor: pointer;
    transition: background 80ms, border-color 80ms;
    font: inherit;
    color: inherit;
  }
  .hit-btn:hover {
    background: var(--bg-hover, rgba(74, 114, 255, 0.06));
    border-color: var(--accent, #4a72ff);
  }

  .page-badge {
    flex-shrink: 0;
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 11px;
    color: var(--fg-muted, #666);
    padding: 2px 7px;
    border-radius: 999px;
    background: var(--bg-subtle, rgba(0, 0, 0, 0.05));
    align-self: flex-start;
    margin-top: 1px;
  }

  .snippet {
    font-size: 13px;
    line-height: 1.5;
    color: var(--fg, #222);
    overflow-wrap: anywhere;
  }

  /* The <mark> tag comes through from the FTS5 snippet() output. */
  .snippet :global(mark) {
    background: var(--mark-bg, #fff3a3);
    color: inherit;
    padding: 0 2px;
    border-radius: 2px;
    font-weight: 600;
  }

  .state {
    max-width: 560px;
    margin: 48px auto;
    text-align: center;
    color: var(--fg-muted, #666);
  }
  .state h2 {
    font-size: 16px;
    font-weight: 600;
    color: var(--fg, #111);
    margin: 0 0 8px 0;
  }
  .state p {
    font-size: 13px;
    line-height: 1.6;
    margin: 0 0 16px 0;
  }
  .empty-icon {
    font-size: 48px;
    color: var(--fg-muted, #ccc);
    margin-bottom: 12px;
  }
  .tips {
    list-style: none;
    padding: 0;
    margin: 16px 0 0 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: 12px;
  }
  .tips kbd {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 11px;
    padding: 1px 6px;
    border-radius: 4px;
    background: var(--bg-subtle, rgba(0, 0, 0, 0.05));
    border: 1px solid var(--border, rgba(0, 0, 0, 0.08));
    margin-right: 4px;
  }
  .state.error {
    color: var(--danger, #c0392b);
  }

  @media (prefers-color-scheme: dark) {
    .search-panel {
      background: var(--bg-app, #1a1a1a);
      color: var(--fg, #eee);
    }
    .search-header,
    .hit-btn {
      background: var(--bg-panel, #222);
    }
    .search-input {
      background: var(--bg-input, #2a2a2a);
      color: var(--fg, #eee);
    }
    .snippet :global(mark) {
      background: var(--mark-bg, #6b5e1f);
      color: #fff;
    }
  }
</style>
