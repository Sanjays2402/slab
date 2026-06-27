<script lang="ts">
  // Atlas Lite — RecentsHome
  //
  // Slab's first-launch hero: the "Continue reading" card + pinned row + the
  // full recents grid with thumbnails and progress dots. Replaces the bare
  // empty-state that ReaderPanel rendered when no document was open.
  //
  // Buy-Button rationale:
  //   - Acrobat's Home tab is universally hated for being noisy and slow.
  //   - PDF Expert's recents are flat — no progress, no resume.
  //   - We pick the one feature both miss (resume) and make it the hero.
  //
  // This component is read-only state: it derives everything from the
  // `listRecent()` store and re-subscribes via `subscribeRecent`. Mutation
  // helpers (pin / remove) are wired through callbacks the parent passes in.

  import {
    listRecent,
    subscribeRecent,
    formatRelTime,
    getRecentThumb,
    pinRecent,
    removeRecent,
    type RecentFile,
  } from "$lib/recent";
  import {
    partitionRecents,
    filterRecents,
    highlightRecentName,
  } from "$lib/recentsHomeView";
  import { notify } from "$lib/notify";
  import { basename } from "$lib/types";
  import { onMount, onDestroy } from "svelte";

  type Props = {
    onOpen: (file: RecentFile) => void;
    onPick?: () => void;            // "Open a document" pick-from-disk
    onContinue?: () => void;        // "Continue reading" big button
    loading?: boolean;
  };
  let { onOpen, onPick, onContinue, loading = false }: Props = $props();

  let recents = $state<RecentFile[]>(listRecent());
  let unsub: (() => void) | null = null;
  onMount(() => {
    unsub = subscribeRecent((files) => {
      recents = files;
    });
  });
  onDestroy(() => {
    unsub?.();
  });

  // Slice 2 (round 44): filter-as-you-type. A user near the 12-file recents
  // cap had no way to jump to one file — the grid was eyeball-only. The
  // filter input narrows the board live, reusing the tested palette scorer
  // (via filterRecents / highlightRecentName) so ranking + <mark> highlight
  // behave EXACTLY like Cmd+K and the library search panel.
  let query = $state("");
  let filterEl: HTMLInputElement | null = $state(null);
  const q = $derived(query.trim());
  const filtering = $derived(q.length > 0);

  // The "Continue reading" hero card surfaces the single most useful next
  // action: the file with the freshest reading momentum. The selection +
  // partition math now lives in the tested pure core (recentsHomeView.ts),
  // so the contract that decides what the app's headline card even is has
  // unit tests instead of an untested inline $derived. `partitionRecents`
  // returns { hero, pinned, others } exactly mirroring the render regions.
  //
  // While a filter is active the hero collapses and the board becomes a
  // flat matched list (pinned strip + recents grid both filtered); with no
  // filter the partition's hero-aware split is used unchanged.
  const partition = $derived(partitionRecents(recents));
  const matched = $derived(filtering ? filterRecents(recents, q) : recents);
  const continueCandidate = $derived(filtering ? null : partition.hero);
  const pinned = $derived(filtering ? matched.filter((r) => r.pinned) : partition.pinned);
  const others = $derived(filtering ? matched.filter((r) => !r.pinned) : partition.others);

  function progressPct(r: RecentFile): number {
    if (!r.lastPage || !r.totalPages || r.totalPages <= 0) return 0;
    return Math.max(0, Math.min(100, Math.round((r.lastPage / r.totalPages) * 100)));
  }

  function dots(r: RecentFile, count = 8): boolean[] {
    if (!r.lastPage || !r.totalPages || r.totalPages <= 0) return [];
    const filled = Math.round((r.lastPage / r.totalPages) * count);
    return Array.from({ length: count }, (_, i) => i < filled);
  }

  function handlePin(e: MouseEvent, r: RecentFile) {
    e.stopPropagation();
    pinRecent(r.path);
    notify.success(r.pinned ? `Unpinned ${r.name}` : `Pinned ${r.name}`);
  }

  function handleRemove(e: MouseEvent, r: RecentFile) {
    e.stopPropagation();
    removeRecent(r.path);
    notify.info(`Removed ${r.name} from recents`);
  }

  function continueReading() {
    if (continueCandidate) {
      onContinue?.();
      onOpen(continueCandidate);
    }
  }

  // Pulse animation trigger: when continueCandidate changes (i.e. on resume
  // landing), the progress bar pulses once to draw the eye.
  let heroKey = $state(0);
  $effect(() => {
    // touch the path so $derived reruns
    const _path = continueCandidate?.path;
    heroKey++;
    void _path;
  });
</script>

