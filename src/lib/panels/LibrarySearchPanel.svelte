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
  import {
    clearLibrarySearchHistory,
    libraryIndexStats,
    listFolders,
    librarySearch,
    recentLibrarySearches,
    type FolderRecord,
    type LibraryIndexStats,
    type RecentSearch,
    type SearchHit,
  } from "$lib/library";
  import { basename } from "$lib/types";

  let query = $state("");
  let hits = $state<SearchHit[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let lastQuery = $state("");
  let inputEl: HTMLInputElement | null = $state(null);
  /** Newest-first rolling log of the user's prior searches. Refreshed
   *  after every successful run + on mount. */
  let recents = $state<RecentSearch[]>([]);
  /** Toggle for the "clear history" affordance shown when recents>0. */
  let clearing = $state(false);
  /** Every indexed folder; loaded once on mount. The picker is only shown
   *  when >1 folder exists (a one-folder library has nothing to scope). */
  let folders = $state<FolderRecord[]>([]);
  /** null = search every indexed folder; otherwise an existing folder id. */
  let scopeFolderId = $state<number | null>(null);
  let scopeFolder = $derived(
    scopeFolderId == null
      ? null
      : (folders.find((f) => f.id === scopeFolderId) ?? null),
  );
  /** FTS5 index size: distinct indexed docs + total indexed pages. Used by
   *  the status footer; refreshed on mount + after every search so a scan
   *  that lands while the user is browsing makes the counts grow live. */
  let indexStats = $state<LibraryIndexStats | null>(null);

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
      const res = await librarySearch(trimmed, 50, scopeFolderId);
      // Discard if a newer query already kicked off.
      if (trimmed !== query.trim()) return;
      hits = res;
      lastQuery = trimmed;
      // The backend writes to library_search_log inside `search()`; refresh
      // the chip strip so the just-run query bubbles to the head (or its
      // dedup-window update bumps an existing chip's resultCount).
      void refreshRecents();
      // A search is a cheap excuse to re-poll the index size — if a scan
      // landed mid-session the footer should grow without a panel remount.
      void refreshIndexStats();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
      hits = [];
    } finally {
      loading = false;
    }
  }

  async function refreshRecents(): Promise<void> {
    try {
      recents = await recentLibrarySearches(8);
    } catch {
      // Non-fatal — chips are an enhancement, not the panel's reason for being.
      recents = [];
    }
  }

  function runRecent(r: RecentSearch): void {
    if (debounceTimer) clearTimeout(debounceTimer);
    query = r.query;
    void runSearch(r.query);
    inputEl?.focus();
  }

  async function clearHistory(): Promise<void> {
    if (clearing || recents.length === 0) return;
    if (
      !window.confirm(
        `Clear ${recents.length} recent search${recents.length === 1 ? "" : "es"}?`,
      )
    )
      return;
    clearing = true;
    try {
      await clearLibrarySearchHistory();
      recents = [];
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      clearing = false;
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
    void refreshRecents();
    void refreshFolders();
    void refreshIndexStats();
  });

  async function refreshFolders(): Promise<void> {
    try {
      folders = await listFolders();
      // If the scoped folder vanished between sessions, fall back to All.
      if (scopeFolderId != null && !folders.some((f) => f.id === scopeFolderId)) {
        scopeFolderId = null;
      }
    } catch {
      folders = [];
    }
  }

  async function refreshIndexStats(): Promise<void> {
    try {
      indexStats = await libraryIndexStats();
    } catch {
      // Footer is a glance, not load-bearing — silent fallback to null
      // collapses the footer rather than spamming an error.
      indexStats = null;
    }
  }

  function onScopeChange(e: Event): void {
    const v = (e.target as HTMLSelectElement).value;
    const next = v === "" ? null : Number(v);
    if (next === scopeFolderId) return;
    scopeFolderId = next;
    // Re-run the active query against the new scope so the result list
    // refreshes immediately — no need for the user to press Enter again.
    if (query.trim()) {
      if (debounceTimer) clearTimeout(debounceTimer);
      void runSearch(query);
    }
  }

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
    {#if folders.length > 1}
      <div class="scope-row">
        <label class="scope-label" for="search-scope">Scope</label>
        <select
          id="search-scope"
          class="scope-select"
          value={scopeFolderId == null ? "" : String(scopeFolderId)}
          onchange={onScopeChange}
          title="Restrict search to one indexed folder"
        >
          <option value="">All folders ({folders.length})</option>
          {#each folders as f (f.id)}
            <option value={String(f.id)}>{basename(f.path) || f.path}</option>
          {/each}
        </select>
        {#if scopeFolder}
          <span class="scope-path" title={scopeFolder.path}
            >{scopeFolder.path}</span
          >
        {/if}
      </div>
    {/if}
    {#if loading}
      <div class="status">Searching…</div>
    {:else if lastQuery && hits.length > 0}
      <div class="status">
        {hits.length} match{hits.length === 1 ? "" : "es"} across {groups.length}
        document{groups.length === 1 ? "" : "s"} for
        <strong>"{lastQuery}"</strong>
        {#if scopeFolder}
          in <strong title={scopeFolder.path}
            >{basename(scopeFolder.path) || scopeFolder.path}</strong
          >
        {/if}
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
        {#if recents.length > 0}
          <section class="recents" aria-label="Recent searches">
            <header class="recents-head">
              <span class="recents-label">Recent searches</span>
              <button
                type="button"
                class="recents-clear"
                onclick={clearHistory}
                disabled={clearing}
                title="Clear your search history"
                aria-label="Clear recent searches"
              >
                {clearing ? "Clearing…" : "Clear history"}
              </button>
            </header>
            <ul class="recents-list">
              {#each recents as r (r.id)}
                <li>
                  <button
                    type="button"
                    class="recent-chip"
                    onclick={() => runRecent(r)}
                    title={r.resultCount > 0
                      ? `${r.resultCount} match${r.resultCount === 1 ? "" : "es"} last run`
                      : "No matches last run"}
                  >
                    <span class="recent-query">{r.query}</span>
                    <span class="recent-meta">{r.resultCount}</span>
                  </button>
                </li>
              {/each}
            </ul>
          </section>
        {/if}
        <ul class="tips">
          <li><kbd>Enter</kbd> — run search immediately</li>
          <li><kbd>Esc</kbd> — clear</li>
          <li>Last word becomes a prefix match automatically</li>
          <li>
            Wrap a phrase in <kbd>"</kbd>quotes<kbd>"</kbd> to match adjacent
            words only
          </li>
          <li>
            Prefix a term with <kbd>-</kbd> to exclude it
            (<code>contract -draft</code>)
          </li>
        </ul>
      </div>
    {:else if !loading && hits.length === 0}
      <div class="state empty">
        <h2>No matches for "{lastQuery}"</h2>
        <p>
          {#if scopeFolder}
            Nothing in <strong title={scopeFolder.path}
              >{basename(scopeFolder.path) || scopeFolder.path}</strong
            > matches that query. Try shorter words, switch the scope back to
            All folders, or check that you've added the folder you expect this
            PDF to live in.
          {:else}
            Nothing in your indexed library matches that query. Try shorter
            words, a different phrase, or check that you've added the folder
            you expect this PDF to live in.
          {/if}
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
  {#if indexStats && (indexStats.docs > 0 || indexStats.pages > 0)}
    <footer
      class="index-footer"
      aria-label="Indexed library size"
      title="Distinct docs and total pages in the FTS5 full-text index"
    >
      <span class="footer-dot" aria-hidden="true">●</span>
      <span>
        <strong>{indexStats.docs.toLocaleString()}</strong>
        doc{indexStats.docs === 1 ? "" : "s"} /
        <strong>{indexStats.pages.toLocaleString()}</strong>
        page{indexStats.pages === 1 ? "" : "s"} indexed
      </span>
    </footer>
  {/if}
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

  /* Folder-scope picker — appears between the search input and the status
     line whenever the library has >1 indexed folder. The picker uses the
     native <select> so platform chrome (macOS focus ring, Windows arrow)
     stays consistent, but is styled to match the rest of the panel. */
  .scope-row {
    margin-top: 10px;
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .scope-label {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--fg-muted, #888);
    font-weight: 600;
  }
  .scope-select {
    font: inherit;
    font-size: 12px;
    padding: 4px 8px;
    border: 1px solid var(--border, rgba(0, 0, 0, 0.12));
    border-radius: 6px;
    background: var(--bg-input, #fff);
    color: var(--fg, #111);
    cursor: pointer;
    max-width: 240px;
  }
  .scope-select:focus {
    outline: none;
    border-color: var(--accent, #4a72ff);
    box-shadow: 0 0 0 3px var(--accent-fade, rgba(74, 114, 255, 0.15));
  }
  .scope-path {
    font-size: 11px;
    color: var(--fg-muted, #888);
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    direction: rtl;
    text-align: left;
  }

  .results {
    flex: 1;
    overflow-y: auto;
    padding: 16px 32px 32px;
  }

  /* Status footer pinned to the bottom of the panel — never scrolls with
     results so the live index-size is always visible. The dot mirrors the
     suggestion-engine status indicator language used elsewhere in Slab
     (LibraryPanel's "indexed" pip). Tinted accent green when the index
     has content, muted when empty (which collapses entirely via the
     {#if} guard). */
  .index-footer {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 32px;
    border-top: 1px solid var(--border, rgba(0, 0, 0, 0.08));
    background: var(--bg-panel, #fff);
    font-size: 11px;
    color: var(--fg-muted, #888);
    flex-shrink: 0;
  }
  .footer-dot {
    color: var(--success, #22c55e);
    font-size: 9px;
    line-height: 1;
  }
  .index-footer strong {
    color: var(--fg, #222);
    font-weight: 600;
    font-variant-numeric: tabular-nums;
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
  .tips code {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 11px;
    padding: 1px 6px;
    border-radius: 4px;
    background: var(--bg-subtle, rgba(0, 0, 0, 0.05));
    color: var(--fg, #222);
  }
  .state.error {
    color: var(--danger, #c0392b);
  }

  /* Recent-searches chip strip — surfaces the rolling library_search_log
     as one-click re-runnable chips above the tips. Each chip wears its
     last result count so a "0" tells the user that query stopped finding
     anything (eg. they re-indexed and the doc no longer matches). */
  .recents {
    margin: 8px auto 18px;
    text-align: left;
  }
  .recents-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 6px;
  }
  .recents-label {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--fg-muted, #888);
    font-weight: 600;
  }
  .recents-clear {
    background: transparent;
    border: none;
    font: inherit;
    font-size: 11px;
    color: var(--fg-muted, #888);
    cursor: pointer;
    padding: 2px 6px;
    border-radius: 4px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .recents-clear:hover:not(:disabled) {
    color: var(--fg, #111);
    background: var(--bg-subtle, rgba(0, 0, 0, 0.05));
  }
  .recents-clear:disabled {
    opacity: 0.5;
    cursor: progress;
  }
  .recents-list {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .recent-chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 10px;
    background: var(--bg-panel, #fff);
    border: 1px solid var(--border, rgba(0, 0, 0, 0.08));
    border-radius: 999px;
    font: inherit;
    font-size: 12px;
    color: var(--fg, #222);
    cursor: pointer;
    transition: border-color 80ms, background 80ms, color 80ms;
    max-width: 320px;
  }
  .recent-chip:hover {
    border-color: var(--accent, #4a72ff);
    background: var(--bg-hover, rgba(74, 114, 255, 0.06));
    color: var(--fg, #111);
  }
  .recent-query {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 240px;
  }
  .recent-meta {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 10px;
    color: var(--fg-muted, #888);
    padding: 1px 6px;
    border-radius: 999px;
    background: var(--bg-subtle, rgba(0, 0, 0, 0.05));
    flex-shrink: 0;
  }

  @media (prefers-color-scheme: dark) {
    .search-panel {
      background: var(--bg-app, #1a1a1a);
      color: var(--fg, #eee);
    }
    .search-header,
    .index-footer,
    .hit-btn {
      background: var(--bg-panel, #222);
    }
    .index-footer strong {
      color: var(--fg, #eee);
    }
    .recent-chip {
      background: var(--bg-panel, #222);
      color: var(--fg, #ddd);
    }
    .scope-select {
      background: var(--bg-input, #2a2a2a);
      color: var(--fg, #eee);
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
