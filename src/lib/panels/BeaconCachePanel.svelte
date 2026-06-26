<!--
  Beacon Cache Inspector (v3.54.0 "Atlas Beacon-Cache").

  Headless modal-style panel that surfaces the entire Beacon embedding
  index in one demo-able place. Before this panel the index was an
  opaque box: the BeaconSearchPanel footer showed only "X PDFs · Y
  chunks indexed", with no way to see WHICH PDFs were in there, no
  per-model breakdown (a mixed-model index silently drops the loser's
  chunks at query time), no stale-path detection, no bulk cleanup.

  Sections:
    - Dashboard tiles: total PDFs / chunks / per-model breakdown (one
      tile per embed_model). A "Mixed model index" warning highlights
      when buckets.length > 1 — that's the search.rs dim-mismatch trap
      made visible.
    - Stale section: every indexed PDF whose path no longer points at
      a readable file. Per-row Forget; section-head "Forget all N
      stale" wraps the transactional `beaconIndexForgetStale`.
    - Full indexed-PDFs table: every row with hash + basename + folder
      hint + chunk count + indexed-at + model. Multi-select with
      "Select all / None / Invert"; floating bar shows "Forget N
      selected"; per-row Forget; column sort toggle (newest / oldest /
      chunks-desc).

  Triggered from:
    - Command palette ("Beacon Cache…")
    - Keyboard shortcut Cmd/Ctrl+Shift+B
    - `slab:open-beacon-cache` window event (panel-mounted
      CollectionsSidebar pattern, mirrors OcrQueuePanel +
      SmartFoldersHubPanel).

  No emoji in chrome (per Slab house style); the status dot mirrors
  LibrarySearchPanel's accent-green pip.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import {
    beaconIndexList,
    beaconIndexForgetMany,
    beaconIndexStatsByModel,
    beaconIndexFindStale,
    beaconIndexForgetStale,
    type IndexedPdfRecord,
    type ModelBucket,
  } from "$lib/beaconCache";
  import { searchIndexedPdfs, sortIndexedPdfs, cycleBeaconSort, beaconSortLabel, BEACON_SORT_FIELDS, filterByModel, reconcileModelFacet, summarizeSelection, describeImpact, describeBeaconView, type BeaconSort, type BeaconSortField } from "$lib/beaconCacheView";
  import { splitHighlight } from "$lib/paletteSearch";

  type Props = {
    open: boolean;
    onClose: () => void;
  };

  const { open, onClose }: Props = $props();

  let pdfs = $state<IndexedPdfRecord[]>([]);
  let stale = $state<IndexedPdfRecord[]>([]);
  let buckets = $state<ModelBucket[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);

  /** Multi-select set, indexed by `pdf_hash`. Pruned to the visible
      list on every refresh so a forgotten hash can't linger. */
  let selected = $state<Set<string>>(new Set());
  let forgettingHash = $state<string | null>(null);
  let bulkForgetBusy = $state(false);
  let staleForgetBusy = $state(false);
  let toast = $state<string | null>(null);
  let toastTimer: ReturnType<typeof setTimeout> | null = null;

  /** Slice 2: multi-field column sort (field + direction). */
  let sort = $state<BeaconSort>({ field: "indexed", dir: "desc" });

  /** Slice 1: filter-as-you-type query over the indexed-PDF table. */
  let search = $state("");
  let searchEl = $state<HTMLInputElement | null>(null);

  /** Slice 3: active model facet (one embed_model) or null for "all". */
  let modelFacet = $state<string | null>(null);

  const totalPdfs = $derived(pdfs.length);
  const totalChunks = $derived(
    pdfs.reduce((acc, p) => acc + p.chunks, 0),
  );
  const isMixedModel = $derived(buckets.length > 1);
  const selectedCount = $derived(selected.size);

  /** Rows after the model facet (slice 3) then the search (slice 1). */
  const facetedPdfs = $derived(filterByModel(pdfs, modelFacet));
  const searchHits = $derived(searchIndexedPdfs(facetedPdfs, search));
  /** hash -> highlight ranges, so the row template can paint the match. */
  const nameRangesByHash = $derived(
    new Map(searchHits.map((h) => [h.record.pdf_hash, h.nameRanges])),
  );
  /** True when any filter (search text OR model facet) is narrowing. */
  const isFiltering = $derived(search.trim().length > 0 || modelFacet !== null);

  /** Faceted + searched subset, sorted by the active column. */
  const sortedPdfs = $derived(
    sortIndexedPdfs(searchHits.map((h) => h.record), sort),
  );
  const matchedCount = $derived(sortedPdfs.length);

  /** Slice 4: real footprint of the current selection (chunks/pages dropped). */
  const selectionImpact = $derived(summarizeSelection(pdfs, selected));
  const impactLabel = $derived(describeImpact(selectionImpact));
  /** Slice 4: context-aware footer line narrating the current view. */
  const viewSummary = $derived(
    describeBeaconView({
      shown: matchedCount,
      total: totalPdfs,
      modelFacet,
      query: search,
      selected: selectedCount,
    }),
  );

  function setSort(field: BeaconSortField) {
    sort = cycleBeaconSort(sort, field);
  }

  function toggleModelFacet(model: string) {
    modelFacet = modelFacet === model ? null : model;
  }

  function showToast(msg: string) {
    toast = msg;
    if (toastTimer) clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = null), 2400);
  }

  function basename(path: string): string {
    const i = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
    return i >= 0 ? path.slice(i + 1) : path;
  }

  function folderHint(path: string): string {
    const i = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
    if (i <= 0) return "";
    const dir = path.slice(0, i);
    if (dir.length <= 40) return dir;
    const parts = dir.split(/[/\\]/);
    if (parts.length <= 3) return dir;
    return `${parts[0] || parts[1]}/…/${parts[parts.length - 1]}`;
  }

  function fmtTimestamp(unixSec: number): string {
    if (!unixSec) return "—";
    const d = new Date(unixSec * 1000);
    const diff = (Date.now() - d.getTime()) / 1000;
    if (diff < 60) return "just now";
    if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
    if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
    if (diff < 7 * 86400) return `${Math.floor(diff / 86400)}d ago`;
    return d.toLocaleDateString();
  }

  function shortHash(hash: string): string {
    return hash.length > 12 ? `${hash.slice(0, 12)}…` : hash;
  }

  async function refresh() {
    loading = true;
    error = null;
    try {
      const [list, modelBuckets, staleRows] = await Promise.all([
        beaconIndexList(),
        beaconIndexStatsByModel(),
        beaconIndexFindStale(),
      ]);
      pdfs = list;
      buckets = modelBuckets;
      stale = staleRows;
      // Drop a model facet whose model no longer exists (its last PDF was
      // forgotten) so the table can't strand on an empty faceted view.
      modelFacet = reconcileModelFacet(
        modelFacet,
        modelBuckets.map((b) => b.embed_model),
      );
      // Prune selection to what's still visible.
      const visibleHashes = new Set(list.map((p) => p.pdf_hash));
      const next = new Set<string>();
      for (const h of selected) {
        if (visibleHashes.has(h)) next.add(h);
      }
      selected = next;
    } catch (e) {
      error = (e as Error).message;
    } finally {
      loading = false;
    }
  }

  function toggleOne(hash: string) {
    const next = new Set(selected);
    if (next.has(hash)) next.delete(hash);
    else next.add(hash);
    selected = next;
  }
  function selectAllVisible() {
    selected = new Set(sortedPdfs.map((p) => p.pdf_hash));
  }
  function clearSelection() {
    selected = new Set();
  }
  function invertSelection() {
    const next = new Set<string>();
    for (const p of sortedPdfs) {
      if (!selected.has(p.pdf_hash)) next.add(p.pdf_hash);
    }
    selected = next;
  }

  async function forgetOne(p: IndexedPdfRecord) {
    if (forgettingHash) return;
    if (
      !confirm(
        `Forget "${basename(p.pdf_path)}" from the Beacon cache? (${p.chunks} chunk${p.chunks === 1 ? "" : "s"} dropped.)`,
      )
    ) {
      return;
    }
    forgettingHash = p.pdf_hash;
    error = null;
    try {
      const removed = await beaconIndexForgetMany([p.pdf_hash]);
      if (removed > 0) {
        showToast(`Forgot ${basename(p.pdf_path)} (${p.chunks} chunks dropped)`);
      } else {
        showToast(`No rows removed — already gone?`);
      }
      await refresh();
    } catch (e) {
      error = (e as Error).message;
    } finally {
      forgettingHash = null;
    }
  }

  async function forgetSelected() {
    if (bulkForgetBusy || selectedCount === 0) return;
    if (
      !confirm(
        `Forget ${selectedCount} indexed PDF${selectedCount === 1 ? "" : "s"} from the Beacon cache?`,
      )
    ) {
      return;
    }
    bulkForgetBusy = true;
    error = null;
    try {
      const removed = await beaconIndexForgetMany(Array.from(selected));
      showToast(
        `Forgot ${removed} indexed PDF${removed === 1 ? "" : "s"} from the cache`,
      );
      clearSelection();
      await refresh();
    } catch (e) {
      error = (e as Error).message;
    } finally {
      bulkForgetBusy = false;
    }
  }

  async function forgetAllStale() {
    if (staleForgetBusy || stale.length === 0) return;
    if (
      !confirm(
        `Forget all ${stale.length} stale entries (files no longer exist on disk)?`,
      )
    ) {
      return;
    }
    staleForgetBusy = true;
    error = null;
    try {
      const removed = await beaconIndexForgetStale();
      showToast(`Pruned ${removed} stale entr${removed === 1 ? "y" : "ies"}`);
      await refresh();
    } catch (e) {
      error = (e as Error).message;
    } finally {
      staleForgetBusy = false;
    }
  }

  function handleKey(e: KeyboardEvent) {
    if (!open) return;
    if (e.key === "Escape") {
      e.preventDefault();
      onClose();
    }
  }

  onMount(() => {
    refresh();
    window.addEventListener("keydown", handleKey);
    const libHandler = () => refresh();
    window.addEventListener("library-changed", libHandler);
    return () => {
      window.removeEventListener("keydown", handleKey);
      window.removeEventListener("library-changed", libHandler);
      if (toastTimer) clearTimeout(toastTimer);
    };
  });

  $effect(() => {
    if (open) refresh();
  });
