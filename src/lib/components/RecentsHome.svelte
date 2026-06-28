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
    clearRecent,
    reorderPinned,
    type RecentFile,
  } from "$lib/recent";
  import {
    partitionRecents,
    filterRecents,
    highlightRecentName,
    sortRecentView,
    recentSortLabel,
    RECENT_SORT_MODES,
    flattenRecentCards,
    classifyRecentKey,
    recentCardScrollOptions,
    moveRecentCursor,
    clampRecentCursor,
    summarizeRecents,
    countInProgress,
    countUnpinned,
    describeClearUnpinned,
    recentProgressBar,
    pinnedStripEdges,
    orderPinnedStrip,
    movePinned,
    type RecentSortMode,
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
    window.addEventListener("keydown", handleKey);
    // Re-measure the pinned strip's overflow on viewport resize (a window
    // narrowing can turn a fitting strip into an overflowing one).
    window.addEventListener("resize", measureStrip);
    queueMicrotask(measureStrip);
  });
  onDestroy(() => {
    unsub?.();
    window.removeEventListener("keydown", handleKey);
    window.removeEventListener("resize", measureStrip);
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

  // Slice 3 (round 44): sort modes. Recents only ever sorted newest-first.
  // The segmented control cycles Recent / Name / Progress / Pages via the
  // tested sortRecentView, applied to BOTH rendered grids (pinned + others)
  // so the whole board re-orders together.
  let sortMode = $state<RecentSortMode>("recent");

  // The "Continue reading" hero card surfaces the single most useful next
  // action: the file with the freshest reading momentum. The selection +
  // partition math now lives in the tested pure core (recentsHomeView.ts),
  // so the contract that decides what the app's headline card even is has
  // unit tests instead of an untested inline $derived. `partitionRecents`
  // returns { hero, pinned, others } exactly mirroring the render regions.
  //
  // While a filter is active the hero collapses and the board becomes a
  // flat matched list (pinned strip + recents grid both filtered); with no
  // filter the partition's hero-aware split is used unchanged. The rendered
  // rows are then run through sortRecentView so the active sort mode wins.
  const partition = $derived(partitionRecents(recents));
  const matched = $derived(filtering ? filterRecents(recents, q) : recents);
  const continueCandidate = $derived(filtering ? null : partition.hero);
  // The pinned strip honours the user's manual drag order (orderPinnedStrip
  // reads each card's pinOrder) ONLY in the default Recent view with no
  // filter — an explicit sort (Name/Progress/Pages) or an active filter
  // takes precedence, exactly like the others grid. So drag-order is the
  // resting arrangement, and choosing a sort still works.
  const pinnedBase = $derived(
    filtering ? matched.filter((r) => r.pinned) : partition.pinned,
  );
  const pinned = $derived(
    !filtering && sortMode === "recent"
      ? orderPinnedStrip(pinnedBase)
      : sortRecentView(pinnedBase, sortMode),
  );
  const others = $derived(
    sortRecentView(filtering ? matched.filter((r) => !r.pinned) : partition.others, sortMode),
  );
  /** True only when the strip is in its manually-orderable resting state
   *  (default Recent sort, no filter) — gates the drag handles + Alt+Arrow
   *  reorder so a reorder during a sort/filter can't write a misleading
   *  pinOrder. */
  const stripReorderable = $derived(!filtering && sortMode === "recent" && pinned.length > 1);

  // Pinned-strip overflow affordance (this round): the strip scrolls
  // horizontally but gave no hint when it overflowed. Track its live scroll
  // geometry; pinnedStripEdges turns it into {overflowing, atStart, atEnd}
  // so we can paint edge fades + enable the scroll chevrons. Recomputed on
  // scroll, on resize, and whenever the pinned list changes.
  let stripEl = $state<HTMLDivElement | null>(null);
  let stripEdges = $state(pinnedStripEdges(null));

  function measureStrip(): void {
    if (!stripEl) {
      stripEdges = pinnedStripEdges(null);
      return;
    }
    stripEdges = pinnedStripEdges({
      scrollLeft: stripEl.scrollLeft,
      scrollWidth: stripEl.scrollWidth,
      clientWidth: stripEl.clientWidth,
    });
  }

  /** Scroll the pinned strip by ~80% of a viewport in the given direction
      (the chevron buttons + a fallback for pointer-only users). */
  function scrollStrip(dir: -1 | 1): void {
    if (!stripEl) return;
    stripEl.scrollBy({ left: dir * stripEl.clientWidth * 0.8, behavior: "smooth" });
  }

  // Slice 8 (this round): drag-to-reorder the pinned strip. The strip
  // rendered in store order with no way to arrange it. A card carries a
  // drag handle; HTML5 drag computes the new order and reorderPinned
  // persists each card's pinOrder. Keyboard users get Alt+Left/Right on a
  // focused card (no pointer needed). All gated on stripReorderable so a
  // reorder during a sort/filter can't stamp a misleading order.
  /** Index (within the rendered `pinned` strip) of the card being dragged,
   *  or -1 when no drag is in flight. Drives the drag-source dimming. */
  let dragIdx = $state(-1);
  /** Index the dragged card is currently hovering over (the drop target),
   *  or -1. Drives the drop-indicator ring. */
  let dragOverIdx = $state(-1);

  /** Commit a reorder: move the card at `from` to `to` and persist. The
      pure movePinned computes the new path order from the CURRENT strip
      order; reorderPinned stamps pinOrder (store sort untouched). */
  function commitReorder(from: number, to: number): void {
    if (!stripReorderable) return;
    const order = movePinned(pinned, from, to);
    if (order.length > 0) reorderPinned(order);
  }

  function onDragStart(e: DragEvent, i: number): void {
    if (!stripReorderable) return;
    dragIdx = i;
    if (e.dataTransfer) {
      e.dataTransfer.effectAllowed = "move";
      // Firefox requires data to be set for a drag to start.
      try { e.dataTransfer.setData("text/plain", String(i)); } catch { /* ignore */ }
    }
  }

  function onDragOver(e: DragEvent, i: number): void {
    if (dragIdx < 0) return;
    e.preventDefault(); // allow the drop
    if (e.dataTransfer) e.dataTransfer.dropEffect = "move";
    dragOverIdx = i;
  }

  function onDrop(e: DragEvent, i: number): void {
    if (dragIdx < 0) return;
    e.preventDefault();
    if (dragIdx !== i) commitReorder(dragIdx, i);
    dragIdx = -1;
    dragOverIdx = -1;
  }

  function onDragEnd(): void {
    dragIdx = -1;
    dragOverIdx = -1;
  }

  /** Keyboard reorder: Alt+Left/Right nudges a focused pinned card one slot.
      Returns true if it handled the key (so the caller can stop). */
  function nudgePinned(i: number, dir: -1 | 1): boolean {
    if (!stripReorderable) return false;
    const to = i + dir;
    if (to < 0 || to >= pinned.length) return false;
    commitReorder(i, to);
    // Keep the keyboard cursor on the moved card so repeated nudges chain.
    cursor = to;
    queueMicrotask(() => cardEls[to]?.focus?.());
    return true;
  }

  // Re-measure when the pinned list changes (a pin/unpin can flip overflow).
  $effect(() => {
    void pinned.length;
    void stripEl;
    queueMicrotask(measureStrip);
  });

  // Slice 4 (round 44): keyboard navigation. The board was mouse-only. A
  // window keydown handler drives a VIRTUAL cursor (ring only, no DOM focus
  // move) over the rendered cards — flattenRecentCards gives one index space
  // across the pinned strip + recents grid (the hero keeps its own ⌘0). The
  // cursor never moves real focus, so Enter on a focused card-button can't
  // double-fire. Reuses the tested palette nav core via moveRecentCursor /
  // clampRecentCursor, and classifyRecentKey owns the bare-key -> action map.
  const flatRows = $derived(flattenRecentCards(pinned, others));
  let cursor = $state(-1);
  let cardEls = $state<Array<HTMLElement | null>>([]);
  let listFocused = $state(false);

  // Slice 5 (round 44): context-aware summary footer. The board gave no
  // running sense of what you were looking at. summarizeRecents narrates the
  // live view (shown-vs-total, the filter term, the in-progress count, the
  // active sort) into an aria-live region, mirroring the command-palette /
  // library-search / beacon-cache footers. countInProgress reuses the
  // palette progress core so the threshold matches the hero chip exactly.
  const summary = $derived(
    summarizeRecents({
      total: recents.length,
      shown: filtering ? matched.length : recents.length,
      query: q,
      inProgress: countInProgress(filtering ? matched : recents),
      sort: sortMode,
    }),
  );

  // Slice 6 (this round): clear-unpinned affordance. The board only let you
  // remove rows one at a time; the store already has a "clear unpinned"
  // primitive (clearRecent preserves pinned). countUnpinned drives an honest
  // count so the footer can offer "Clear N unpinned" and hide it when every
  // row is pinned (nothing to clear). Confirm before wiping.
  const unpinnedCount = $derived(countUnpinned(recents));
  const clearUnpinnedLabel = $derived(describeClearUnpinned(unpinnedCount));
  function clearUnpinned() {
    if (unpinnedCount <= 0) return;
    if (
      !window.confirm(
        `Clear ${unpinnedCount} unpinned ${unpinnedCount === 1 ? "document" : "documents"} from recents? Pinned documents are kept.`,
      )
    )
      return;
    clearRecent();
    notify.info(
      `Cleared ${unpinnedCount} unpinned ${unpinnedCount === 1 ? "document" : "documents"}`,
    );
  }

  // Keep the cursor in range when the list shrinks (a filter narrowed it, a
  // file was pinned/removed). A cleared filter / emptied board parks it.
  $effect(() => {
    cursor = clampRecentCursor(cursor, flatRows.length);
    if (flatRows.length === 0) listFocused = false;
  });

  function scrollCursorIntoView() {
    queueMicrotask(() => {
      const el = cardEls[cursor];
      if (!el) return;
      // A pinned card lives in a horizontally-overflowing strip, so scroll
      // it along the inline (horizontal) axis; a grid card scrolls
      // vertically. The view-core decides the alignment per section.
      const row = cursor >= 0 ? flatRows[cursor] : null;
      const opts = recentCardScrollOptions(row?.section ?? "others");
      el.scrollIntoView({ block: opts.block, inline: opts.inline });
    });
  }

  /**
   * Window-level key handler. Returns true when it consumed the event.
   * While the filter input is focused only nav + Enter are honored (so the
   * arrows reach the cards but typed letters like "p" stay literal text);
   * elsewhere the full bare-key map (Enter / P / Backspace / Esc) applies.
   */
  function handleKey(e: KeyboardEvent): boolean {
    if (recents.length === 0) return false;
    const target = e.target as HTMLElement | null;
    const inFilter = target === filterEl;
    const tag = target?.tagName;
    // Outside the filter, ignore keys aimed at other inputs/controls.
    if (!inFilter && (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT")) {
      return false;
    }
    // Slice 8: Alt+Left/Right reorders a focused PINNED card one slot — a
    // keyboard twin of drag-to-reorder. Checked before classifyRecentKey
    // (which disqualifies modifier chords). Only when the cursor sits on a
    // pinned card and the strip is in its reorderable resting state.
    if (
      e.altKey && !e.metaKey && !e.ctrlKey && !inFilter &&
      (e.key === "ArrowLeft" || e.key === "ArrowRight") &&
      stripReorderable && cursor >= 0 && cursor < pinned.length
    ) {
      if (nudgePinned(cursor, e.key === "ArrowRight" ? 1 : -1)) {
        e.preventDefault();
        return true;
      }
    }
    const action = classifyRecentKey(e);
    if (!action) return false;
    // From inside the filter box, only let movement + open through — pin /
    // remove / clear would hijack the user's typing.
    if (inFilter && action.kind !== "move" && action.kind !== "open") return false;
    // ...and inside the filter box, leave the HORIZONTAL arrows to the
    // text caret (Left/Right move within the typed query); only the
    // vertical arrows cross into the card grid. Outside the filter, both
    // axes walk the cursor (so the pinned strip is reachable with Right).
    if (inFilter && action.kind === "move" && (e.key === "ArrowLeft" || e.key === "ArrowRight")) {
      return false;
    }

    const row = cursor >= 0 ? flatRows[cursor] : null;
    switch (action.kind) {
      case "move": {
        if (flatRows.length === 0) return false;
        e.preventDefault();
        listFocused = true;
        cursor = moveRecentCursor(action.intent, cursor, flatRows.length);
        scrollCursorIntoView();
        return true;
      }
      case "open": {
        if (!row) return false;
        e.preventDefault();
        onOpen(row.file);
        return true;
      }
      case "pin": {
        if (!row) return false;
        e.preventDefault();
        pinRecent(row.file.path);
        notify.success(row.file.pinned ? `Unpinned ${row.file.name}` : `Pinned ${row.file.name}`);
        return true;
      }
      case "remove": {
        if (!row) return false;
        e.preventDefault();
        removeRecent(row.file.path);
        notify.info(`Removed ${row.file.name} from recents`);
        return true;
      }
      case "clear": {
        // Esc parks the cursor if the list has focus; otherwise it falls
        // through (the filter's own Esc handler clears the query first).
        if (listFocused && cursor >= 0) {
          e.preventDefault();
          cursor = -1;
          listFocused = false;
          return true;
        }
        return false;
      }
    }
    return false;
  }

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
<!-- Slice 7 — thumbnail reading-progress overlay. A thin accent bar along
     the bottom edge of a card/hero thumbnail shows how far through the doc
     you are, derived from the same tested recentReadingProgress core the
     dots + palette chip use (so they can never disagree). Finished docs
     read as a full bar with a distinct done tint. Hidden when there's no
     usable position. -->
{#snippet progressOverlay(r: RecentFile)}
  {@const bar = recentProgressBar(r)}
  {#if bar.show}
    <div
      class="thumb-progress"
      class:done={bar.finished}
      role="progressbar"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={bar.percent}
      aria-label={bar.label}
      title={bar.label}
    >
      <span class="thumb-progress-fill" style="width: {bar.percent}%"></span>
    </div>
  {/if}
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
      <div class="sort-seg" role="group" aria-label="Sort recent documents">
        {#each RECENT_SORT_MODES as mode (mode)}
          <button
            class="sort-btn"
            class:active={sortMode === mode}
            onclick={() => (sortMode = mode)}
            aria-pressed={sortMode === mode}
            title={`Sort by ${recentSortLabel(mode)}`}
          >{recentSortLabel(mode)}</button>
        {/each}
      </div>
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
      <div
        class="strip-wrap"
        class:has-start={stripEdges.atStart}
        class:has-end={stripEdges.atEnd}
      >
        {#if stripEdges.overflowing}
          <button
            type="button"
            class="strip-nav prev"
            aria-label="Scroll pinned documents left"
            disabled={!stripEdges.atStart}
            onclick={() => scrollStrip(-1)}
          >
            <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
              <path d="M10 3L5 8l5 5" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" fill="none" />
            </svg>
          </button>
          <button
            type="button"
            class="strip-nav next"
            aria-label="Scroll pinned documents right"
            disabled={!stripEdges.atEnd}
            onclick={() => scrollStrip(1)}
          >
            <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
              <path d="M6 3l5 5-5 5" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" fill="none" />
            </svg>
          </button>
        {/if}
        <div class="row-strip" bind:this={stripEl} onscroll={measureStrip} role="list" aria-label="Pinned documents">
        {#each pinned as r, i (r.path)}
          {@const thumb = getRecentThumb(r.path)}
          <div
            class="card pinned"
            class:cursor={listFocused && cursor === i}
            class:dragging={dragIdx === i}
            class:drop-target={dragOverIdx === i && dragIdx >= 0 && dragIdx !== i}
            class:reorderable={stripReorderable}
            bind:this={cardEls[i]}
            role="listitem"
            aria-label={stripReorderable ? `${r.name} — draggable, Alt+Left or Alt+Right to reorder` : r.name}
            draggable={stripReorderable}
            ondragstart={(e) => onDragStart(e, i)}
            ondragover={(e) => onDragOver(e, i)}
            ondrop={(e) => onDrop(e, i)}
            ondragend={onDragEnd}
          >
            {#if stripReorderable}
              <span
                class="drag-grip"
                aria-hidden="true"
                title="Drag to reorder (or Alt+←/→)"
              >
                <svg viewBox="0 0 10 16" width="8" height="13">
                  <circle cx="2.5" cy="3" r="1.2" fill="currentColor" />
                  <circle cx="7.5" cy="3" r="1.2" fill="currentColor" />
                  <circle cx="2.5" cy="8" r="1.2" fill="currentColor" />
                  <circle cx="7.5" cy="8" r="1.2" fill="currentColor" />
                  <circle cx="2.5" cy="13" r="1.2" fill="currentColor" />
                  <circle cx="7.5" cy="13" r="1.2" fill="currentColor" />
                </svg>
              </span>
            {/if}
            <button class="card-body" onclick={() => onOpen(r)} title={r.path}>
              <div class="card-thumb">
                {#if thumb}
                  <img src={thumb} alt="" loading="lazy" />
                {:else}
                  <span class="card-thumb-placeholder">PDF</span>
                {/if}
                <span class="pin-flag" aria-hidden="true">{@render pinGlyph()}</span>
                {@render progressOverlay(r)}
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
        {#each others as r, i (r.path)}
          {@const thumb = getRecentThumb(r.path)}
          {@const flatIdx = pinned.length + i}
          <div
            class="card"
            class:cursor={listFocused && cursor === flatIdx}
            bind:this={cardEls[flatIdx]}
          >
            <button class="card-body" onclick={() => onOpen(r)} title={r.path}>
              <div class="card-thumb">
                {#if thumb}
                  <img src={thumb} alt="" loading="lazy" />
                {:else}
                  <span class="card-thumb-placeholder">{basename(r.name).slice(0, 3).toUpperCase()}</span>
                {/if}
                {@render progressOverlay(r)}
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

  {#if recents.length > 0}
    <footer class="board-foot">
      <span class="board-foot-summary" aria-live="polite">{summary}</span>
      {#if clearUnpinnedLabel}
        <button
          class="board-foot-clear"
          onclick={clearUnpinned}
          title="Remove every unpinned document from this list (pinned are kept)"
        >{clearUnpinnedLabel}</button>
      {/if}
      <span class="board-foot-hint" aria-hidden="true">
        <kbd>↑</kbd><kbd>↓</kbd> move · <kbd>↵</kbd> open · <kbd>P</kbd> pin · <kbd>⌫</kbd> remove
      </span>
    </footer>
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

  /* Slice 3 — segmented sort control. */
  .sort-seg {
    display: flex;
    gap: 2px;
    padding: 2px;
    border-radius: 8px;
    background: var(--surface-3, rgba(255,255,255,0.04));
    flex: 0 0 auto;
  }
  .sort-btn {
    appearance: none;
    border: 0;
    background: transparent;
    color: inherit;
    opacity: 0.6;
    font-size: 0.74rem;
    font-weight: 600;
    cursor: pointer;
    padding: 0.22rem 0.5rem;
    border-radius: 6px;
    transition: background 120ms ease, opacity 120ms ease;
    white-space: nowrap;
  }
  .sort-btn:hover { opacity: 0.9; background: color-mix(in srgb, white 6%, transparent); }
  .sort-btn.active {
    opacity: 1;
    background: color-mix(in srgb, var(--accent, #5e6ad2) 26%, transparent);
  }

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

  /* Pinned-strip overflow affordance: a positioned wrapper paints edge
     fade masks (only when there's hidden content that way) and hosts the
     scroll chevrons. The masks use pseudo-elements gated by the
     has-start / has-end classes the component toggles from pinnedStripEdges. */
  .strip-wrap { position: relative; }
  .strip-wrap::before,
  .strip-wrap::after {
    content: "";
    position: absolute;
    top: 0.25rem;
    bottom: 0.5rem;
    width: 2.5rem;
    pointer-events: none;
    opacity: 0;
    transition: opacity 140ms ease;
    z-index: 1;
  }
  .strip-wrap::before {
    left: 0;
    background: linear-gradient(to right, var(--bg, #0d0d10), transparent);
  }
  .strip-wrap::after {
    right: 0;
    background: linear-gradient(to left, var(--bg, #0d0d10), transparent);
  }
  .strip-wrap.has-start::before { opacity: 1; }
  .strip-wrap.has-end::after { opacity: 1; }

  .strip-nav {
    position: absolute;
    top: 50%;
    transform: translateY(-50%);
    z-index: 2;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    border-radius: 50%;
    border: 1px solid var(--border, rgba(255, 255, 255, 0.12));
    background: color-mix(in srgb, var(--bg-panel, #1a1a1f) 88%, transparent);
    backdrop-filter: blur(6px);
    color: var(--fg, #fff);
    cursor: pointer;
    opacity: 0;
    transition: opacity 140ms ease, background 120ms ease, border-color 120ms ease;
  }
  .strip-nav.prev { left: -6px; }
  .strip-nav.next { right: -6px; }
  .strip-wrap.has-start .strip-nav.prev,
  .strip-wrap.has-end .strip-nav.next { opacity: 1; }
  .strip-nav:hover:not(:disabled) {
    background: var(--bg-panel, #1a1a1f);
    border-color: var(--accent, #7c8cff);
  }
  .strip-nav:disabled { opacity: 0; pointer-events: none; }

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
  /* Slice 4 — virtual keyboard cursor ring. */
  .card.cursor {
    border-color: var(--accent, #5e6ad2);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent, #5e6ad2) 55%, transparent),
      0 8px 22px -14px rgba(0,0,0,0.5);
  }
  .card.cursor .card-actions { opacity: 1; }

  /* Slice 8 — drag-to-reorder the pinned strip. The card carries a grip
     affordance (top-left, fades in on hover/cursor); the drag source dims
     and the drop target shows an accent ring so the landing slot is clear. */
  .card.reorderable { position: relative; }
  .drag-grip {
    position: absolute;
    top: 6px;
    left: 6px;
    z-index: 2;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 2px;
    border-radius: 4px;
    color: var(--text-3, #8b8b94);
    background: color-mix(in srgb, var(--bg-1, #16161a) 70%, transparent);
    cursor: grab;
    opacity: 0;
    transition: opacity 100ms ease, color 100ms ease;
  }
  .card.reorderable:hover .drag-grip,
  .card.reorderable.cursor .drag-grip {
    opacity: 1;
  }
  .drag-grip:hover { color: var(--accent, #5e6ad2); }
  .card.dragging {
    opacity: 0.45;
    cursor: grabbing;
  }
  .card.drop-target {
    border-color: var(--accent, #5e6ad2);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent, #5e6ad2) 70%, transparent);
  }

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
  /* Slice 7 — thumbnail progress overlay. A thin accent bar pinned to the
     bottom edge of the thumbnail; a translucent track behind the fill so
     even a tiny fill reads as "started". Finished docs get a calm green. */
  .thumb-progress {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    height: 3px;
    background: rgba(0, 0, 0, 0.35);
  }
  .thumb-progress-fill {
    display: block;
    height: 100%;
    background: var(--accent, #5e6ad2);
    transition: width 200ms ease;
  }
  .thumb-progress.done .thumb-progress-fill {
    background: var(--success, #3fb950);
  }
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

  /* Slice 5 — context-aware summary footer. */
  .board-foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    padding: 0.5rem 0.25rem 0;
    border-top: 1px solid var(--border-1, rgba(255,255,255,0.06));
    font-size: 0.76rem;
    flex-wrap: wrap;
  }
  .board-foot-summary { opacity: 0.6; margin-right: auto; }
  /* Slice 6 — clear-unpinned affordance. Quiet by default; the danger
     tint only surfaces on hover so it never competes with the summary. */
  .board-foot-clear {
    appearance: none;
    border: 1px solid var(--border-1, rgba(255, 255, 255, 0.1));
    background: transparent;
    color: inherit;
    opacity: 0.55;
    font-size: 0.74rem;
    padding: 0.18rem 0.55rem;
    border-radius: 6px;
    cursor: pointer;
    white-space: nowrap;
    transition: opacity 120ms, border-color 120ms, color 120ms;
  }
  .board-foot-clear:hover {
    opacity: 1;
    color: #ef4444;
    border-color: #ef4444;
  }
  .board-foot-hint {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    opacity: 0.4;
    white-space: nowrap;
  }
  .board-foot-hint kbd {
    font-family: ui-monospace, SFMono-Regular, monospace;
    font-size: 0.92em;
    padding: 0.04em 0.32em;
    border-radius: 4px;
    background: var(--surface-3, rgba(255,255,255,0.06));
    border: 1px solid var(--border-1, rgba(255,255,255,0.08));
  }
</style>
