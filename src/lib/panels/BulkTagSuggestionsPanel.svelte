<!--
  Bulk tag-suggestions review panel (v3.39.0 Atlas Tag-Suggest slice 52).

  A right-side drawer that fans the heuristic suggester across the
  active library filter, surfaces every (doc, suggested-tag) pair
  as a checkbox grid, and ships a single "Apply N" button that
  goes through the bulk-accept primitive in one round-trip.

  Demo path: open the library, click "Review suggestions (N)" in
  the toolbar (gated on the slice 51 stats badge), the drawer
  fans the suggester across the current filter, the user ticks
  the ones they want, Apply lands them with one event refresh.

  Sections:
    1. Source strip — "Untagged only" / "Current filter" toggle +
       per-doc cap input (defaults to 50).
    2. Bulk control bar — Refresh, Select-all-source-X chips (one
       per suggestion source: vocabulary / co-occurrence / domain),
       "Apply N suggestions" primary action with disabled state.
    3. Suggestion grid — one card per doc, with title + path, then
       up to 5 chips. Each chip has accept + dismiss buttons; the
       accept toggles selection (so a click adds it to the
       Apply-N queue).
    4. Hidden suggestions disclosure — lists per-doc dismissals
       with a 1-click undismiss button, mounted only when the
       grid is empty so it doubles as recovery.
    5. Empty/loading skeletons + toast confirmation row.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import {
    tagSuggestionsBulk,
    tagSuggestionsBulkForFilter,
    acceptTagSuggestionsBulk,
    listDismissedTagSuggestions,
    undismissOneTagSuggestion,
    tagSuggestionStats,
    type BulkTagSuggestion,
    type TagSuggestion,
    type TagSuggestionAcceptItem,
    type DismissedTagSuggestion,
    type TagSuggestionStats,
    type LibraryFilter,
  } from "$lib/library";

  type Props = {
    /** When non-null, the drawer is mounted + visible. */
    open: boolean;
    /** Active library filter (passed through to suggest_for_filter when
        the source mode is "filter"). */
    filter: LibraryFilter | null;
    /** Called when Apply succeeded so the parent can refresh its grid. */
    onApplied?: (attached: number) => void;
    /** Called to dismiss the drawer. */
    onClose: () => void;
  };
  let { open, filter, onApplied = () => {}, onClose }: Props = $props();

  /** "untagged" uses the lighter `tagSuggestionsBulk` (LEFT JOIN /
      tag_id IS NULL); "filter" uses `tagSuggestionsBulkForFilter`
      so saved views + clause trees + starred-only all narrow the
      candidate set. */
  let sourceMode = $state<"untagged" | "filter">("untagged");
  let perDocCap = $state<number>(50);
  let groups = $state<BulkTagSuggestion[]>([]);
  let loading = $state(false);
  let applying = $state(false);
  let stats = $state<TagSuggestionStats | null>(null);
  let dismissed = $state<DismissedTagSuggestion[]>([]);
  let dismissedExpanded = $state(false);
  let activeDismissedDoc = $state<number | null>(null);
  let toast = $state<string | null>(null);
  let errorMsg = $state<string | null>(null);

  // Selection state — pairs of (doc_id::tag_name) in a Set so toggle
  // is O(1) and the cardinality drives the Apply-N button label.
  let selected = $state<Set<string>>(new Set());
  function keyFor(docId: number, name: string): string {
    return `${docId}::${name.toLowerCase()}`;
  }
  function isSelected(docId: number, name: string): boolean {
    return selected.has(keyFor(docId, name));
  }
  function toggleSelected(docId: number, name: string) {
    const k = keyFor(docId, name);
    const next = new Set(selected);
    if (next.has(k)) {
      next.delete(k);
    } else {
      next.add(k);
    }
    selected = next;
  }

  async function refresh() {
    loading = true;
    errorMsg = null;
    try {
      if (sourceMode === "filter" && filter) {
        groups = await tagSuggestionsBulkForFilter(filter, perDocCap);
      } else {
        groups = await tagSuggestionsBulk(perDocCap);
      }
      // Drop selections referring to docs that fell out of the new fetch.
      const stillThere = new Set<string>();
      for (const g of groups) {
        for (const s of g.suggestions) {
          stillThere.add(keyFor(g.doc_id, s.tag_name));
        }
      }
      const filtered = new Set<string>();
      for (const k of selected) {
        if (stillThere.has(k)) filtered.add(k);
      }
      selected = filtered;
      stats = await tagSuggestionStats(undefined);
    } catch (e) {
      errorMsg = String(e);
      groups = [];
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    if (open) {
      refresh();
    }
  });

  function selectAllVisible() {
    const next = new Set(selected);
    for (const g of groups) {
      for (const s of g.suggestions) {
        next.add(keyFor(g.doc_id, s.tag_name));
      }
    }
    selected = next;
  }

  function selectAllBySource(src: TagSuggestion["source"]) {
    const next = new Set(selected);
    for (const g of groups) {
      for (const s of g.suggestions) {
        if (s.source === src) {
          next.add(keyFor(g.doc_id, s.tag_name));
        }
      }
    }
    selected = next;
  }

  function clearSelection() {
    selected = new Set();
  }

  async function applySelected() {
    if (selected.size === 0 || applying) return;
    applying = true;
    errorMsg = null;
    try {
      // Convert Set<string> -> AcceptItem[]; the backend dedupes
      // case/whitespace variants so we don't need to here.
      const items: TagSuggestionAcceptItem[] = [];
      for (const k of selected) {
        const idx = k.indexOf("::");
        if (idx < 0) continue;
        items.push({
          doc_id: Number(k.slice(0, idx)),
          tag_name: k.slice(idx + 2),
        });
      }
      const res = await acceptTagSuggestionsBulk(items);
      const ok = res.attached.length;
      const bad = res.failed.length;
      toast = bad === 0
        ? `Applied ${ok} suggestion${ok === 1 ? "" : "s"}.`
        : `Applied ${ok}; ${bad} failed.`;
      onApplied(ok);
      selected = new Set();
      await refresh();
      // Auto-hide toast after 4s.
      setTimeout(() => {
        toast = null;
      }, 4000);
    } catch (e) {
      errorMsg = String(e);
    } finally {
      applying = false;
    }
  }

  async function dismissChip(docId: number, name: string) {
    try {
      // Reuses the per-doc dismissTagSuggestion via the row primitive.
      const mod = await import("$lib/library");
      await mod.dismissTagSuggestion(docId, name);
      // Remove the suggestion from the in-memory group; if the group
      // empties, drop it too.
      groups = groups
        .map((g) =>
          g.doc_id === docId
            ? {
                ...g,
                suggestions: g.suggestions.filter((s) => s.tag_name !== name),
              }
            : g,
        )
        .filter((g) => g.suggestions.length > 0);
      // Drop selection for the dismissed pair.
      const next = new Set(selected);
      next.delete(keyFor(docId, name));
      selected = next;
    } catch (e) {
      errorMsg = String(e);
    }
  }

  async function loadDismissedFor(docId: number) {
    activeDismissedDoc = docId;
    dismissed = [];
    try {
      dismissed = await listDismissedTagSuggestions(docId);
    } catch (e) {
      errorMsg = String(e);
    }
  }

  async function undismissOne(name: string) {
    if (activeDismissedDoc == null) return;
    try {
      await undismissOneTagSuggestion(activeDismissedDoc, name);
      dismissed = dismissed.filter((d) => d.tag_name !== name);
      await refresh();
    } catch (e) {
      errorMsg = String(e);
    }
  }

  function sourceGlyph(src: TagSuggestion["source"]): string {
    if (src === "cooccurrence") return "\u26AD"; // ⚭
    if (src === "domain") return "\u2317"; // ⌗
    return "\u2666"; // ♦
  }
  function sourceLabel(src: TagSuggestion["source"]): string {
    if (src === "cooccurrence") return "co-occurrence";
    if (src === "domain") return "domain";
    return "vocabulary";
  }

  function pastelFor(name: string): string {
    let h = 0x811c9dc5;
    for (let i = 0; i < name.length; i++) {
      h ^= name.charCodeAt(i);
      h = Math.imul(h, 0x01000193) >>> 0;
    }
    return `hsl(${h % 360}, 60%, 80%)`;
  }

  function relativeTime(unix: number): string {
    const now = Math.floor(Date.now() / 1000);
    const diff = now - unix;
    if (diff < 60) return "just now";
    if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
    if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
    return `${Math.floor(diff / 86400)}d ago`;
  }

  function onOverlayClick(e: MouseEvent) {
    if (e.target === e.currentTarget) onClose();
  }

  let totalSuggestions = $derived(
    groups.reduce((sum, g) => sum + g.suggestions.length, 0),
  );
  let badgeText = $derived(() => {
    if (!stats) return "";
    const n = stats.untagged_docs_with_suggestions;
    return n >= 200 ? "200+" : String(n);
  });