</script>

{#if open}
  <div
    class="bc-backdrop"
    role="dialog"
    aria-modal="true"
    aria-label="Beacon Cache Inspector"
    onclick={(e) => {
      if (e.target === e.currentTarget) onClose();
    }}
    onkeydown={handleKey}
    tabindex="-1"
  >
    <div class="bc-shell">
      <header class="bc-head">
        <div class="bc-title">
          <span class="bc-dot" aria-hidden="true"></span>
          <div>
            <h2>Beacon Cache</h2>
            <p class="bc-subtitle">
              {totalPdfs} PDF{totalPdfs === 1 ? "" : "s"} · {totalChunks.toLocaleString()} chunks indexed
              {#if stale.length > 0}
                · <span class="bc-subtitle-warn">{stale.length} stale</span>
              {/if}
            </p>
          </div>
        </div>
        <div class="bc-actions">
          <button class="bc-btn" onclick={refresh} disabled={loading} title="Reload (also runs automatically on library-changed events)">
            {loading ? "Loading…" : "Refresh"}
          </button>
          <button class="bc-btn" onclick={onClose} aria-label="Close">Close</button>
        </div>
      </header>

      {#if error}
        <div class="bc-error" role="alert">{error}</div>
      {/if}

      <!-- Dashboard: total + per-model -->
      <section class="bc-dashboard">
        <div class="bc-tile">
          <div class="bc-tile-num tabular">{totalPdfs.toLocaleString()}</div>
          <div class="bc-tile-label">indexed PDFs</div>
        </div>
        <div class="bc-tile">
          <div class="bc-tile-num tabular">{totalChunks.toLocaleString()}</div>
          <div class="bc-tile-label">chunks stored</div>
        </div>
        {#each buckets as b (b.embed_model)}
          <button
            type="button"
            class="bc-tile bc-tile-model bc-tile-facet"
            class:active={modelFacet === b.embed_model}
            onclick={() => toggleModelFacet(b.embed_model)}
            aria-pressed={modelFacet === b.embed_model}
            title={modelFacet === b.embed_model
              ? `Showing only ${b.embed_model} — click to clear`
              : `Show only ${b.embed_model}`}
          >
            <div class="bc-tile-num tabular">{b.chunks.toLocaleString()}</div>
            <div class="bc-tile-label" title={b.embed_model}>
              {b.embed_model} · {b.pdfs} PDF{b.pdfs === 1 ? "" : "s"}
            </div>
            {#if modelFacet === b.embed_model}
              <span class="bc-tile-facet-pill">Filtering</span>
            {/if}
          </button>
        {/each}
      </section>

      {#if isMixedModel}
        <div class="bc-warn" role="status">
          <strong>Mixed-model index detected.</strong>
          You have chunks under {buckets.length} different embed models.
          Beacon's query path silently skips dim-mismatched chunks, so the
          loser is effectively dead weight. Forget one bucket to reclaim
          space — re-index runs will pick up the active model.
        </div>
      {/if}

      <!-- Stale section -->
      {#if stale.length > 0}
        <section class="bc-section bc-stale">
          <div class="bc-section-head">
            <h3>Stale entries · {stale.length}</h3>
            <button
              class="bc-btn bc-btn-danger"
              onclick={forgetAllStale}
              disabled={staleForgetBusy}
              title="Forget every entry whose file no longer exists on disk"
            >
              {staleForgetBusy ? "Pruning…" : `Forget all ${stale.length} stale`}
            </button>
          </div>
          <ul class="bc-stale-list">
            {#each stale as p (p.pdf_hash)}
              <li class="bc-stale-row">
                <div class="bc-stale-name">{basename(p.pdf_path)}</div>
                <div class="bc-stale-path" title={p.pdf_path}>{folderHint(p.pdf_path)}</div>
                <div class="bc-stale-meta tabular">
                  {p.chunks} chunk{p.chunks === 1 ? "" : "s"}
                </div>
              </li>
            {/each}
          </ul>
        </section>
      {/if}

      <!-- Main table -->
      <section class="bc-section bc-main">
        <div class="bc-section-head">
          <h3>
            Indexed PDFs · {#if isFiltering}{matchedCount} of {totalPdfs}{:else}{totalPdfs}{/if}
          </h3>
          <div class="bc-sort" role="group" aria-label="Sort indexed PDFs">
            {#each BEACON_SORT_FIELDS as field (field)}
              <button
                class="bc-sort-btn"
                class:active={sort.field === field}
                onclick={() => setSort(field)}
                aria-pressed={sort.field === field}
                title={`Sort by ${beaconSortLabel(field)}${sort.field === field ? (sort.dir === "asc" ? " (ascending)" : " (descending)") : ""}`}
              >
                {beaconSortLabel(field)}
                {#if sort.field === field}
                  <span class="bc-caret" aria-hidden="true">{sort.dir === "asc" ? "\u2191" : "\u2193"}</span>
                {/if}
              </button>
            {/each}
          </div>
        </div>

        {#if pdfs.length > 0}
          <div class="bc-search">
            <input
              bind:this={searchEl}
              class="bc-search-input"
              type="text"
              placeholder="Filter by name, folder, model, or hash…"
              bind:value={search}
              spellcheck="false"
              autocomplete="off"
              aria-label="Filter indexed PDFs"
              onkeydown={(e) => {
                if (e.key === "Escape" && search) {
                  e.preventDefault();
                  e.stopPropagation();
                  search = "";
                }
              }}
            />
            {#if isFiltering}
              <button
                class="bc-search-clear"
                onclick={() => {
                  search = "";
                  searchEl?.focus();
                }}
                aria-label="Clear filter"
                title="Clear filter (Esc)"
              >Clear</button>
            {/if}
          </div>
        {/if}

        {#if pdfs.length === 0}
          <div class="bc-empty">
            {#if loading}
              Loading…
            {:else}
              Beacon cache is empty. Open a PDF with Beacon enabled to start indexing.
            {/if}
          </div>
        {:else if sortedPdfs.length === 0}
          <div class="bc-empty">
            {#if search.trim() && modelFacet}
              No <span class="bc-empty-q">{modelFacet}</span> PDFs match
              <span class="bc-empty-q">“{search.trim()}”</span>.
            {:else if search.trim()}
              No PDFs match <span class="bc-empty-q">“{search.trim()}”</span>.
            {:else}
              No PDFs indexed under <span class="bc-empty-q">{modelFacet}</span>.
            {/if}
            <button
              class="bc-link"
              onclick={() => {
                search = "";
                modelFacet = null;
              }}
            >Clear filters</button>
          </div>
        {:else}
          <div class="bc-select-bar">
            <button class="bc-link" onclick={selectAllVisible}>Select all</button>
            <span class="bc-sep">·</span>
            <button class="bc-link" onclick={clearSelection}>None</button>
            <span class="bc-sep">·</span>
            <button class="bc-link" onclick={invertSelection}>Invert</button>
            {#if selectedCount > 0}
              <span class="bc-sep">·</span>
              <span class="bc-sel-count tabular">{selectedCount} selected</span>
            {/if}
          </div>
          <ul class="bc-pdf-list" role="list">
            {#each sortedPdfs as p (p.pdf_hash)}
              <li
                class="bc-row"
                class:selected={selected.has(p.pdf_hash)}
                role="listitem"
              >
                <label class="bc-row-check">
                  <input
                    type="checkbox"
                    checked={selected.has(p.pdf_hash)}
                    onchange={() => toggleOne(p.pdf_hash)}
                    aria-label={`Select ${basename(p.pdf_path)}`}
                  />
                </label>
                <div class="bc-row-body">
                  <div class="bc-row-name">
                    {#each splitHighlight(basename(p.pdf_path), nameRangesByHash.get(p.pdf_hash) ?? []) as seg}
                      {#if seg.hit}<mark class="bc-hl">{seg.text}</mark>{:else}{seg.text}{/if}
                    {/each}
                  </div>
                  <div class="bc-row-hint" title={p.pdf_path}>
                    <span class="bc-row-folder">{folderHint(p.pdf_path) || "/"}</span>
                    <span class="bc-row-hash tabular" title={p.pdf_hash}>{shortHash(p.pdf_hash)}</span>
                  </div>
                </div>
                <div class="bc-row-meta tabular">
                  <div>{p.chunks} chunk{p.chunks === 1 ? "" : "s"}</div>
                  <div class="bc-row-sub">{p.pages} pg · {p.embed_model}</div>
                </div>
                <div class="bc-row-when tabular">{fmtTimestamp(p.indexed_at)}</div>
                <button
                  class="bc-row-forget"
                  onclick={() => forgetOne(p)}
                  disabled={forgettingHash === p.pdf_hash}
                  title="Forget this PDF (drops every chunk)"
                  aria-label={`Forget ${basename(p.pdf_path)}`}
                >
                  {forgettingHash === p.pdf_hash ? "…" : "Forget"}
                </button>
              </li>
            {/each}
          </ul>
        {/if}

        {#if pdfs.length > 0}
          <div class="bc-footer">
            <span class="bc-footer-summary">{viewSummary}</span>
            {#if selectedCount > 0}
              <span class="bc-footer-impact" title="Total footprint of the current selection">
                Selection: {impactLabel}
              </span>
            {/if}
          </div>
        {/if}
      </section>

      {#if selectedCount > 0}
        <div class="bc-bulk-bar">
          <div class="bc-bulk-info">
            <span class="tabular">{selectedCount} selected</span>
            <span class="bc-bulk-impact tabular">drops {impactLabel}</span>
          </div>
          <button
            class="bc-btn bc-btn-danger"
            onclick={forgetSelected}
            disabled={bulkForgetBusy}
          >
            {bulkForgetBusy ? "Forgetting…" : `Forget ${selectedCount}`}
          </button>
        </div>
      {/if}

      {#if toast}
        <div class="bc-toast" role="status">{toast}</div>
      {/if}
    </div>
  </div>
{/if}

<style>
  .bc-backdrop {
    position: fixed;
    inset: 0;
    background: color-mix(in srgb, black 45%, transparent);
    backdrop-filter: blur(14px) saturate(140%);
    -webkit-backdrop-filter: blur(14px) saturate(140%);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1500;
    animation: bc-fade-in 140ms ease-out;
  }
  @keyframes bc-fade-in {
    from { opacity: 0; }
    to { opacity: 1; }
  }
  .bc-shell {
    width: min(900px, 94vw);
    max-height: 88vh;
    display: flex;
    flex-direction: column;
    background: var(--panel-bg, #181826);
    color: var(--text, #e7e7f0);
    border: 1px solid color-mix(in srgb, white 8%, transparent);
    border-radius: 16px;
    box-shadow:
      0 24px 64px rgba(0, 0, 0, 0.55),
      0 0 0 1px color-mix(in srgb, white 4%, transparent);
    overflow: hidden;
    animation: bc-pop-in 160ms cubic-bezier(.2,.9,.3,1);
    position: relative;
  }
  @keyframes bc-pop-in {
    from { transform: scale(.97); opacity: 0; }
    to   { transform: scale(1);   opacity: 1; }
  }
  .bc-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    padding: 18px 22px 14px;
    border-bottom: 1px solid color-mix(in srgb, white 7%, transparent);
    background: linear-gradient(
      180deg,
      color-mix(in srgb, white 4%, transparent),
      transparent
    );
  }
  .bc-title {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
  }
  .bc-dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: #7ee787;
    box-shadow: 0 0 0 3px color-mix(in srgb, #7ee787 22%, transparent);
  }
  .bc-title h2 {
    margin: 0;
    font-size: 17px;
    font-weight: 600;
    letter-spacing: -0.01em;
  }
  .bc-subtitle {
    margin: 2px 0 0;
    font-size: 12px;
    opacity: 0.62;
  }
  .bc-subtitle-warn {
    color: #f5c518;
    opacity: 0.92;
  }
  .bc-actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .bc-btn {
    appearance: none;
    border: 1px solid color-mix(in srgb, white 12%, transparent);
    background: color-mix(in srgb, white 6%, transparent);
    color: inherit;
    padding: 6px 12px;
    border-radius: 8px;
    font-size: 13px;
    cursor: pointer;
    transition: background 120ms, transform 80ms;
  }
  .bc-btn:hover:not(:disabled) {
    background: color-mix(in srgb, white 11%, transparent);
  }
  .bc-btn:active:not(:disabled) {
    transform: translateY(1px);
  }
  .bc-btn:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
  .bc-btn-danger {
    border-color: color-mix(in srgb, #ff5d6c 40%, transparent);
    background: color-mix(in srgb, #ff5d6c 14%, transparent);
    color: #ffd1d5;
  }
  .bc-btn-danger:hover:not(:disabled) {
    background: color-mix(in srgb, #ff5d6c 22%, transparent);
  }
  .bc-error {
    margin: 10px 22px 0;
    padding: 8px 12px;
    background: color-mix(in srgb, #ff5d6c 18%, transparent);
    border: 1px solid color-mix(in srgb, #ff5d6c 40%, transparent);
    color: #ffb8be;
    border-radius: 8px;
    font-size: 12px;
  }

  .bc-dashboard {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
    gap: 10px;
    padding: 14px 22px 10px;
  }
  .bc-tile {
    border: 1px solid color-mix(in srgb, white 7%, transparent);
    background: color-mix(in srgb, white 3%, transparent);
    border-radius: 10px;
    padding: 10px 12px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .bc-tile-model {
    border-color: color-mix(in srgb, #79c0ff 22%, transparent);
    background: color-mix(in srgb, #79c0ff 6%, transparent);
  }
  .bc-tile-facet {
    position: relative;
    appearance: none;
    color: inherit;
    text-align: left;
    cursor: pointer;
    font: inherit;
    transition: background 120ms, border-color 120ms, transform 80ms;
  }
  .bc-tile-facet:hover {
    background: color-mix(in srgb, #79c0ff 12%, transparent);
    border-color: color-mix(in srgb, #79c0ff 38%, transparent);
  }
  .bc-tile-facet:active {
    transform: translateY(1px);
  }
  .bc-tile-facet.active {
    background: color-mix(in srgb, #79c0ff 22%, transparent);
    border-color: color-mix(in srgb, #79c0ff 60%, transparent);
    box-shadow: 0 0 0 1px color-mix(in srgb, #79c0ff 40%, transparent);
  }
  .bc-tile-facet-pill {
    position: absolute;
    top: 8px;
    right: 8px;
    font-size: 9px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    padding: 2px 6px;
    border-radius: 999px;
    background: color-mix(in srgb, #79c0ff 30%, transparent);
    color: #d6e9ff;
  }
  .bc-tile-num {
    font-size: 20px;
    font-weight: 600;
    letter-spacing: -0.02em;
  }
  .bc-tile-label {
    font-size: 11px;
    opacity: 0.62;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .bc-warn {
    margin: 0 22px 10px;
    padding: 10px 12px;
    background: color-mix(in srgb, #f5c518 12%, transparent);
    border: 1px solid color-mix(in srgb, #f5c518 36%, transparent);
    color: #fff2b8;
    border-radius: 8px;
    font-size: 12px;
    line-height: 1.5;
  }
  .bc-warn strong {
    color: #f5c518;
    font-weight: 600;
  }

  .bc-section {
    margin: 0 22px;
    padding-top: 8px;
  }
  .bc-section + .bc-section {
    border-top: 1px solid color-mix(in srgb, white 6%, transparent);
    margin-top: 10px;
    padding-top: 12px;
  }
  .bc-section-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 8px;
  }
  .bc-section-head h3 {
    margin: 0;
    font-size: 12px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    opacity: 0.66;
  }
  .bc-sort {
    display: flex;
    gap: 2px;
    border: 1px solid color-mix(in srgb, white 10%, transparent);
    border-radius: 8px;
    overflow: hidden;
  }
  .bc-sort-btn {
    appearance: none;
    background: transparent;
    color: inherit;
    border: none;
    padding: 4px 10px;
    font-size: 11px;
    cursor: pointer;
    opacity: 0.6;
  }
  .bc-sort-btn:hover {
    background: color-mix(in srgb, white 6%, transparent);
    opacity: 0.9;
  }
  .bc-sort-btn.active {
    background: color-mix(in srgb, #79c0ff 22%, transparent);
    color: #c5e0ff;
    opacity: 1;
  }
  .bc-caret {
    margin-left: 3px;
    font-size: 10px;
    opacity: 0.85;
  }

  .bc-search {
    position: relative;
    display: flex;
    align-items: center;
    margin-bottom: 8px;
  }
  .bc-search-input {
    flex: 1;
    appearance: none;
    background: color-mix(in srgb, white 4%, transparent);
    border: 1px solid color-mix(in srgb, white 10%, transparent);
    color: inherit;
    font-size: 12px;
    padding: 7px 11px;
    padding-right: 60px;
    border-radius: 8px;
    outline: none;
    transition: border-color 120ms, background 120ms;
  }
  .bc-search-input::placeholder {
    color: inherit;
    opacity: 0.4;
  }
  .bc-search-input:focus {
    border-color: color-mix(in srgb, #79c0ff 50%, transparent);
    background: color-mix(in srgb, #79c0ff 6%, transparent);
  }
  .bc-search-clear {
    position: absolute;
    right: 6px;
    appearance: none;
    background: transparent;
    border: none;
    color: inherit;
    opacity: 0.55;
    font-size: 11px;
    cursor: pointer;
    padding: 3px 7px;
    border-radius: 6px;
  }
  .bc-search-clear:hover {
    opacity: 1;
    background: color-mix(in srgb, white 8%, transparent);
  }
  .bc-hl {
    background: color-mix(in srgb, #79c0ff 30%, transparent);
    color: #d6e9ff;
    border-radius: 3px;
    padding: 0 1px;
  }
  .bc-empty-q {
    color: #c5e0ff;
    font-style: italic;
  }

  .bc-stale-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin-bottom: 12px;
  }
  .bc-stale-row {
    display: grid;
    grid-template-columns: 1fr auto auto;
    gap: 12px;
    align-items: center;
    padding: 6px 10px;
    border-radius: 8px;
    background: color-mix(in srgb, #ff5d6c 6%, transparent);
    font-size: 12px;
  }
  .bc-stale-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .bc-stale-path {
    opacity: 0.55;
    font-size: 11px;
    font-family: ui-monospace, "SF Mono", Menlo, monospace;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 280px;
  }
  .bc-stale-meta {
    opacity: 0.65;
    font-size: 11px;
  }

  .bc-main {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    padding-bottom: 18px;
  }
  .bc-select-bar {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 11px;
    opacity: 0.7;
    margin-bottom: 6px;
  }
  .bc-link {
    appearance: none;
    background: transparent;
    color: inherit;
    border: none;
    padding: 0;
    font-size: inherit;
    cursor: pointer;
    text-decoration: underline;
    text-underline-offset: 2px;
    opacity: 0.8;
  }
  .bc-link:hover { opacity: 1; }
  .bc-sep { opacity: 0.4; }
  .bc-sel-count { color: #79c0ff; }
  .bc-pdf-list {
    list-style: none;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .bc-row {
    display: grid;
    grid-template-columns: 28px 1fr auto auto auto;
    align-items: center;
    gap: 12px;
    padding: 8px 10px;
    border-radius: 8px;
    transition: background 120ms;
    border: 1px solid transparent;
  }
  .bc-row:hover {
    background: color-mix(in srgb, white 4%, transparent);
  }
  .bc-row.selected {
    background: color-mix(in srgb, #79c0ff 12%, transparent);
    border-color: color-mix(in srgb, #79c0ff 30%, transparent);
  }
  .bc-row-check {
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .bc-row-check input { cursor: pointer; }
  .bc-row-body { min-width: 0; }
  .bc-row-name {
    font-size: 13px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .bc-row-hint {
    font-size: 11px;
    opacity: 0.55;
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .bc-row-folder {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 280px;
  }
  .bc-row-hash {
    font-family: ui-monospace, "SF Mono", Menlo, monospace;
    opacity: 0.7;
  }
  .bc-row-meta {
    font-size: 12px;
    text-align: right;
    line-height: 1.25;
  }
  .bc-row-sub {
    font-size: 10px;
    opacity: 0.55;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .bc-row-when {
    font-size: 11px;
    opacity: 0.6;
    text-align: right;
    min-width: 64px;
  }
  .bc-row-forget {
    appearance: none;
    background: transparent;
    border: 1px solid color-mix(in srgb, white 10%, transparent);
    color: inherit;
    padding: 4px 10px;
    border-radius: 6px;
    font-size: 11px;
    cursor: pointer;
    opacity: 0.7;
  }
  .bc-row-forget:hover:not(:disabled) {
    border-color: color-mix(in srgb, #ff5d6c 40%, transparent);
    background: color-mix(in srgb, #ff5d6c 12%, transparent);
    color: #ffd1d5;
    opacity: 1;
  }
  .bc-row-forget:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .bc-empty {
    padding: 32px 14px;
    text-align: center;
    opacity: 0.55;
    font-size: 12px;
  }

  .bc-bulk-bar {
    position: absolute;
    bottom: 18px;
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 8px 14px;
    background: color-mix(in srgb, var(--panel-bg, #181826) 92%, transparent);
    border: 1px solid color-mix(in srgb, white 14%, transparent);
    border-radius: 999px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
    font-size: 12px;
    z-index: 4;
    animation: bc-pop-in 160ms ease-out;
  }
  .bc-bulk-info {
    display: flex;
    flex-direction: column;
    gap: 1px;
    line-height: 1.3;
  }
  .bc-bulk-impact {
    font-size: 10px;
    opacity: 0.6;
  }

  .bc-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin-top: 8px;
    padding-top: 8px;
    border-top: 1px solid color-mix(in srgb, white 6%, transparent);
    font-size: 11px;
    opacity: 0.7;
  }
  .bc-footer-summary {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  .bc-footer-impact {
    flex-shrink: 0;
    color: #79c0ff;
    font-variant-numeric: tabular-nums;
  }

  .bc-toast {
    position: absolute;
    bottom: 18px;
    right: 22px;
    padding: 8px 12px;
    background: color-mix(in srgb, var(--panel-bg, #181826) 92%, transparent);
    border: 1px solid color-mix(in srgb, white 14%, transparent);
    color: inherit;
    border-radius: 8px;
    font-size: 12px;
    box-shadow: 0 8px 20px rgba(0, 0, 0, 0.35);
    z-index: 5;
    animation: bc-pop-in 160ms ease-out;
  }

  .tabular {
    font-variant-numeric: tabular-nums;
  }
</style>
