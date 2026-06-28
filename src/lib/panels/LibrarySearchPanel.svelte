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
    deleteLibrarySearch,
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
  import {
    classifySearchResultKey,
    flattenSearchHits,
    flatSearchHitCount,
    searchGroupStarts,
    classifyPaletteGroupNav,
    nextGroupIndex,
    nextSearchCursor,
    clampSearchCursor,
    interpretSearchQuery,
    describeQueryInterpretation,
    refineSearchHits,
    sortSearchGroups,
    searchSortLabel,
    summarizeSearchResults,
    pageSpread,
    buildSnippetSpans,
    classifyRecentChipKey,
    nextChipCursor,
    clampChipCursor,
    formatRelativeAge,
    SEARCH_SORT_MODES,
    type SearchSortMode,
    type SearchGroupLike,
  } from "$lib/librarySearchView";

  let query = $state("");
  let hits = $state<SearchHit[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let lastQuery = $state("");
  let inputEl: HTMLInputElement | null = $state(null);
  /** Newest-first rolling log of the user's prior searches. Refreshed
   *  after every successful run + on mount. */
  let recents = $state<RecentSearch[]>([]);
  /** Slice 6: flat horizontal cursor over the recent-search chip strip
   *  (-1 = none focused). Left/Right walk it, Enter runs the chip. Lives
   *  only in the empty-query state where the strip is shown. */
  let chipCursor = $state(-1);
  let chipEls = $state<(HTMLButtonElement | null)[]>([]);
  /** Reactive "now" (unix seconds) for the per-chip relative-age suffix.
   *  Ticks once a minute — the ages are coarse (m/h/d/w) so a minute is
   *  plenty fresh, and an empty-query strip is the only place it's read. */
  let nowSec = $state(Math.floor(Date.now() / 1000));
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
      // A fresh result set starts unrefined — a stale refine from the
      // previous query would silently hide rows of the new one.
      refine = "";
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
    chipCursor = -1; // leaving the empty-query strip; park the chip cursor
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

  /** Slice (this round): delete ONE recent-search chip — the per-chip x
      button or Backspace/Delete on the focused chip — complementing the
      all-or-nothing Clear history. Optimistically drops the row locally
      and keeps the chip cursor anchored on the same slot (so a Backspace
      run can delete several in a row), reconciling the cursor into the
      shrunken strip. Reverts the optimistic drop if the backend rejects. */
  async function deleteRecent(r: RecentSearch): Promise<void> {
    const prev = recents;
    const idx = recents.findIndex((x) => x.id === r.id);
    if (idx < 0) return;
    recents = recents.filter((x) => x.id !== r.id);
    // Keep focus on the slot the deleted chip occupied (now the next chip),
    // clamped into the shrunken strip; -1 when the strip is now empty.
    chipCursor = clampChipCursor(idx, recents.length);
    try {
      await deleteLibrarySearch(r.id);
      if (recents.length === 0) inputEl?.focus();
      else chipEls[chipCursor]?.focus();
    } catch (e) {
      // Backend rejected — restore the chip so the UI never lies.
      recents = prev;
      chipCursor = clampChipCursor(idx, recents.length);
      error = e instanceof Error ? e.message : String(e);
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
  // ---- Slice 3 (Atlas III): in-results refine ----
  // Narrow the already-returned hits client-side, instantly, without a
  // round-trip to the FTS index — so a broad query's wall of results can
  // be cut down without losing your scroll position or re-ranking.
  let refine = $state("");
  let refineEl = $state<HTMLInputElement | null>(null);
  const refinedHits = $derived(refineSearchHits(hits, refine));
  const rawGroups: DocGroup[] = $derived.by(() => {
    const map = new Map<number, DocGroup>();
    for (const h of refinedHits) {
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

  // ---- Slice 4 (Atlas III): result sort modes ----
  // Reorder the document groups: Relevance (bm25 arrival order, default),
  // Document (title A->Z), or Matches (hit count, biggest first). Stable,
  // so equal groups never jitter. The cursor flat-index space is rebuilt
  // off this sorted order so keyboard nav follows what the eye sees.
  let sortMode = $state<SearchSortMode>("relevance");
  const groups = $derived(
    sortSearchGroups(rawGroups as SearchGroupLike<SearchHit>[], sortMode) as DocGroup[],
  );

  // ---- Slice 5 (Atlas III): result summary footer ----
  // A context-aware footer that narrates the live result view (matches
  // shown vs total, the refine term, and the active sort) the way the
  // command palette and beacon inspector footers do. Replaces the static
  // index-size footer with something that tracks the current query.
  const resultSummary = $derived(
    summarizeSearchResults({
      shown: refinedHits.length,
      docs: groups.length,
      total: hits.length,
      refine,
      sortMode,
    }),
  );

  // ---- Slice 1 (Atlas III): keyboard navigation through results ----
  // A flat cursor walks every hit across the grouped render. The arrow /
  // Home/End / paging math reuses the tested palette nav core via
  // librarySearchView so behaviour matches ⌘K and the beacon inspector.
  /** Active cursor row in the flat hit space, -1 = none parked. */
  let cursor = $state(-1);
  /** Bound <button> refs for each flat hit, for scroll-into-view + focus. */
  let hitEls = $state<(HTMLButtonElement | null)[]>([]);
  /** Every hit flattened into one cursor index space, in render order. */
  const flatHits = $derived(flattenSearchHits(groups as SearchGroupLike<SearchHit>[]));
  const flatCount = $derived(flatSearchHitCount(groups as SearchGroupLike<SearchHit>[]));
  /** Flat index of each document group's first hit, for the Cmd+↑/↓ jump. */
  const groupStarts = $derived(searchGroupStarts(groups as SearchGroupLike<SearchHit>[]));

  // ---- Slice 2 (Atlas III): live query-interpretation preview ----
  // Mirror the FTS backend's lexer so the user can SEE how their query
  // will be matched — which word is a prefix, what's a phrase, what's
  // excluded, and the "only exclusions returns nothing" trap. Recomputes
  // on every keystroke off the raw (untrimmed) query.
  const interp = $derived(interpretSearchQuery(query));
  const interpAria = $derived(describeQueryInterpretation(interp));

  /** Tooltip explaining a token chip's match behaviour. */
  function tokenHint(kind: string): string {
    switch (kind) {
      case "prefix":
        return "Prefix match — matches words that START with this";
      case "term":
        return "Exact word match";
      case "phrase":
        return "Phrase — these words must appear together, in order";
      case "exclude":
        return "Excluded — pages containing this are dropped";
      default:
        return "";
    }
  }

  /** The flat index of the first hit in a given group (for cursor-ring math). */
  function groupFlatStart(groupIndex: number): number {
    let n = 0;
    for (let i = 0; i < groupIndex; i++) n += groups[i].hits.length;
    return n;
  }

  // Re-clamp the cursor whenever the result set changes (new search,
  // scope switch). A parked cursor past the new end snaps back into range;
  // an empty list parks it at -1 (no ring) rather than 0.
  $effect(() => {
    void flatCount;
    if (flatCount === 0) {
      cursor = -1;
    } else if (cursor >= flatCount) {
      cursor = clampSearchCursor(cursor, flatCount);
    }
  });

  // Slice 6: keep the chip cursor in range as the recents strip refreshes
  // (a run bubbles a query to the head; clearing empties it). A stale
  // cursor past the new end snaps back; an empty strip parks it at -1.
  $effect(() => {
    chipCursor = clampChipCursor(chipCursor, recents.length);
  });

  function moveCursor(intent: Parameters<typeof nextSearchCursor>[0]): void {
    if (flatCount === 0) return;
    const start = cursor < 0 ? 0 : cursor;
    cursor = intent === "first" || intent === "last" || cursor >= 0
      ? nextSearchCursor(intent, start, flatCount)
      : start; // first arrow press lands on row 0 without skipping it
    const el = hitEls[cursor];
    el?.scrollIntoView({ block: "nearest" });
  }

  /** Which text input (if any) currently owns the keypress. */
  function keyTarget(e: KeyboardEvent): "search" | "refine" | null {
    const t = e.target as HTMLElement | null;
    if (t === inputEl) return "search";
    if (t === refineEl) return "refine";
    return null;
  }

  /** Window-level keydown: drive the results cursor when the list has hits. */
  function onResultsKey(e: KeyboardEvent): void {
    // Slice 6: in the empty-query state the recent-search chip strip owns
    // the keyboard — Left/Right walk it, Enter runs the focused chip,
    // Escape parks it. This can't collide with the results cursor (there
    // are no results to walk when the query is empty). Only active when
    // the strip is actually shown.
    if (!query.trim() && recents.length > 0) {
      const chipAction = classifyRecentChipKey(e);
      if (chipAction) {
        if (chipAction.kind === "move") {
          // Don't steal Home/End from the search input's caret unless a
          // chip is already focused; Left/Right are free (the empty input
          // has no horizontal caret meaning worth preserving here).
          const fromInput = (e.target as HTMLElement | null) === inputEl;
          if (fromInput && chipCursor < 0 && (e.key === "Home" || e.key === "End")) return;
          e.preventDefault();
          const start = chipCursor < 0 ? 0 : chipCursor;
          chipCursor =
            chipCursor < 0 && (e.key === "ArrowLeft" || e.key === "ArrowRight")
              ? e.key === "ArrowRight"
                ? 0
                : recents.length - 1
              : nextChipCursor(chipAction.intent, start, recents.length);
          chipEls[chipCursor]?.scrollIntoView({ block: "nearest", inline: "nearest" });
          chipEls[chipCursor]?.focus();
          return;
        }
        if (chipAction.kind === "run" && chipCursor >= 0 && chipCursor < recents.length) {
          e.preventDefault();
          runRecent(recents[chipCursor]);
          return;
        }
        if (chipAction.kind === "delete" && chipCursor >= 0 && chipCursor < recents.length) {
          // Backspace / Delete on the focused chip drops just that one
          // recent search (the per-chip x button's keyboard twin).
          e.preventDefault();
          void deleteRecent(recents[chipCursor]);
          return;
        }
        if (chipAction.kind === "clear" && chipCursor >= 0) {
          e.preventDefault();
          chipCursor = -1;
          inputEl?.focus();
          return;
        }
      }
    }
    // Only when there are results to walk; let the inputs own typing.
    if (flatCount === 0) return;
    // Cmd/Ctrl+Up/Down leaps between document groups (Linear/Finder-style).
    // Checked BEFORE classifySearchResultKey, which bails on any modifier,
    // and works from the search/refine boxes too (the chord has no caret
    // meaning there). Reuses the palette group-jump core via the view-core.
    const groupIntent = classifyPaletteGroupNav(e);
    if (groupIntent) {
      e.preventDefault();
      const start = cursor < 0 ? 0 : cursor;
      cursor = nextGroupIndex(groupStarts, start, groupIntent, flatCount);
      hitEls[cursor]?.scrollIntoView({ block: "nearest" });
      return;
    }
    const action = classifySearchResultKey(e);
    if (!action) return;
    const where = keyTarget(e);
    if (action.kind === "move") {
      // Up/Down drive the cursor from anywhere (incl. the search / refine
      // boxes, Raycast-style). Inside a text box, leave Home/End/PageUp/
      // Down to their normal caret meaning; on a focused row, all nav
      // keys work.
      if (where && action.intent !== "next" && action.intent !== "prev") return;
      e.preventDefault();
      moveCursor(action.intent);
    } else if (action.kind === "open") {
      // Enter in the search box belongs to runSearch; elsewhere it opens
      // the focused hit.
      if (where === "search") return;
      if (cursor >= 0 && cursor < flatHits.length) {
        e.preventDefault();
        openHit(flatHits[cursor].hit);
      }
    } else if (action.kind === "clear") {
      // Escape ownership: the search box clears the query (its own
      // handler), the refine box clears the refine, otherwise a parked
      // cursor clears itself.
      if (where === "search") return;
      if (where === "refine") {
        if (refine) {
          e.preventDefault();
          refine = "";
        }
        return;
      }
      if (cursor >= 0) {
        e.preventDefault();
        cursor = -1;
      }
    }
  }

  onMount(() => {
    inputEl?.focus();
    void refreshRecents();
    void refreshFolders();
    void refreshIndexStats();
    // Tick the relative-age clock once a minute so chip ages ("2m", "1h")
    // stay current without a per-second timer (the units are coarse).
    const ageTimer = setInterval(() => {
      nowSec = Math.floor(Date.now() / 1000);
    }, 60_000);
    return () => clearInterval(ageTimer);
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

<svelte:window on:keydown={onResultsKey} />

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
    {#if !interp.empty && interp.tokens.length > 0}
      <div
        class="interp"
        class:warn={interp.noAnchor}
        aria-label={interpAria}
        title={interpAria}
      >
        {#if interp.noAnchor}
          <span class="interp-warn-icon" aria-hidden="true">!</span>
          <span class="interp-warn-text"
            >Only exclusions — add a word to search for</span
          >
        {:else}
          <span class="interp-lead" aria-hidden="true">Matching</span>
          {#each interp.tokens as t, ti (`${t.kind}-${t.text}-${ti}`)}
            <span class="interp-chip interp-{t.kind}" title={tokenHint(t.kind)}>
              {#if t.kind === "exclude"}<span class="interp-minus" aria-hidden="true"
                  >−</span
                >{/if}<span class="interp-text">{t.text}</span>
              {#if t.kind === "prefix"}<span class="interp-glob" aria-hidden="true"
                  >*</span
                >{/if}
            </span>
          {/each}
        {/if}
      </div>
    {/if}
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
            <ul class="recents-list" role="listbox" aria-label="Recent searches — arrow keys to navigate, Enter to run, Backspace to delete">
              {#each recents as r, ci (r.id)}
                <li role="presentation" class="recent-chip-wrap">
                  <button
                    bind:this={chipEls[ci]}
                    type="button"
                    class="recent-chip"
                    class:cursor={ci === chipCursor}
                    role="option"
                    aria-selected={ci === chipCursor}
                    onclick={() => runRecent(r)}
                    onmouseenter={() => (chipCursor = ci)}
                    title={r.resultCount > 0
                      ? `${r.resultCount} match${r.resultCount === 1 ? "" : "es"} last run`
                      : "No matches last run"}
                  >
                    <span class="recent-query">{r.query}</span>
                    <span class="recent-age">{formatRelativeAge(r.ts, nowSec)}</span>
                    <span class="recent-meta">{r.resultCount}</span>
                  </button>
                  <button
                    type="button"
                    class="recent-chip-del"
                    onclick={(e) => {
                      e.stopPropagation();
                      void deleteRecent(r);
                    }}
                    title="Remove this search"
                    aria-label={`Remove "${r.query}" from recent searches`}
                  >
                    <svg viewBox="0 0 16 16" width="11" height="11" aria-hidden="true">
                      <path
                        d="M4 4l8 8M12 4l-8 8"
                        stroke="currentColor"
                        stroke-width="1.6"
                        stroke-linecap="round"
                        fill="none"
                      />
                    </svg>
                  </button>
                </li>
              {/each}
            </ul>
            <p class="recents-hint" aria-hidden="true">
              <kbd>←</kbd><kbd>→</kbd> to browse · <kbd>Enter</kbd> to run · <kbd>⌫</kbd> to remove
            </p>
          </section>
        {/if}
        <ul class="tips">
          <li><kbd>Enter</kbd> — run search immediately</li>
          <li><kbd>Esc</kbd> — clear</li>
          <li><kbd>⌘</kbd><kbd>↑</kbd>/<kbd>↓</kbd> — jump between documents in results</li>
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
      <div class="refine-bar">
        <span class="refine-icon" aria-hidden="true">⌕</span>
        <input
          bind:this={refineEl}
          bind:value={refine}
          class="refine-input"
          type="text"
          placeholder="Refine these results…"
          aria-label="Refine results"
          autocomplete="off"
          spellcheck="false"
        />
        {#if refine}
          <button
            type="button"
            class="refine-clear"
            aria-label="Clear refine"
            onclick={() => {
              refine = "";
              refineEl?.focus();
            }}>×</button
          >
        {/if}
        <div class="sort-seg" role="group" aria-label="Sort results">
          {#each SEARCH_SORT_MODES as m (m)}
            <button
              type="button"
              class="sort-btn"
              class:active={sortMode === m}
              aria-pressed={sortMode === m}
              onclick={() => (sortMode = m)}
              title="Sort by {searchSortLabel(m).toLowerCase()}"
            >
              {searchSortLabel(m)}
            </button>
          {/each}
        </div>
      </div>
      {#if groups.length === 0}
        <div class="state empty refine-empty">
          <h2>No results match "{refine}"</h2>
          <p>
            {hits.length.toLocaleString()} match{hits.length === 1 ? "" : "es"} for
            <strong>"{lastQuery}"</strong>, but none also contain
            <strong>"{refine}"</strong>. Clear the refine to see them all.
          </p>
          <button type="button" class="refine-reset" onclick={() => (refine = "")}
            >Clear refine</button
          >
        </div>
      {/if}
      {#each groups as g, gi (g.docId)}
        <article class="group">
          <header class="group-header">
            <h3 class="doc-title">{g.title}</h3>
            <span class="doc-path" title={g.path}>{g.path}</span>
            {#if pageSpread(g.hits)}
              <span class="page-spread" title="Pages with a match">
                {pageSpread(g.hits)}
              </span>
            {/if}
            <span class="hit-count"
              >{g.hits.length} hit{g.hits.length === 1 ? "" : "s"}</span
            >
          </header>
          <ul class="hit-list">
            {#each g.hits as h, i (`${h.docId}-${h.pageIndex}-${i}`)}
              {@const flat = groupFlatStart(gi) + i}
              <li class="hit">
                <button
                  bind:this={hitEls[flat]}
                  type="button"
                  class="hit-btn"
                  class:cursor={flat === cursor}
                  aria-current={flat === cursor ? "true" : undefined}
                  onclick={() => {
                    cursor = flat;
                    openHit(h);
                  }}
                  onmouseenter={() => (cursor = flat)}
                  title="Open at page {h.pageIndex + 1}"
                >
                  <span class="page-badge">p. {h.pageIndex + 1}</span>
                  <span class="snippet"
                    >{#each buildSnippetSpans(h.snippet, refine) as seg}{#if seg.match && seg.refine}<mark
                          class="refine-in-match">{seg.text}</mark
                        >{:else if seg.match}<mark>{seg.text}</mark>{:else if seg.refine}<mark
                          class="refine-only">{seg.text}</mark
                        >{:else}{seg.text}{/if}{/each}</span
                  >
                </button>
              </li>
            {/each}
          </ul>
        </article>
      {/each}
    {/if}
  </div>
  {#if resultSummary}
    <footer class="index-footer result-footer" aria-live="polite">
      <span class="footer-dot" aria-hidden="true">●</span>
      <span>{resultSummary}</span>
    </footer>
  {:else if indexStats && (indexStats.docs > 0 || indexStats.pages > 0)}
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

  /* Slice 2: live query-interpretation strip — mirrors how the FTS
     backend will lex the query into prefix / exact / phrase / exclude
     tokens, so the user can see what's actually being searched. */
  .interp {
    margin-top: 10px;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    line-height: 1;
  }
  .interp-lead {
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--fg-muted, #999);
    font-weight: 600;
    font-size: 10px;
  }
  .interp-chip {
    display: inline-flex;
    align-items: center;
    gap: 1px;
    padding: 3px 8px;
    border-radius: 999px;
    border: 1px solid var(--border, rgba(0, 0, 0, 0.1));
    background: var(--bg-subtle, rgba(0, 0, 0, 0.04));
    color: var(--fg, #222);
    font-size: 11px;
    cursor: default;
    max-width: 240px;
  }
  .interp-text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  /* Prefix: accent-tinted, with a trailing glob glyph. */
  .interp-prefix {
    border-color: var(--accent, #4a72ff);
    background: var(--accent-fade, rgba(74, 114, 255, 0.12));
    color: var(--accent, #4a72ff);
    font-weight: 600;
  }
  .interp-glob {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    opacity: 0.7;
  }
  /* Phrase: quoted feel via a subtle italic. */
  .interp-phrase {
    font-style: italic;
  }
  /* Exclude: muted strike-through with a leading minus. */
  .interp-exclude {
    color: var(--fg-muted, #999);
    text-decoration: line-through;
    text-decoration-color: var(--fg-muted, #bbb);
  }
  .interp-minus {
    font-weight: 700;
    text-decoration: none;
    display: inline-block;
  }
  /* No-anchor warning: amber, matching the toast warning severity. */
  .interp.warn {
    color: #b7791f;
  }
  .interp-warn-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 15px;
    height: 15px;
    border-radius: 50%;
    background: #ffb648;
    color: #1a1a1a;
    font-weight: 700;
    font-size: 11px;
  }
  .interp-warn-text {
    font-weight: 500;
  }

  .results {
    flex: 1;
    overflow-y: auto;
    padding: 16px 32px 32px;
  }

  /* Slice 3: in-results refine bar — pinned at the top of the result
     list, narrows the already-returned hits client-side with no
     round-trip. Sticky so it stays reachable while scrolling a long
     result set. */
  .refine-bar {
    position: sticky;
    top: 0;
    z-index: 2;
    display: flex;
    align-items: center;
    gap: 8px;
    margin: -16px -32px 16px;
    padding: 10px 32px;
    background: var(--bg-app, #fafafa);
    border-bottom: 1px solid var(--border, rgba(0, 0, 0, 0.06));
  }
  .refine-icon {
    font-size: 13px;
    color: var(--fg-muted, #999);
    pointer-events: none;
  }
  .refine-input {
    flex: 1;
    border: none;
    background: transparent;
    font: inherit;
    font-size: 13px;
    color: var(--fg, #111);
    outline: none;
    padding: 2px 0;
  }
  .refine-input::placeholder {
    color: var(--fg-muted, #999);
  }
  .refine-clear {
    width: 22px;
    height: 22px;
    border: none;
    background: transparent;
    color: var(--fg-muted, #888);
    font-size: 17px;
    line-height: 1;
    cursor: pointer;
    border-radius: 5px;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .refine-clear:hover {
    background: var(--bg-subtle, rgba(0, 0, 0, 0.05));
    color: var(--fg, #111);
  }
  .refine-empty {
    margin: 24px auto;
  }
  .refine-reset {
    font: inherit;
    font-size: 12px;
    padding: 6px 14px;
    border: 1px solid var(--border, rgba(0, 0, 0, 0.12));
    border-radius: 7px;
    background: var(--bg-panel, #fff);
    color: var(--fg, #111);
    cursor: pointer;
    transition: border-color 80ms, background 80ms;
  }
  .refine-reset:hover {
    border-color: var(--accent, #4a72ff);
    background: var(--bg-hover, rgba(74, 114, 255, 0.06));
  }

  /* Slice 4: sort segmented control — sits at the right of the refine
     bar. Mirrors the Linear/Raycast segmented toggle: a pill container
     with one active segment. */
  .sort-seg {
    display: inline-flex;
    flex-shrink: 0;
    padding: 2px;
    gap: 1px;
    border-radius: 7px;
    background: var(--bg-subtle, rgba(0, 0, 0, 0.05));
    border: 1px solid var(--border, rgba(0, 0, 0, 0.06));
  }
  .sort-btn {
    font: inherit;
    font-size: 11px;
    font-weight: 500;
    padding: 3px 10px;
    border: none;
    border-radius: 5px;
    background: transparent;
    color: var(--fg-muted, #777);
    cursor: pointer;
    transition: background 80ms, color 80ms;
    white-space: nowrap;
  }
  .sort-btn:hover {
    color: var(--fg, #222);
  }
  .sort-btn.active {
    background: var(--bg-panel, #fff);
    color: var(--accent, #4a72ff);
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.08);
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

  /* Slice 5: per-group page-spread badge — the page range a document's
     matches span, shown in the group header. Monospace + accent-tinted
     so it reads as a precise locator, not chrome. */
  .page-spread {
    flex-shrink: 0;
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 10px;
    color: var(--accent, #4a72ff);
    padding: 1px 7px;
    border-radius: 999px;
    background: var(--accent-fade, rgba(74, 114, 255, 0.1));
    font-variant-numeric: tabular-nums;
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
  /* Slice 1: keyboard cursor ring — the focused result row as the arrow
     keys walk the flattened hit list. Mirrors the palette/beacon accent
     ring so the affordance is consistent across surfaces. */
  .hit-btn.cursor {
    background: var(--bg-hover, rgba(74, 114, 255, 0.08));
    border-color: var(--accent, #4a72ff);
    box-shadow: 0 0 0 2px var(--accent-fade, rgba(74, 114, 255, 0.2));
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

  /* Refine-highlight (Atlas III follow-up): a SECOND, distinct tint over
     the live refine term so you can see WHY a row survived the refine —
     the FTS match (yellow above) vs the client-side refine (cyan). A
     refine that also lands inside an FTS match gets a blended underline so
     both meanings read at once. */
  .snippet :global(mark.refine-only) {
    background: var(--refine-bg, #b5ecff);
    font-weight: 600;
  }
  .snippet :global(mark.refine-in-match) {
    background: var(--mark-bg, #fff3a3);
    box-shadow: inset 0 -2px 0 var(--refine-underline, #2bb5e0);
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
  /* Slice 6: keyboard cursor ring on the focused recent-search chip. */
  .recent-chip.cursor {
    border-color: var(--accent, #4a72ff);
    background: var(--accent-fade, rgba(74, 114, 255, 0.15));
    color: var(--fg, #111);
    box-shadow: 0 0 0 2px var(--accent-fade, rgba(74, 114, 255, 0.25));
  }
  .recent-chip.cursor:focus {
    outline: none;
  }
  /* Per-chip delete: the x button rides just inside the chip's trailing
     edge. Hidden until the chip is hovered or carries the keyboard cursor
     so the strip stays calm, then fades in as an escape hatch. */
  .recent-chip-wrap {
    position: relative;
    display: inline-flex;
  }
  .recent-chip-wrap .recent-chip {
    padding-right: 24px;
  }
  .recent-chip-del {
    position: absolute;
    right: 4px;
    top: 50%;
    transform: translateY(-50%);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    padding: 0;
    border: none;
    border-radius: 50%;
    background: transparent;
    color: var(--fg-muted, #888);
    cursor: pointer;
    opacity: 0;
    transition: opacity 80ms, background 80ms, color 80ms;
  }
  .recent-chip-wrap:hover .recent-chip-del,
  .recent-chip.cursor + .recent-chip-del {
    opacity: 1;
  }
  .recent-chip-del:hover,
  .recent-chip-del:focus-visible {
    opacity: 1;
    background: var(--danger-fade, rgba(229, 72, 77, 0.16));
    color: var(--danger, #e5484d);
    outline: none;
  }
  .recents-hint {
    margin: 8px 0 0;
    font-size: 11px;
    color: var(--fg-muted, #888);
  }
  .recents-hint kbd {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 10px;
    padding: 1px 5px;
    margin: 0 1px;
    border-radius: 4px;
    background: var(--bg-subtle, rgba(0, 0, 0, 0.05));
    border: 1px solid var(--border, rgba(0, 0, 0, 0.1));
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
  /* Relative-age suffix on a recent chip ("2m" / "3d") — a quiet,
     tabular-aligned hint sitting between the query and its match count. */
  .recent-age {
    font-size: 10px;
    color: var(--fg-muted, #999);
    opacity: 0.75;
    font-variant-numeric: tabular-nums;
    flex-shrink: 0;
    white-space: nowrap;
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
    .snippet :global(mark.refine-only) {
      background: var(--refine-bg, #11475a);
      color: #eaffff;
    }
    .snippet :global(mark.refine-in-match) {
      background: var(--mark-bg, #6b5e1f);
      color: #fff;
      box-shadow: inset 0 -2px 0 var(--refine-underline, #4ec8ee);
    }
    .interp-chip {
      background: var(--bg-subtle, rgba(255, 255, 255, 0.06));
      color: var(--fg, #ddd);
    }
    .refine-bar {
      background: var(--bg-app, #1a1a1a);
    }
    .refine-input {
      color: var(--fg, #eee);
    }
    .refine-reset {
      background: var(--bg-panel, #222);
      color: var(--fg, #ddd);
    }
    .sort-seg {
      background: var(--bg-subtle, rgba(255, 255, 255, 0.05));
    }
    .sort-btn.active {
      background: var(--bg-panel, #333);
    }
  }
</style>
