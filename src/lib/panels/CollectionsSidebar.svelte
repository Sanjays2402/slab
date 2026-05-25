<!--
  Atlas Collections sidebar rail section (v3.32.0).

  Renders user-curated Collections and Smart Collections under the
  Library panel's left rail. Pulses the count badge whenever the count
  grows — that's the "wow" moment when a freshly-scanned doc lands in a
  Smart Collection like "Recently added".

  Click a row -> emit `select` with the resolved DocumentRecord[]. The
  parent (LibraryPanel) decides what to do (we override its filtered
  view in v3.32.0).
-->
<script lang="ts">
  import { onMount } from "svelte";
  import {
    collectionList,
    collectionCreate,
    collectionDelete,
    collectionListDocs,
    smartCollectionList,
    smartCollectionExpand,
    type CollectionRecord,
    type SmartCollectionRecord,
    type DocumentRecord,
  } from "$lib/library";

  type SelectPayload = {
    kind: "collection" | "smart";
    id: number;
    name: string;
    docs: DocumentRecord[];
  };

  let { onSelect = (_: SelectPayload) => {} }: { onSelect?: (p: SelectPayload) => void } = $props();

  let collections = $state<CollectionRecord[]>([]);
  let smart = $state<SmartCollectionRecord[]>([]);
  let activeId = $state<string | null>(null);
  let creating = $state(false);
  let newName = $state("");
  let loading = $state(true);
  let error = $state<string | null>(null);

  // Pulse tracking — { collectionId: previousCount } so we can detect a
  // delta on the next refresh and add a one-shot CSS class.
  let prevCounts = $state<Map<number, number>>(new Map());
  let pulsing = $state<Set<number>>(new Set());

  async function refresh() {
    try {
      loading = true;
      const [cs, ss] = await Promise.all([collectionList(), smartCollectionList()]);
      const next = new Map<number, number>();
      const grew = new Set<number>();
      for (const c of cs) {
        next.set(c.id, c.doc_count);
        const prev = prevCounts.get(c.id);
        if (prev !== undefined && c.doc_count > prev) grew.add(c.id);
      }
      collections = cs;
      smart = ss;
      prevCounts = next;
      if (grew.size > 0) {
        pulsing = new Set([...pulsing, ...grew]);
        setTimeout(() => {
          const fresh = new Set(pulsing);
          for (const id of grew) fresh.delete(id);
          pulsing = fresh;
        }, 700);
      }
      error = null;
    } catch (e) {
      error = (e as Error).message;
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    refresh();
    // Library mutations broadcast `library-changed`; piggy-back on it.
    const handler = () => refresh();
    window.addEventListener("library-changed", handler);
    return () => window.removeEventListener("library-changed", handler);
  });

  async function handleCreate() {
    const name = newName.trim();
    if (!name) return;
    try {
      await collectionCreate(name, "folder", "#a78bfa");
      newName = "";
      creating = false;
      await refresh();
    } catch (e) {
      error = (e as Error).message;
    }
  }

  async function pickCollection(c: CollectionRecord) {
    activeId = `c:${c.id}`;
    const docs = await collectionListDocs(c.id);
    onSelect({ kind: "collection", id: c.id, name: c.name, docs });
  }

  async function pickSmart(s: SmartCollectionRecord) {
    activeId = `s:${s.id}`;
    const docs = await smartCollectionExpand(s.id);
    onSelect({ kind: "smart", id: s.id, name: s.name, docs });
  }

  async function handleDelete(c: CollectionRecord, ev: MouseEvent) {
    ev.stopPropagation();
    if (!confirm(`Delete collection "${c.name}"? (Docs inside stay in your library.)`)) return;
    await collectionDelete(c.id);
    await refresh();
  }
</script>