</script>

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="bts-overlay" onclick={onOverlayClick}>
    <div class="bts-drawer" role="dialog" aria-label="Bulk tag suggestions" aria-modal="true">
      <header class="bts-head">
        <div class="bts-title">
          <span class="bts-glyph" aria-hidden="true">✨</span>
          <div>
            <h2>Review tag suggestions</h2>
            <p class="bts-sub">
              {#if stats}
                {stats.untagged_docs_with_suggestions} untagged doc{stats.untagged_docs_with_suggestions === 1 ? "" : "s"} have plausible tags
                · {stats.dismissed_total} dismissed total
              {:else}
                Probing the library…
              {/if}
            </p>
          </div>
        </div>
        <button
          class="bts-close"
          type="button"
          aria-label="Close review panel"
          onclick={onClose}>×</button>
      </header>

      <section class="bts-source">
        <div class="bts-source-row">
          <span class="bts-label">Source</span>
          <div class="bts-segment">
            <button
              class="bts-seg"
              class:on={sourceMode === "untagged"}
              type="button"
              onclick={() => {
                sourceMode = "untagged";
                refresh();
              }}>Untagged only</button>
            <button
              class="bts-seg"
              class:on={sourceMode === "filter"}
              type="button"
              disabled={!filter}
              title={filter ? "Use the active library filter" : "No active filter"}
              onclick={() => {
                sourceMode = "filter";
                refresh();
              }}>Current filter</button>
          </div>
        </div>
        <div class="bts-source-row">
          <span class="bts-label">Per-doc cap</span>
          <input
            class="bts-num"
            type="number"
            min="5"
            max="500"
            step="5"
            bind:value={perDocCap}
            onchange={refresh} />
        </div>
      </section>

      <section class="bts-toolbar">
        <button
          class="bts-btn ghost"
          type="button"
          onclick={refresh}
          disabled={loading}>{loading ? "Refreshing…" : "Refresh"}</button>
        <div class="bts-chips">
          <button
            class="bts-chip"
            type="button"
            disabled={totalSuggestions === 0}
            onclick={selectAllVisible}>All ({totalSuggestions})</button>
          <button
            class="bts-chip"
            type="button"
            title="Select every vocabulary-source chip"
            disabled={totalSuggestions === 0}
            onclick={() => selectAllBySource("vocabulary")}>♦ vocab</button>
          <button
            class="bts-chip"
            type="button"
            title="Select every co-occurrence-source chip"
            disabled={totalSuggestions === 0}
            onclick={() => selectAllBySource("cooccurrence")}>⚭ co-occ</button>
          <button
            class="bts-chip"
            type="button"
            title="Select every domain-hint chip"
            disabled={totalSuggestions === 0}
            onclick={() => selectAllBySource("domain")}>⌗ domain</button>
          {#if selected.size > 0}
            <button
              class="bts-chip danger"
              type="button"
              onclick={clearSelection}>Clear ({selected.size})</button>
          {/if}
        </div>
        <button
          class="bts-btn primary"
          type="button"
          disabled={selected.size === 0 || applying}
          onclick={applySelected}>
          {applying ? "Applying…" : `Apply ${selected.size}`}
        </button>
      </section>

      {#if errorMsg}
        <div class="bts-error">{errorMsg}</div>
      {/if}

      <section class="bts-grid">
        {#if loading}
          <div class="bts-empty">Loading suggestions…</div>
        {:else if groups.length === 0}
          <div class="bts-empty">
            <p>Nothing to review right now.</p>
            <p class="bts-empty-sub">
              The suggester found no untagged docs with plausible tags
              {#if sourceMode === "filter"}
                in the current filter
              {/if}.
            </p>
            {#if stats && stats.dismissed_total > 0}
              <button
                class="bts-btn ghost"
                type="button"
                onclick={() => {
                  dismissedExpanded = !dismissedExpanded;
                  if (dismissedExpanded && groups.length === 0) {
                    // Surface the first dismissed list for any doc id (UI hint).
                    // Without a doc context the inspector path is the right entry.
                  }
                }}>
                {dismissedExpanded ? "Hide dismissed" : `Show dismissed (${stats.dismissed_total})`}
              </button>
            {/if}
          </div>
        {:else}
          {#each groups as g (g.doc_id)}
            <article class="bts-card">
              <header class="bts-card-head">
                <div class="bts-card-title" title={g.path}>
                  {g.title ?? g.path.split("/").pop() ?? `Doc #${g.doc_id}`}
                </div>
                <button
                  class="bts-card-hidden"
                  type="button"
                  title="View dismissed suggestions for this doc"
                  onclick={() => {
                    loadDismissedFor(g.doc_id);
                    dismissedExpanded = true;
                  }}>Hidden…</button>
              </header>
              <div class="bts-suggs">
                {#each g.suggestions as s (s.tag_name)}
                  <label
                    class="bts-sugg"
                    class:selected={isSelected(g.doc_id, s.tag_name)}
                    style:--chip-accent={pastelFor(s.tag_name)}>
                    <input
                      type="checkbox"
                      checked={isSelected(g.doc_id, s.tag_name)}
                      onchange={() => toggleSelected(g.doc_id, s.tag_name)} />
                    <span class="bts-sugg-glyph" title={sourceLabel(s.source)}>
                      {sourceGlyph(s.source)}
                    </span>
                    <span class="bts-sugg-name">{s.tag_name}</span>
                    <button
                      class="bts-sugg-dismiss"
                      type="button"
                      title="Never suggest this for this document"
                      onclick={(e) => {
                        e.preventDefault();
                        dismissChip(g.doc_id, s.tag_name);
                      }}>✗</button>
                  </label>
                {/each}
              </div>
            </article>
          {/each}
        {/if}
      </section>

      {#if dismissedExpanded}
        <section class="bts-dismissed">
          <header class="bts-dismissed-head">
            <span>Hidden suggestions {activeDismissedDoc != null ? `for doc #${activeDismissedDoc}` : ""}</span>
            <button
              class="bts-btn ghost small"
              type="button"
              onclick={() => {
                dismissedExpanded = false;
                activeDismissedDoc = null;
                dismissed = [];
              }}>Close</button>
          </header>
          {#if dismissed.length === 0}
            <p class="bts-empty-sub">
              {activeDismissedDoc != null
                ? "No dismissals on this doc."
                : "Click a card's 'Hidden…' link above to load its dismissals."}
            </p>
          {:else}
            <ul class="bts-dismissed-list">
              {#each dismissed as d (d.tag_name)}
                <li>
                  <span
                    class="bts-dismissed-chip"
                    style:--chip-accent={pastelFor(d.tag_name)}>{d.tag_name}</span>
                  <span class="bts-dismissed-when">{relativeTime(d.dismissed_at)}</span>
                  <button
                    class="bts-btn ghost small"
                    type="button"
                    onclick={() => undismissOne(d.tag_name)}>Undo</button>
                </li>
              {/each}
            </ul>
          {/if}
        </section>
      {/if}

      {#if toast}
        <div class="bts-toast" role="status">{toast}</div>
      {/if}
    </div>
  </div>
{/if}

<style>
  .bts-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.32);
    z-index: 1000;
    display: flex;
    justify-content: flex-end;
    animation: bts-fade 180ms ease;
  }
  @keyframes bts-fade {
    from { background: rgba(0, 0, 0, 0); }
    to { background: rgba(0, 0, 0, 0.32); }
  }
  .bts-drawer {
    width: 560px;
    max-width: 96vw;
    height: 100vh;
    background: var(--bg-1);
    border-left: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    overflow-y: auto;
    box-shadow: -8px 0 32px rgba(0, 0, 0, 0.28);
    animation: bts-slide 180ms cubic-bezier(0.16, 1, 0.3, 1);
  }
  @keyframes bts-slide {
    from { transform: translateX(40px); opacity: 0; }
    to { transform: translateX(0); opacity: 1; }
  }
  .bts-head {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 12px;
    padding: 16px 18px 12px;
    border-bottom: 1px solid var(--border);
    position: sticky;
    top: 0;
    background: var(--bg-1);
    z-index: 2;
  }
  .bts-title {
    display: flex;
    gap: 12px;
    align-items: flex-start;
  }
  .bts-glyph {
    font-size: 22px;
    line-height: 1;
    color: var(--accent, #7c3aed);
  }
  .bts-title h2 {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
    color: var(--text-1);
  }
  .bts-sub {
    margin: 2px 0 0 0;
    font-size: 11px;
    color: var(--text-3);
  }
  .bts-close {
    background: transparent;
    border: 1px solid var(--border);
    color: var(--text-2);
    width: 30px;
    height: 30px;
    border-radius: 6px;
    font-size: 18px;
    line-height: 1;
    cursor: pointer;
  }
  .bts-close:hover { background: var(--surface-2); color: var(--text-1); }

  .bts-source {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px 18px;
    border-bottom: 1px solid var(--border);
    background: color-mix(in oklab, var(--surface-2, #1b1b1f) 50%, transparent);
  }
  .bts-source-row {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .bts-label {
    font-size: 11px;
    font-weight: 600;
    color: var(--text-2);
    width: 88px;
    text-transform: uppercase;
    letter-spacing: 0.03em;
  }
  .bts-segment {
    display: inline-flex;
    border: 1px solid var(--border);
    border-radius: 6px;
    overflow: hidden;
  }
  .bts-seg {
    background: transparent;
    border: 0;
    color: var(--text-2);
    padding: 5px 10px;
    font-size: 12px;
    cursor: pointer;
  }
  .bts-seg:disabled { opacity: 0.4; cursor: not-allowed; }
  .bts-seg.on {
    background: var(--accent, #7c3aed);
    color: #fff;
  }
  .bts-num {
    width: 88px;
    padding: 4px 8px;
    border: 1px solid var(--border);
    border-radius: 5px;
    background: var(--surface-2);
    color: var(--text-1);
    font-size: 12px;
  }

  .bts-toolbar {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    padding: 10px 18px;
    border-bottom: 1px solid var(--border);
  }
  .bts-chips {
    display: flex;
    gap: 6px;
    flex: 1;
    flex-wrap: wrap;
  }
  .bts-chip {
    background: var(--surface-2);
    border: 1px solid var(--border);
    color: var(--text-2);
    padding: 4px 9px;
    border-radius: 999px;
    font-size: 11px;
    cursor: pointer;
  }
  .bts-chip:hover:not(:disabled) {
    background: var(--surface-3);
    color: var(--text-1);
  }
  .bts-chip:disabled { opacity: 0.4; cursor: not-allowed; }
  .bts-chip.danger {
    border-color: color-mix(in oklab, #ff6464 50%, var(--border));
    color: #ff8a8a;
  }
  .bts-btn {
    background: var(--surface-2);
    border: 1px solid var(--border);
    color: var(--text-1);
    padding: 6px 12px;
    border-radius: 6px;
    font-size: 12px;
    cursor: pointer;
  }
  .bts-btn.small { padding: 3px 8px; font-size: 11px; }
  .bts-btn.ghost { background: transparent; }
  .bts-btn.primary {
    background: var(--accent, #7c3aed);
    border-color: var(--accent, #7c3aed);
    color: #fff;
    font-weight: 600;
  }
  .bts-btn:disabled { opacity: 0.4; cursor: not-allowed; }
  .bts-btn:hover:not(:disabled) {
    filter: brightness(1.08);
  }

  .bts-grid {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px 18px 18px;
  }
  .bts-empty {
    padding: 32px 16px;
    text-align: center;
    color: var(--text-2);
  }
  .bts-empty-sub {
    font-size: 12px;
    color: var(--text-3);
    margin: 6px 0 12px;
  }
  .bts-card {
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 10px 12px;
  }
  .bts-card-head {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 8px;
    margin-bottom: 8px;
  }
  .bts-card-title {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-1);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .bts-card-hidden {
    background: transparent;
    border: 0;
    color: var(--text-3);
    font-size: 10px;
    cursor: pointer;
    text-decoration: underline dotted;
    flex-shrink: 0;
  }
  .bts-card-hidden:hover { color: var(--text-1); }
  .bts-suggs {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
  }
  .bts-sugg {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 3px 7px 3px 5px;
    background: var(--surface-3, #26262c);
    border: 1px solid var(--border);
    border-left: 3px solid var(--chip-accent, var(--accent));
    border-radius: 999px;
    font-size: 11px;
    color: var(--text-1);
    cursor: pointer;
    user-select: none;
    transition: background 100ms ease;
  }
  .bts-sugg.selected {
    background: color-mix(in oklab, var(--chip-accent, var(--accent)) 22%, var(--surface-3));
    border-color: var(--chip-accent, var(--accent));
  }
  .bts-sugg input[type="checkbox"] {
    accent-color: var(--accent, #7c3aed);
    width: 11px;
    height: 11px;
    cursor: pointer;
  }
  .bts-sugg-glyph { font-size: 10px; opacity: 0.7; }
  .bts-sugg-name { color: var(--text-1); }
  .bts-sugg-dismiss {
    background: transparent;
    border: 0;
    color: var(--text-3);
    cursor: pointer;
    font-size: 11px;
    padding: 0 2px;
    margin-left: 2px;
  }
  .bts-sugg-dismiss:hover { color: #ff8a8a; }

  .bts-dismissed {
    border-top: 1px solid var(--border);
    padding: 12px 18px;
  }
  .bts-dismissed-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 8px;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    color: var(--text-2);
  }
  .bts-dismissed-list {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .bts-dismissed-list li {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .bts-dismissed-chip {
    padding: 2px 8px;
    background: var(--surface-3);
    border: 1px solid var(--border);
    border-left: 3px solid var(--chip-accent, var(--accent));
    border-radius: 999px;
    font-size: 11px;
    color: var(--text-1);
  }
  .bts-dismissed-when {
    font-size: 10px;
    color: var(--text-3);
    flex: 1;
  }

  .bts-error {
    margin: 8px 18px 0;
    padding: 8px 12px;
    background: rgba(255, 100, 100, 0.12);
    border: 1px solid rgba(255, 100, 100, 0.32);
    color: #ffb0b0;
    font-size: 11px;
    border-radius: 6px;
  }

  .bts-toast {
    position: sticky;
    bottom: 12px;
    margin: 12px 18px 0;
    padding: 8px 12px;
    background: var(--accent, #7c3aed);
    color: #fff;
    font-size: 12px;
    border-radius: 6px;
    text-align: center;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.22);
  }

  @media (prefers-reduced-motion: reduce) {
    .bts-overlay, .bts-drawer { animation: none !important; }
  }
</style>