<!-- Monochrome glyphs (Slab chrome is icon-only, never emoji). -->
{#snippet pinGlyph()}
  <svg class="ico" viewBox="0 0 24 24" aria-hidden="true">
    <path d="M9 4h6l-1 6 3 3v2H7v-2l3-3-1-6z" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round" />
    <path d="M12 15v5" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
  </svg>
{/snippet}
{#snippet removeGlyph()}
  <svg class="ico" viewBox="0 0 24 24" aria-hidden="true">
    <path d="M6 6l12 12M18 6L6 18" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
  </svg>
{/snippet}
{#snippet searchGlyph()}
  <svg class="ico" viewBox="0 0 24 24" aria-hidden="true">
    <circle cx="11" cy="11" r="6" fill="none" stroke="currentColor" stroke-width="2" />
    <path d="M16 16l4 4" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
  </svg>
{/snippet}
<!-- Renders a recent file's name with a live <mark> over the matched range. -->
{#snippet nameSegs(name: string)}
  {#each highlightRecentName(name, q) as seg}
    {#if seg.hit}<mark class="hl">{seg.text}</mark>{:else}{seg.text}{/if}
  {/each}
{/snippet}

<div class="recents-home">
  {#if recents.length > 0}
    <div class="filter-bar">
      <span class="filter-ico" aria-hidden="true">{@render searchGlyph()}</span>
      <input
        bind:this={filterEl}
        class="filter-input"
        type="text"
        placeholder="Filter recent documents…"
        bind:value={query}
        spellcheck="false"
        autocomplete="off"
        aria-label="Filter recent documents"
        onkeydown={(e) => {
          if (e.key === "Escape" && query) {
            e.preventDefault();
            e.stopPropagation();
            query = "";
          }
        }}
      />
      {#if filtering}
        <button
          class="filter-clear"
          onclick={() => {
            query = "";
            filterEl?.focus();
          }}
          aria-label="Clear filter"
          title="Clear filter (Esc)"
        >Clear</button>
      {/if}
    </div>
  {/if}

  {#if continueCandidate && continueCandidate.lastPage && continueCandidate.totalPages}
    <!-- Hero: a user with reading momentum gets resume as the headline. -->
    <button class="hero-card resume" onclick={continueReading} title={continueCandidate.path}>
      <div class="hero-thumb">
        {#if getRecentThumb(continueCandidate.path)}
          <img src={getRecentThumb(continueCandidate.path)} alt="" loading="eager" />
        {:else}
          <span class="hero-thumb-placeholder">PDF</span>
        {/if}
      </div>
      <div class="hero-body">
        <span class="hero-eyebrow">Continue reading</span>
        <h2 class="hero-title">{continueCandidate.name}</h2>
        <p class="hero-meta">
          Page {continueCandidate.lastPage} of {continueCandidate.totalPages}
          · {formatRelTime(continueCandidate.lastReadAt ?? continueCandidate.openedAt)}
        </p>
        <div class="progress-track" aria-label="Reading progress">
          {#key heroKey}
            <div class="progress-fill pulse" style="width: {progressPct(continueCandidate)}%"></div>
          {/key}
        </div>
        <div class="hero-cta">Resume <span class="hero-kbd">⌘0</span></div>
      </div>
    </button>
  {:else if continueCandidate}
    <!-- Hero (no progress yet): big "open last document" card. -->
    <button class="hero-card cold" onclick={continueReading} title={continueCandidate.path}>
      <div class="hero-thumb">
        {#if getRecentThumb(continueCandidate.path)}
          <img src={getRecentThumb(continueCandidate.path)} alt="" loading="eager" />
        {:else}
          <span class="hero-thumb-placeholder">PDF</span>
        {/if}
      </div>
      <div class="hero-body">
        <span class="hero-eyebrow">Most recent</span>
        <h2 class="hero-title">{continueCandidate.name}</h2>
        <p class="hero-meta">
          {#if continueCandidate.pageCount}{continueCandidate.pageCount} pages · {/if}
          {formatRelTime(continueCandidate.openedAt)}
        </p>
        <div class="hero-cta">Open <span class="hero-kbd">⌘0</span></div>
      </div>
    </button>
  {:else}
    <!-- True empty state — first launch -->
    <button class="hero-card empty" onclick={() => onPick?.()} disabled={loading}>
      <div class="empty-icon">+</div>
      <div class="hero-body">
        <h2 class="hero-title">{loading ? "Loading…" : "Open your first document"}</h2>
        <p class="hero-meta">
          PDF, Office, HTML, EPUB, CSV, images. Drag-drop or click. Files stay on your machine.
        </p>
        <div class="hero-cta">Choose a file <span class="hero-kbd">⌘O</span></div>
      </div>
    </button>
  {/if}

  {#if pinned.length > 0}
    <section class="row">
      <header class="row-head">
        <span class="row-label">Pinned</span>
        <span class="row-hint">{pinned.length} file{pinned.length === 1 ? "" : "s"}</span>
      </header>
      <div class="row-strip">
        {#each pinned as r (r.path)}
          {@const thumb = getRecentThumb(r.path)}
          <div class="card pinned">
            <button class="card-body" onclick={() => onOpen(r)} title={r.path}>
              <div class="card-thumb">
                {#if thumb}
                  <img src={thumb} alt="" loading="lazy" />
                {:else}
                  <span class="card-thumb-placeholder">PDF</span>
                {/if}
                <span class="pin-flag" aria-hidden="true">{@render pinGlyph()}</span>
              </div>
              <span class="card-name">{@render nameSegs(r.name)}</span>
              <span class="card-meta">
                {#if r.pageCount}{r.pageCount}p · {/if}{formatRelTime(r.openedAt)}
              </span>
              {#if r.lastPage && r.totalPages}
                <div class="dots" aria-label="Reading progress">
                  {#each dots(r) as on}
                    <span class="dot" class:on></span>
                  {/each}
                </div>
              {/if}
            </button>
            <div class="card-actions">
              <button class="act" title="Unpin" aria-label="Unpin" onclick={(e) => handlePin(e, r)}>{@render pinGlyph()}</button>
              <button class="act danger" title="Remove" aria-label="Remove" onclick={(e) => handleRemove(e, r)}>{@render removeGlyph()}</button>
            </div>
          </div>
        {/each}
      </div>
    </section>
  {/if}

  {#if others.length > 0}
    <section class="row">
      <header class="row-head">
        <span class="row-label">{filtering ? "Results" : "Recent"}</span>
        <span class="row-hint">{others.length} file{others.length === 1 ? "" : "s"}</span>
      </header>
      <div class="grid">
        {#each others as r (r.path)}
          {@const thumb = getRecentThumb(r.path)}
          <div class="card">
            <button class="card-body" onclick={() => onOpen(r)} title={r.path}>
              <div class="card-thumb">
                {#if thumb}
                  <img src={thumb} alt="" loading="lazy" />
                {:else}
                  <span class="card-thumb-placeholder">{basename(r.name).slice(0, 3).toUpperCase()}</span>
                {/if}
              </div>
              <span class="card-name">{@render nameSegs(r.name)}</span>
              <span class="card-meta">
                {#if r.pageCount}{r.pageCount}p · {/if}{formatRelTime(r.openedAt)}
              </span>
              {#if r.lastPage && r.totalPages}
                <div class="dots" aria-label="Reading progress">
                  {#each dots(r) as on}
                    <span class="dot" class:on></span>
                  {/each}
                </div>
              {/if}
            </button>
            <div class="card-actions">
              <button class="act" title="Pin to top" aria-label="Pin" onclick={(e) => handlePin(e, r)}>{@render pinGlyph()}</button>
              <button class="act danger" title="Remove" aria-label="Remove" onclick={(e) => handleRemove(e, r)}>{@render removeGlyph()}</button>
            </div>
          </div>
        {/each}
      </div>
    </section>
  {/if}

  {#if filtering && pinned.length === 0 && others.length === 0}
    <div class="filter-empty">
      <p class="filter-empty-line">
        No recent documents match <span class="filter-empty-q">“{q}”</span>.
      </p>
      <button class="filter-empty-reset" onclick={() => { query = ""; filterEl?.focus(); }}>
        Clear filter
      </button>
    </div>
  {/if}
</div>

<style>
  /* Liquid Glass — frosted card, soft ring, subtle hover lift. */
  .recents-home {
    display: flex;
    flex-direction: column;
    gap: 1.5rem;
    padding: 0;
  }

  /* Slice 2 — filter bar: palette-grade filter-as-you-type. */
  .filter-bar {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.5rem 0.75rem;
    border-radius: 12px;
    border: 1px solid var(--border-1, rgba(255,255,255,0.08));
    background: var(--surface-2, rgba(255,255,255,0.03));
    transition: border-color 140ms ease, box-shadow 140ms ease;
  }
  .filter-bar:focus-within {
    border-color: var(--accent, #5e6ad2);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent, #5e6ad2) 22%, transparent);
  }
  .filter-ico { display: flex; opacity: 0.5; }
  .filter-ico .ico { width: 16px; height: 16px; }
  .filter-input {
    flex: 1;
    min-width: 0;
    background: transparent;
    border: 0;
    color: inherit;
    font-size: 0.9rem;
    outline: none;
  }
  .filter-input::placeholder { opacity: 0.45; }
  .filter-clear {
    appearance: none;
    border: 0;
    background: transparent;
    color: var(--accent, #5e6ad2);
    font-size: 0.8rem;
    font-weight: 600;
    cursor: pointer;
    padding: 0.15rem 0.35rem;
    border-radius: 6px;
  }
  .filter-clear:hover { background: color-mix(in srgb, var(--accent, #5e6ad2) 14%, transparent); }

  .hl {
    background: color-mix(in srgb, var(--accent, #5e6ad2) 36%, transparent);
    color: inherit;
    border-radius: 3px;
    padding: 0 1px;
  }

  .filter-empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.6rem;
    padding: 2.5rem 1rem;
    text-align: center;
    opacity: 0.85;
  }
  .filter-empty-line { margin: 0; opacity: 0.7; font-size: 0.95rem; }
  .filter-empty-q { color: var(--accent, #5e6ad2); font-weight: 600; }
  .filter-empty-reset {
    appearance: none;
    border: 1px solid var(--border-1, rgba(255,255,255,0.12));
    background: var(--surface-2, rgba(255,255,255,0.03));
    color: inherit;
    font-size: 0.85rem;
    font-weight: 500;
    cursor: pointer;
    padding: 0.4rem 0.9rem;
    border-radius: 9px;
  }
  .filter-empty-reset:hover { border-color: var(--accent, #5e6ad2); }

  .hero-card {
    display: grid;
    grid-template-columns: 220px 1fr;
    gap: 1.5rem;
    width: 100%;
    padding: 1.25rem;
    border-radius: 18px;
    background: linear-gradient(180deg, var(--surface-2, rgba(255,255,255,0.04)) 0%, var(--surface-1, rgba(255,255,255,0.02)) 100%);
    border: 1px solid var(--border-1, rgba(255,255,255,0.08));
    backdrop-filter: blur(20px) saturate(160%);
    -webkit-backdrop-filter: blur(20px) saturate(160%);
    box-shadow: 0 1px 0 0 rgba(255,255,255,0.04) inset, 0 12px 32px -16px rgba(0,0,0,0.45);
    text-align: left;
    cursor: pointer;
    transition: transform 160ms ease, border-color 160ms ease, box-shadow 160ms ease;
    color: inherit;
  }
  .hero-card:hover {
    transform: translateY(-1px);
    border-color: var(--border-2, rgba(255,255,255,0.16));
    box-shadow: 0 1px 0 0 rgba(255,255,255,0.06) inset, 0 18px 40px -16px rgba(0,0,0,0.55);
  }
  .hero-card:disabled { opacity: 0.5; cursor: progress; }
  .hero-card.empty { grid-template-columns: 96px 1fr; }

  .hero-thumb {
    position: relative;
    aspect-ratio: 3 / 4;
    border-radius: 10px;
    overflow: hidden;
    background: var(--surface-3, rgba(255,255,255,0.05));
    display: flex; align-items: center; justify-content: center;
  }
  .hero-thumb img { width: 100%; height: 100%; object-fit: cover; }
  .hero-thumb-placeholder {
    font-family: ui-monospace, SFMono-Regular, monospace;
    font-size: 24px; opacity: 0.5; letter-spacing: 2px;
  }

  .empty-icon {
    width: 96px; height: 96px; border-radius: 14px;
    background: var(--surface-3, rgba(255,255,255,0.05));
    display: flex; align-items: center; justify-content: center;
    font-size: 48px; font-weight: 200; opacity: 0.65;
  }

  .hero-body { display: flex; flex-direction: column; gap: 0.5rem; min-width: 0; }
  .hero-eyebrow {
    text-transform: uppercase;
    font-size: 11px;
    letter-spacing: 0.12em;
    opacity: 0.55;
    font-weight: 600;
  }
  .hero-title {
    font-size: 1.5rem;
    font-weight: 600;
    margin: 0;
    line-height: 1.2;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .hero-meta { opacity: 0.65; margin: 0; font-size: 0.9rem; }
  .hero-cta {
    margin-top: auto;
    align-self: flex-start;
    display: inline-flex; gap: 0.5rem; align-items: center;
    padding: 0.5rem 0.9rem;
    background: var(--accent, #5e6ad2);
    color: white;
    border-radius: 10px;
    font-weight: 600;
    font-size: 0.9rem;
  }
  .hero-kbd {
    font-family: ui-monospace, SFMono-Regular, monospace;
    font-size: 0.8em;
    padding: 0.1em 0.4em;
    background: rgba(0,0,0,0.18);
    border-radius: 4px;
  }

  .progress-track {
    height: 6px;
    width: 100%;
    border-radius: 999px;
    background: var(--surface-3, rgba(255,255,255,0.06));
    overflow: hidden;
    margin: 0.3rem 0;
  }
  .progress-fill {
    height: 100%;
    background: linear-gradient(90deg, var(--accent, #5e6ad2), var(--accent-2, #8b5cf6));
    border-radius: 999px;
    transition: width 400ms cubic-bezier(0.4, 0, 0.2, 1);
  }
  .progress-fill.pulse { animation: pulse 720ms cubic-bezier(0.4, 0, 0.2, 1) 1; }
  @keyframes pulse {
    0% { filter: brightness(1); transform: scaleY(1); }
    50% { filter: brightness(1.4); transform: scaleY(1.6); }
    100% { filter: brightness(1); transform: scaleY(1); }
  }

  .row { display: flex; flex-direction: column; gap: 0.6rem; }
  .row-head {
    display: flex; align-items: baseline; justify-content: space-between;
    padding: 0 0.25rem;
  }
  .row-label {
    font-size: 0.78rem;
    text-transform: uppercase;
    letter-spacing: 0.12em;
    font-weight: 600;
    opacity: 0.6;
  }
  .row-hint { font-size: 0.75rem; opacity: 0.45; }

  .row-strip {
    display: flex; gap: 0.75rem; overflow-x: auto; padding: 0.25rem 0 0.5rem;
    scrollbar-width: thin;
  }
  .row-strip .card { min-width: 160px; flex: 0 0 160px; }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
    gap: 0.75rem;
  }

  .card {
    position: relative;
    border-radius: 14px;
    border: 1px solid var(--border-1, rgba(255,255,255,0.08));
    background: var(--surface-2, rgba(255,255,255,0.03));
    overflow: hidden;
    transition: transform 140ms ease, border-color 140ms ease, box-shadow 140ms ease;
  }
  .card:hover {
    transform: translateY(-1px);
    border-color: var(--border-2, rgba(255,255,255,0.18));
    box-shadow: 0 8px 22px -14px rgba(0,0,0,0.5);
  }
  .card.pinned { border-color: var(--accent, #5e6ad2); }

  .card-body {
    width: 100%;
    display: flex; flex-direction: column; gap: 0.3rem;
    padding: 0.6rem;
    background: transparent;
    border: 0;
    color: inherit;
    cursor: pointer;
    text-align: left;
  }
  .card-thumb {
    position: relative;
    aspect-ratio: 3 / 4;
    border-radius: 8px;
    overflow: hidden;
    background: var(--surface-3, rgba(255,255,255,0.04));
    display: flex; align-items: center; justify-content: center;
  }
  .card-thumb img { width: 100%; height: 100%; object-fit: cover; }
  .card-thumb-placeholder {
    font-family: ui-monospace, SFMono-Regular, monospace;
    opacity: 0.45;
    letter-spacing: 1.5px;
    font-size: 14px;
  }
  .pin-flag {
    position: absolute; top: 6px; right: 6px;
    color: var(--accent, #5e6ad2);
    filter: drop-shadow(0 1px 2px rgba(0,0,0,0.4));
    display: flex;
  }
  .pin-flag .ico { width: 14px; height: 14px; }
  .card-name {
    font-size: 0.85rem;
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .card-meta { font-size: 0.72rem; opacity: 0.55; }

  .dots { display: flex; gap: 3px; margin-top: 0.2rem; }
  .dot {
    width: 6px; height: 6px; border-radius: 999px;
    background: var(--surface-3, rgba(255,255,255,0.12));
    transition: background 300ms ease;
  }
  .dot.on { background: var(--accent, #5e6ad2); }

  .card-actions {
    position: absolute;
    top: 6px; left: 6px;
    display: flex; gap: 4px;
    opacity: 0;
    transition: opacity 140ms ease;
  }
  .card:hover .card-actions { opacity: 1; }
  .act {
    width: 22px; height: 22px;
    border-radius: 6px;
    border: 1px solid var(--border-1, rgba(255,255,255,0.12));
    background: var(--surface-1, rgba(0,0,0,0.4));
    backdrop-filter: blur(8px);
    color: inherit;
    font-size: 11px;
    cursor: pointer;
    display: flex; align-items: center; justify-content: center;
  }
  .act .ico { width: 13px; height: 13px; }
  .act:hover { border-color: var(--accent, #5e6ad2); }
  .act.danger:hover { border-color: #ef4444; color: #ef4444; }
</style>