<section class="cs-rail">
  <header class="cs-head">
    <span class="cs-title">Collections</span>
    <button class="cs-add" aria-label="New collection" onclick={() => (creating = !creating)}>
      +
    </button>
  </header>

  {#if creating}
    <form
      class="cs-new"
      onsubmit={(e) => {
        e.preventDefault();
        handleCreate();
      }}
    >
      <input
        class="cs-input"
        type="text"
        placeholder="Name your collection…"
        bind:value={newName}
        autofocus
      />
      <button class="cs-save" type="submit" disabled={!newName.trim()}>Create</button>
    </form>
  {/if}

  {#if loading && collections.length === 0 && smart.length === 0}
    <div class="cs-empty">Loading…</div>
  {:else}
    {#each collections as c (c.id)}
      <div class="cs-row-wrap" class:active={activeId === `c:${c.id}`}>
        <button
          class="cs-row"
          class:active={activeId === `c:${c.id}`}
          onclick={() => pickCollection(c)}
          title={c.name}
        >
          <span class="cs-dot" style:background={c.color ?? "var(--text-3)"}></span>
          <span class="cs-label">{c.name}</span>
          <span class="cs-count" class:pulse={pulsing.has(c.id)}>{c.doc_count}</span>
        </button>
        <button
          class="cs-x"
          aria-label="Delete {c.name}"
          onclick={(e) => handleDelete(c, e)}
        >×</button>
      </div>
    {/each}

    {#if smart.length > 0}
      <div class="cs-sub">Smart</div>
      {#each smart as s (s.id)}
        <button
          class="cs-row smart"
          class:active={activeId === `s:${s.id}`}
          onclick={() => pickSmart(s)}
          title={s.name}
        >
          <span class="cs-dot diamond" style:background={s.color ?? "var(--accent)"}></span>
          <span class="cs-label">{s.name}</span>
        </button>
      {/each}
    {/if}

    {#if collections.length === 0 && smart.length === 0}
      <div class="cs-empty">No collections yet — click + to make one.</div>
    {/if}
  {/if}

  {#if error}
    <div class="cs-err">{error}</div>
  {/if}
</section>

<style>
  .cs-rail {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 10px 6px;
  }
  .cs-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 8px 6px;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-3);
  }
  .cs-title {
    font-weight: 600;
  }
  .cs-add {
    background: transparent;
    border: none;
    color: var(--text-2);
    font-size: 16px;
    line-height: 1;
    cursor: pointer;
    padding: 2px 6px;
    border-radius: 6px;
    transition: background 120ms ease, color 120ms ease;
  }
  .cs-add:hover {
    background: var(--surface-2);
    color: var(--text-1);
  }
  .cs-new {
    display: flex;
    gap: 4px;
    padding: 4px 8px 8px;
  }
  .cs-input {
    flex: 1;
    background: var(--surface-2);
    border: 1px solid var(--border-1);
    border-radius: 6px;
    padding: 4px 8px;
    color: var(--text-1);
    font-size: 12px;
    outline: none;
  }
  .cs-input:focus {
    border-color: var(--accent);
  }
  .cs-save {
    background: var(--accent);
    color: #fff;
    border: none;
    border-radius: 6px;
    padding: 0 10px;
    font-size: 12px;
    cursor: pointer;
  }
  .cs-save:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  .cs-row-wrap {
    display: flex;
    align-items: center;
  }
  .cs-row {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    background: transparent;
    border: none;
    padding: 6px 8px;
    border-radius: 6px;
    cursor: pointer;
    text-align: left;
    color: var(--text-2);
    font-size: 13px;
    transition: background 120ms ease, color 120ms ease;
  }
  .cs-row:hover {
    background: var(--surface-2);
    color: var(--text-1);
  }
  .cs-row.active {
    background: color-mix(in oklab, var(--accent) 18%, transparent);
    color: var(--text-1);
  }
  .cs-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex-shrink: 0;
  }
  .cs-dot.diamond {
    transform: rotate(45deg);
    border-radius: 1px;
  }
  .cs-label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .cs-count {
    background: var(--surface-2);
    color: var(--text-3);
    border-radius: 999px;
    padding: 1px 8px;
    font-size: 11px;
    font-variant-numeric: tabular-nums;
    transition:
      transform 220ms cubic-bezier(0.34, 1.56, 0.64, 1),
      background 180ms ease,
      color 180ms ease;
  }
  .cs-count.pulse {
    background: var(--accent);
    color: #fff;
    transform: scale(1.18);
  }
  .cs-x {
    background: transparent;
    border: none;
    color: var(--text-3);
    cursor: pointer;
    padding: 0 4px;
    font-size: 14px;
    line-height: 1;
    opacity: 0;
    transition: opacity 120ms ease, color 120ms ease;
  }
  .cs-row:hover .cs-x {
    opacity: 1;
  }
  .cs-x:hover {
    color: var(--danger, #f87171);
  }
  .cs-sub {
    padding: 12px 8px 4px;
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.07em;
    color: var(--text-3);
  }
  .cs-empty {
    padding: 8px;
    font-size: 12px;
    color: var(--text-3);
    font-style: italic;
  }
  .cs-err {
    padding: 6px 8px;
    font-size: 11px;
    color: var(--danger, #f87171);
  }
</style>
