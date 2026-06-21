<script lang="ts">
  // LibraryPanel — v0.12.0 "Atlas" Slice 3.
  //
  // Two-pane Library experience that finally makes the Slice 1 backend
  // user-visible. Left rail: folder filter + tag filter. Main pane:
  // toolbar (Add Folder, Rescan All, search, sort) + responsive cards
  // grid. Click a card → opens that PDF in a new Reader tab (via a
  // `slab:open-library-doc` window event that `+page.svelte` listens
  // for). Right-click / "⋯" → context menu (Open, Add Tag, Remove).
  //
  // The whole panel is dumb-as-possible: state lives here, every action
  // goes through the typed `$lib/library` client, no global stores.
  //
  // What's intentionally NOT here yet:
  //   - Slice 2 will add `notify`-crate filesystem watching so this
  //     panel auto-refreshes when files change in registered folders.
  //   - Slice 4 will add a "Ask Beacon" button per-card and a top-of-
  //     panel "Ask across library" entry point.
  //   - Slice 5 will add saved searches (pin a LibraryFilter).
  //   - Virtualization (Slice 7-ish): a 5000-card grid will be slow.
  //     We accept that until we have a real workload to optimize against.

  import { onMount, onDestroy } from "svelte";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { isInTauri } from "$lib/tauri";
  import {
    addFolder,
    addTag,
    autoTagRunMany,
    autoTagRunOne,
    bulkApplyTag,
    bulkRemoveTag,
    listDocuments,
    listFolders,
    listTags,
    ocrQueueRunAll,
    ocrQueueRunOne,
    removeDocument,
    removeFolder,
    removeTag,
    rescanAll,
    scanFolder,
    setDocumentTags,
    setDocumentTitle,
    setDocumentNotes,
    setDocumentStarred,
    setTagColor,
    renameTag,
    mergeTags,
    setTagDescription,
    recentlyUsedTags,
    tagUsageCounts,
    deleteUnusedTags,
    savedViewSave,
    savedViewList,
    savedViewDelete,
    savedViewRename,
    savedViewUpdateFilter,
    savedViewDuplicate,
    savedViewSetPinned,
    savedViewReorder,
    collectionList,
    collectionAddDocs,
    type AutoTagRunResult,
    type CollectionRecord,
    type DocumentRecord,
    type FilterClause,
    type FilterCombinator,
    type FolderRecord,
    type LibraryFilter,
    type LibrarySortBy,
    type OcrQueueResult,
    type OcrState,
    type TagRecord,
    type TagMatch,
    type SavedViewRecord,
  } from "$lib/library";
  import { basename } from "$lib/types";
  import { formatRelTime } from "$lib/recent";
  import { registerLibraryNav } from "$lib/vim/library-adapter";
  import CollectionsSidebar from "$lib/panels/CollectionsSidebar.svelte";
  import SuggestedTagsRow from "$lib/panels/SuggestedTagsRow.svelte";
  import DocInspectorPanel from "$lib/panels/DocInspectorPanel.svelte";
  import BulkTagSuggestionsPanel from "$lib/panels/BulkTagSuggestionsPanel.svelte";
  import { tagSuggestionStats, type TagSuggestionStats } from "$lib/library";

  // ---------- Props (Cabinet v1.1.0) ----------
  //
  // When the panel is rendered inside a detached WebviewWindow, the parent
  // route passes `detached={true}`. In that mode, double-clicking a doc
  // can't just dispatch a window event — there's no Reader tabstrip in
  // this window. We forward the path to the *main* window via the
  // `slab_request_open_in_main` command and let it open the tab.
  type Props = { detached?: boolean };
  let { detached = false }: Props = $props();

  // ---------- State ----------

  let folders = $state<FolderRecord[]>([]);
  let tags = $state<TagRecord[]>([]);
  // v3.46.0 Atlas Tag-Usage-Counts — how many documents wear each tag, keyed
  // by tag id (a tag attached to nothing maps to 0). Loaded alongside the tag
  // list and refreshed on every library-changed poke, so the muted rail count
  // stays truthful through tag/merge/bulk edits. `tagSort` flips the rail
  // between alphabetical (the default) and most-used-first; the count is what
  // makes "most used" a meaningful order.
  let tagCounts = $state<Map<number, number>>(new Map());
  let tagSort = $state<"name" | "count">("name");
  // v3.47.0 Atlas Tag-Cleanup — how many tags are attached to zero documents.
  // Derived straight from the usage-count map the rail already loads, so it
  // self-heals on every refresh; drives the rail-head "Clean up N" affordance.
  // `cleaningTags` guards the one-click prune from double-fire while the
  // backend round-trips.
  let unusedTagCount = $derived(
    tags.reduce((n, t) => n + ((tagCounts.get(t.id) ?? 0) === 0 ? 1 : 0), 0),
  );
  let cleaningTags = $state(false);
  // The tag rail's render order. Alphabetical mirrors the backend's
  // `list_tags` ORDER BY name; "count" sorts by usage desc, falling back to
  // name for ties so the order is stable and never jitters between equal
  // counts. Either way it's a cheap copy-then-sort over the in-memory list.
  let sortedTags = $derived.by(() => {
    const list = [...tags];
    if (tagSort === "count") {
      list.sort((a, b) => {
        const d = (tagCounts.get(b.id) ?? 0) - (tagCounts.get(a.id) ?? 0);
        return d !== 0 ? d : a.name.localeCompare(b.name);
      });
    }
    return list;
  });
  let docs = $state<DocumentRecord[]>([]);
  let activeFolder = $state<number | "all">("all");
  // v3.32.0 Atlas — when set, overrides the folder/tag filter and shows
  // the resolved collection docs instead. Null means "use folder+tag filter".
  let activeCollection = $state<{ kind: "collection" | "smart"; id: number; name: string; docs: DocumentRecord[] } | null>(null);

  function onCollectionSelect(payload: {
    kind: "collection" | "smart";
    id: number;
    name: string;
    docs: DocumentRecord[];
  }) {
    activeCollection = payload;
    // Clear folder/tag filters so the user sees the collection cleanly.
    activeFolder = "all";
    activeTagIds = new Set();
    docs = payload.docs;
  }

  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  function clearCollection() {
    activeCollection = null;
    refreshDocs();
  }
  let activeTagIds = $state<Set<number>>(new Set());
  // v3.48.0 Atlas Tag-Combinator — how multiple selected tags combine in the
  // rail filter. "all" (default) intersects (docs with EVERY selected tag),
  // "any" unions (docs with AT LEAST ONE). Only meaningful with >1 tag
  // selected; the toggle is shown in the Tags rail head in that case.
  let tagMatch = $state<TagMatch>("all");
  let query = $state("");
  let sort = $state<LibrarySortBy>("added_desc");
  // v3.40.0 Atlas Untagged-Filter — when on, restrict the grid to docs
  // with no tags at all (no `library_doc_tags` rows). Cleared when a
  // collection is selected (collections fully own their doc list).
  let untaggedOnly = $state(false);
  // v3.55.0 Atlas Doc-Inspector — when on, restrict the grid to docs the
  // user has starred. AND-combined with every other filter; persists for
  // the session but resets when a collection or smart collection is loaded
  // (those load their own filter or doc list).
  let starredOnly = $state(false);
  // v3.49.0 Atlas Tag-Filter-Clear — true when any tag-rail filter is engaged
  // (>=1 tag selected, or the untagged-only toggle is on). Drives the rail-head
  // "Clear" affordance, which resets the whole tag filter in one click. The
  // match mode is excluded from this test on purpose: it only affects results
  // with >1 tag selected, so a lingering non-default mode with 0 tags is inert
  // and shouldn't surface a Clear button on its own — clearTagFilter resets it
  // anyway for a clean slate.
  let tagFilterActive = $derived(activeTagIds.size > 0 || untaggedOnly);
  // v3.50.0 Atlas Saved Views — named LibraryFilter snapshots the user can
  // pin and one-click restore. Distinct from collections (which own a doc
  // list) and personal presets (which materialize INTO a smart collection):
  // a view simply RE-RUNS the saved filter live. Loaded alongside folders
  // + tags + counts in refreshAll so a save / delete / rename round-trip
  // self-heals via the existing library-changed reactive path.
  let savedViews = $state<SavedViewRecord[]>([]);
  // True while a save / delete round-trip is in flight, so the rail-head
  // affordances disable themselves to prevent double-fire.
  let savedViewBusy = $state(false);
  // Tracks the id of the most recently restored view so the rail can show
  // it as `active` — purely cosmetic, cleared whenever the user manually
  // edits any filter dimension (since that diverges from the saved snapshot).
  let activeSavedViewId = $state<number | null>(null);
  // v3.56.0 Atlas Saved-Views-Polish — per-row overflow menu state.
  // Holds the id of the view whose ⋯ menu is currently open, or null.
  // Declared up here (not next to the handler block below) so the
  // window-click-outside handler can reference it before it'd otherwise
  // be in scope at function-declaration time.
  let savedViewMenuId = $state<number | null>(null);
  // Inline-rename draft state for the per-row rename verb. Scoped to a
  // single view id; null when no rename is in progress.
  let savedViewRenameId = $state<number | null>(null);
  let savedViewRenameDraft = $state("");
  let savedViewRenameError = $state<string | null>(null);
  // The "Save current filter" inline form. `null` when closed.
  let saveViewDraftName = $state<string | null>(null);
  let saveViewError = $state<string | null>(null);
  // The rail's "Save current filter" button is meaningful only when SOME
  // filter dimension is non-default — saving an empty filter would just be
  // "show everything", which is the default view and not worth a button.
  let filterIsNonDefault = $derived(
    activeFolder !== "all" ||
      activeTagIds.size > 0 ||
      untaggedOnly ||
      query.trim().length > 0 ||
      sort !== "added_desc",
  );
  let loading = $state(false);
  let scanning = $state(false);
  let error = $state<string | null>(null);
  let initialized = $state(false);

  // OCR queue state.
  let ocringAll = $state(false);
  let ocringDocIds = $state<Set<number>>(new Set());
  let ocrSummary = $state<string | null>(null);

  // Auto-tag state (Lens Slice 6).
  let autoTaggingAll = $state(false);
  let autoTaggingDocIds = $state<Set<number>>(new Set());
  let autoTagSummary = $state<string | null>(null);

  // ---------- Bulk tag-apply (v3.41.0 Atlas) ----------
  //
  // Multi-select mode: a checkbox appears on each card; checking docs
  // builds `selectedDocIds`. A floating action bar then applies or removes
  // a tag across the whole selection in one backend transaction. Selection
  // is keyed by doc id so it survives a grid refresh (stale ids are simply
  // ignored by the backend and pruned on the next refreshDocs()).
  let selecting = $state(false);
  let selectedDocIds = $state<Set<number>>(new Set());
  let bulkBusy = $state(false);
  let bulkSummary = $state<string | null>(null);
  // The bulk "apply tag" picker: a small popover listing existing tags
  // plus a free-text "new tag" entry. Null when closed.
  let bulkTagPickerOpen = $state(false);
  let bulkNewTagName = $state("");

  // Bulk "add to collection" picker (v3.53.0 Atlas Collections — Slice 27).
  // Lists every manual collection by name; click adds the N selected docs
  // to that collection and toasts the result count. Pure frontend slice
  // wrapping collectionAddDocs + collectionList — no new backend needed.
  let bulkCollectionPickerOpen = $state(false);
  let bulkCollections = $state<CollectionRecord[]>([]);
  let bulkCollectionsLoading = $state(false);

  let selectedCount = $derived(selectedDocIds.size);
  let allVisibleSelected = $derived(
    docs.length > 0 && docs.every((d) => selectedDocIds.has(d.id)),
  );

  // Context menu state. `null` when closed.
  type Menu = {
    doc: DocumentRecord;
    x: number;
    y: number;
    submenu: "tag" | null;
  };
  let menu = $state<Menu | null>(null);

  // v3.44.0 Atlas Recent-Tags — the most recently applied tags, surfaced as
  // quick-chips at the top of a doc's tag menu so common tags are one click
  // away. Loaded lazily when a tag menu opens, then refreshed after any tag
  // change so the order stays current. Chips already on the doc are hidden.
  let recentTags = $state<TagRecord[]>([]);

  // Recently-used chips minus the tags already on the menu's doc (those show
  // checked in the list below, so re-offering them as "apply" chips is noise).
  let recentChips = $derived(
    menu
      ? recentTags.filter(
          (rt) => !menu!.doc.tags.some((dt) => dt.id === rt.id),
        )
      : [],
  );

  async function loadRecentTags() {
    try {
      recentTags = await recentlyUsedTags(8);
    } catch {
      // Non-fatal: the menu still shows the full alphabetical tag list.
      recentTags = [];
    }
  }

  // New-tag modal state.
  let newTagOpen = $state(false);
  let newTagName = $state("");
  let newTagColor = $state<string | null>(null);
  let pendingDocForTag = $state<DocumentRecord | null>(null);

  // Edit-tag-color modal state (v3.42.0 Atlas Tag-Color editing). Holds the
  // tag whose color is being edited plus the in-flight selection; null = closed.
  let editColorTag = $state<TagRecord | null>(null);
  let editColorValue = $state<string | null>(null);
  let editColorBusy = $state(false);

  // Inline tag-rename state (v3.43.0 Atlas Tag-Rename). When `renameTagId` is
  // set, that rail row renders a text input seeded with `renameDraft` instead
  // of its label. `renameBusy` guards the in-flight commit; `renameError`
  // surfaces a rejected rename (e.g. a name collision) inline on the row.
  let renameTagId = $state<number | null>(null);
  let renameDraft = $state("");
  let renameBusy = $state(false);
  let renameError = $state<string | null>(null);

  // Tag-merge state (v3.45.0 Atlas Tag-Merge). `mergeSourceTag` is the tag the
  // user chose to fold away; the modal then picks a `mergeTargetId` to fold it
  // into. `mergeBusy` guards the in-flight fold; `mergeError` surfaces a
  // rejected merge inline.
  let mergeSourceTag = $state<TagRecord | null>(null);
  let mergeTargetId = $state<number | null>(null);
  let mergeBusy = $state(false);
  let mergeError = $state<string | null>(null);
  // The tags the source can be merged into — everything except the source.
  let mergeCandidates = $derived(
    mergeSourceTag
      ? tags.filter((t) => t.id !== mergeSourceTag!.id)
      : [],
  );

  // Edit-tag-description modal state (v3.51.0 Atlas Tag-Descriptions). Holds
  // the tag whose description is being edited plus the in-flight draft;
  // null = closed. The draft is the *textarea contents* (the empty string is
  // a real, user-typed value) — the backend semantically maps empty/whitespace
  // to clearing the column, so we don't need a separate "null" sentinel here.
  // `editDescError` surfaces a rejected save (e.g. oversized text) inline.
  const MAX_TAG_DESCRIPTION_LEN = 500; // mirror of backend constant.
  let editDescTag = $state<TagRecord | null>(null);
  let editDescDraft = $state("");
  let editDescBusy = $state(false);
  let editDescError = $state<string | null>(null);

  // Debounced search.
  let searchDebounceTimer: ReturnType<typeof setTimeout> | null = null;

  // ---------- Glass II Vim adapter (v1.2.0 Slice 3) ----------
  //
  // We track a single "focused" doc index for j/k navigation. The
  // currently focused card gets a visual ring; Enter / l opens it,
  // dd removes it.
  let vimFocusIdx = $state(-1);
  let cardEls: HTMLElement[] = [];

  function clampFocus(idx: number): number {
    if (docs.length === 0) return -1;
    if (idx < 0) return 0;
    if (idx >= docs.length) return docs.length - 1;
    return idx;
  }

  function scrollFocusIntoView() {
    if (vimFocusIdx < 0) return;
    const el = cardEls[vimFocusIdx];
    if (el && typeof el.scrollIntoView === "function") {
      el.scrollIntoView({ behavior: "smooth", block: "nearest" });
    }
  }

  function onVimLibMove(e: Event) {
    const d = (e as CustomEvent<{ direction: "up" | "down" | "left" | "right"; count?: number }>).detail;
    const step = Math.max(1, d.count ?? 1);
    if (vimFocusIdx < 0) vimFocusIdx = 0;
    // h/l = left/right within row (treat as -1 / +1). j/k same as a single step.
    const delta =
      d.direction === "down" || d.direction === "right" ? step : -step;
    vimFocusIdx = clampFocus(vimFocusIdx + delta);
    scrollFocusIntoView();
  }

  function onVimLibFirst() {
    vimFocusIdx = clampFocus(0);
    scrollFocusIntoView();
  }

  function onVimLibLast() {
    vimFocusIdx = clampFocus(docs.length - 1);
    scrollFocusIntoView();
  }

  function onVimLibOpen() {
    if (vimFocusIdx < 0 || vimFocusIdx >= docs.length) return;
    openDocInTab(docs[vimFocusIdx]);
  }

  function onVimLibRemove() {
    if (vimFocusIdx < 0 || vimFocusIdx >= docs.length) return;
    const doc = docs[vimFocusIdx];
    void onMenuRemoveDoc(doc);
    // Step backward one so the next dd targets a sane card.
    vimFocusIdx = clampFocus(vimFocusIdx - 1);
  }

  const TAG_PALETTE = [
    "#ff7a59", // accent
    "#6ab7ff",
    "#7ee787",
    "#f5c518",
    "#c084fc",
    "#f47272",
    "#79c0ff",
    "#a4a4b0",
  ];

  // ---------- Derived counts ----------

  let totalDocs = $derived(docs.length);
  let docsForActiveFolder = $derived(
    activeFolder === "all"
      ? docs.length
      : docs.filter((d) => d.folder_id === activeFolder).length,
  );
  /** Number of documents currently visible that are eligible for OCR. */
  let pendingOcrCount = $derived(
    docs.filter((d) => d.ocr_state === "scanned" || d.ocr_state === "mixed").length,
  );

  // ---------- Lifecycle ----------

  // Cabinet v1.1.0: stay coherent with sibling windows. Backend emits
  // `slab://library-changed` (no payload) after every mutation in any
  // window — folder add/remove, scan, tag changes, OCR, auto-tag, doc
  // delete. We just refetch; the event is a poke, not a patch.
  let unlistenLibraryChanged: UnlistenFn | null = null;
  let unregisterVim: (() => void) | null = null;

  onMount(async () => {
    await refreshAll();
    initialized = true;
    void refreshBulkBadge();
    window.addEventListener("click", onWindowClickForMenu);
    // Glass II Vim adapter — subscribe to the panel-targeted events.
    window.addEventListener("slab:vim-library:move", onVimLibMove as EventListener);
    window.addEventListener("slab:vim-library:first", onVimLibFirst as EventListener);
    window.addEventListener("slab:vim-library:last", onVimLibLast as EventListener);
    window.addEventListener("slab:vim-library:open", onVimLibOpen as EventListener);
    window.addEventListener("slab:vim-library:remove", onVimLibRemove as EventListener);
    unregisterVim = registerLibraryNav();
    if (isInTauri()) {
      try {
        unlistenLibraryChanged = await listen("slab://library-changed", () => {
          // Fire-and-forget. If a refetch fails mid-flight (e.g. sqlite
          // file briefly locked) the next event will resync.
          void refreshAll();
          void refreshBulkBadge();
        });
      } catch (e) {
        console.error("[library] failed to subscribe to library-changed:", e);
      }
    }
  });

  onDestroy(() => {
    window.removeEventListener("click", onWindowClickForMenu);
    window.removeEventListener("slab:vim-library:move", onVimLibMove as EventListener);
    window.removeEventListener("slab:vim-library:first", onVimLibFirst as EventListener);
    window.removeEventListener("slab:vim-library:last", onVimLibLast as EventListener);
    window.removeEventListener("slab:vim-library:open", onVimLibOpen as EventListener);
    window.removeEventListener("slab:vim-library:remove", onVimLibRemove as EventListener);
    if (unregisterVim) {
      unregisterVim();
      unregisterVim = null;
    }
    if (unlistenLibraryChanged) {
      unlistenLibraryChanged();
      unlistenLibraryChanged = null;
    }
  });

  function onWindowClickForMenu(_e: MouseEvent) {
    // Close the menu whenever the user clicks elsewhere. Right-click
    // on a different card replaces it; this just handles the
    // "click outside" case.
    menu = null;
    // Same dismiss-on-outside semantics for the saved-views per-row menu
    // (v3.56.0 Atlas Saved-Views-Polish). The button itself stops
    // propagation, so its own click doesn't fire this listener.
    savedViewMenuId = null;
  }

  // ---------- Data loaders ----------

  async function refreshAll() {
    loading = true;
    error = null;
    try {
      const [f, t, c, v] = await Promise.all([
        listFolders(),
        listTags(),
        tagUsageCounts(),
        savedViewList(),
      ]);
      folders = f;
      tags = t;
      tagCounts = c;
      savedViews = v;
      await refreshDocs();
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function refreshDocs() {
    // v3.32.0 Atlas — collection override short-circuits the filter.
    if (activeCollection) {
      docs = activeCollection.docs;
      return;
    }
    const folderId = activeFolder === "all" ? null : activeFolder;
    const title = query.trim() ? query.trim() : null;
    let filter: LibraryFilter;
    if (untaggedOnly) {
      // v3.40.0: the flat folder/tag/title fields and the clause tree are
      // mutually exclusive on the backend, so when the untagged toggle is
      // on we express the whole filter as an AND clause group.
      const clauses: FilterClause[] = [{ type: "untagged" }];
      if (folderId != null) clauses.push({ type: "folder", id: folderId });
      for (const id of activeTagIds) clauses.push({ type: "tag", id });
      if (title) clauses.push({ type: "title_contains", value: title });
      const combinator: FilterCombinator = "and";
      filter = {
        sort,
        clauses: { combinator, clauses },
        starred_only: starredOnly,
      };
    } else {
      filter = {
        folder_id: folderId,
        tag_ids: Array.from(activeTagIds),
        // v3.48.0: "all" intersects, "any" unions. Sent always; the backend
        // treats a missing field as "all" but being explicit keeps stored
        // filters self-describing.
        tag_match: tagMatch,
        title_substring: title,
        sort,
        // v3.55.0 Atlas Doc-Inspector: AND-combined with every other
        // constraint; backend treats missing as false so omitting it is
        // safe but being explicit keeps the wire shape self-describing.
        starred_only: starredOnly,
      };
    }
    try {
      docs = await listDocuments(filter);
      // v3.41.0: prune any selected ids that fell out of the current view
      // so the bulk action bar's count reflects what's actually on screen.
      if (selectedDocIds.size > 0) {
        const visible = new Set(docs.map((d) => d.id));
        const pruned = new Set(
          [...selectedDocIds].filter((id) => visible.has(id)),
        );
        if (pruned.size !== selectedDocIds.size) selectedDocIds = pruned;
      }
    } catch (e) {
      error = String(e);
    }
  }

  // Reactive search — debounce 150ms so we don't hammer sqlite on
  // every keystroke.
  function onQueryInput(e: Event) {
    query = (e.target as HTMLInputElement).value;
    if (searchDebounceTimer) clearTimeout(searchDebounceTimer);
    searchDebounceTimer = setTimeout(() => {
      void refreshDocs();
    }, 150);
  }

  $effect(() => {
    // Re-run docs whenever folder / tags / sort changes. Search is
    // handled by its own debounce.
    if (!initialized) return;
    // Touching reactive deps deliberately:
    activeFolder;
    activeTagIds;
    tagMatch;
    sort;
    untaggedOnly;
    starredOnly;
    void refreshDocs();
  });

  // ---------- Toolbar actions ----------

  async function onAddFolder() {
    try {
      const picked = await openDialog({ directory: true, multiple: false });
      if (typeof picked !== "string") return;
      const folder = await addFolder(picked);
      // Reload folder list immediately so the rail shows the new entry,
      // then kick off the scan in the background.
      folders = await listFolders();
      scanning = true;
      error = null;
      try {
        await scanFolder(folder.id);
      } catch (e) {
        error = `Scan failed for ${picked}: ${String(e)}`;
      } finally {
        scanning = false;
        await refreshDocs();
      }
    } catch (e) {
      error = String(e);
    }
  }

  async function onRescanAll() {
    if (folders.length === 0) {
      error = "Add a folder before rescanning.";
      return;
    }
    scanning = true;
    error = null;
    try {
      const reports = await rescanAll();
      const totals = reports.reduce(
        (acc, r) => {
          acc.added += r.added ?? 0;
          acc.updated += r.updated ?? 0;
          acc.unchanged += r.unchanged ?? 0;
          return acc;
        },
        { added: 0, updated: 0, unchanged: 0 },
      );
      lastScanSummary = `Scanned ${folders.length} folder${folders.length === 1 ? "" : "s"}: ${totals.added} new, ${totals.updated} updated, ${totals.unchanged} unchanged`;
    } catch (e) {
      error = String(e);
    } finally {
      scanning = false;
      await refreshAll();
    }
  }
  let lastScanSummary = $state<string | null>(null);

  // ---------- OCR queue actions ----------

  /** Human-readable label for the OCR state badge. */
  function ocrStateLabel(state: OcrState): string {
    switch (state) {
      case "scanned":
        return "Scanned";
      case "mixed":
        return "Mixed";
      case "ocr_pending":
        return "OCR'ing…";
      case "ocr_done":
        return "OCR'd";
      case "ocr_failed":
        return "OCR failed";
      case "text_native":
        return "";
      case "unknown":
      default:
        return "";
    }
  }

  /** Whether the badge should show at all for this state. */
  function showOcrBadge(state: OcrState): boolean {
    return state !== "text_native" && state !== "unknown";
  }

  function isOcrCandidate(state: OcrState): boolean {
    return state === "scanned" || state === "mixed" || state === "ocr_failed";
  }

  /** Local optimistic update so the UI shows new ocr_state/output instantly. */
  function applyResult(r: OcrQueueResult): void {
    docs = docs.map((d) =>
      d.id === r.doc_id
        ? {
            ...d,
            ocr_state: r.state_after,
            ocr_output_path: r.output_path,
            // v3.52.0: a successful run clears the persisted error; a
            // failure replaces it with the fresh reason. Mirror what
            // run_one wrote to the DB so the local doc list and the
            // backend stay in lockstep without a refetch.
            ocr_error: r.error,
          }
        : d,
    );
    if (menu && menu.doc.id === r.doc_id) {
      const fresh = docs.find((d) => d.id === r.doc_id);
      if (fresh) menu = { ...menu, doc: fresh };
    }
  }

  async function onRunOcrFor(doc: DocumentRecord) {
    menu = null;
    if (ocringDocIds.has(doc.id)) return;
    const next = new Set(ocringDocIds);
    next.add(doc.id);
    ocringDocIds = next;
    error = null;
    try {
      const result = await ocrQueueRunOne(doc.id, null);
      applyResult(result);
      if (result.error) {
        error = `OCR failed for ${displayTitle(doc)}: ${result.error}`;
      } else {
        ocrSummary = `OCR'd "${displayTitle(doc)}"`;
      }
    } catch (e) {
      error = String(e);
    } finally {
      const after = new Set(ocringDocIds);
      after.delete(doc.id);
      ocringDocIds = after;
    }
  }

  async function onRunOcrAll() {
    if (pendingOcrCount === 0) return;
    ocringAll = true;
    error = null;
    ocrSummary = null;
    try {
      const results = await ocrQueueRunAll(null);
      for (const r of results) applyResult(r);
      const ok = results.filter((r) => r.state_after === "ocr_done").length;
      const failed = results.filter((r) => r.state_after === "ocr_failed").length;
      ocrSummary = `OCR queue: ${ok} succeeded, ${failed} failed (of ${results.length})`;
      if (failed > 0 && ok === 0) {
        error = `All ${failed} OCR attempts failed. Is Tesseract installed?`;
      }
    } catch (e) {
      error = String(e);
    } finally {
      ocringAll = false;
    }
  }

  function openOcrOutput(doc: DocumentRecord) {
    if (!doc.ocr_output_path) return;
    menu = null;
    requestOpen(doc.ocr_output_path);
  }

  /** Cabinet v1.1.0 router for "open this doc in Reader".
   *
   * Main window → dispatch a local CustomEvent that `+page.svelte` listens
   * for and turns into a new Reader tab.
   *
   * Detached window → forward to the main window via the Tauri
   * `slab_request_open_in_main` command (which emits `slab://open-doc`
   * targeted at the `main` window). This keeps the user's main shell as
   * the canonical multi-tab Reader. */
  function requestOpen(path: string): void {
    if (detached && isInTauri()) {
      void invoke("slab_request_open_in_main", { path }).catch((e) => {
        console.error("[library] slab_request_open_in_main failed:", e);
      });
      return;
    }
    window.dispatchEvent(
      new CustomEvent("slab:open-library-doc", { detail: { path } }),
    );
  }

  // ---------- Auto-tag (Lens Slice 6) ----------

  /** Apply an auto-tag result optimistically — patch the doc's tag list
   * locally so the chips show up before the next listDocuments() refresh. */
  function applyAutoTagResult(r: AutoTagRunResult): void {
    if (r.error) return;
    // Build TagRecord[] from name+id pairs. If a tag was newly created
    // by the backend we don't have its color yet — null is fine, the
    // next refresh will fill it in (and the chip falls back to a grey
    // border).
    const knownById = new Map(tags.map((t) => [t.id, t]));
    const newTags = r.tag_ids.map((id, i) => {
      const existing = knownById.get(id);
      if (existing) return existing;
      return {
        id,
        name: r.tags_assigned[i] ?? `tag-${id}`,
        color: null,
      } as TagRecord;
    });
    docs = docs.map((d) =>
      d.id === r.doc_id ? { ...d, tags: newTags } : d,
    );
    if (menu && menu.doc.id === r.doc_id) {
      const fresh = docs.find((d) => d.id === r.doc_id);
      if (fresh) menu = { ...menu, doc: fresh };
    }
  }

  /** v3.39.0 Atlas Tag-Suggest — optimistically attach an accepted
   * suggested tag to a doc's chip row (unioned, no dupes), then refresh
   * the tag rail so a newly-created tag shows up there too. */
  async function onTagSuggestionAccepted(docId: number, tag: TagRecord): Promise<void> {
    docs = docs.map((d) => {
      if (d.id !== docId) return d;
      if (d.tags.some((t) => t.id === tag.id)) return d;
      return { ...d, tags: [...d.tags, tag] };
    });
    if (menu && menu.doc.id === docId) {
      const fresh = docs.find((d) => d.id === docId);
      if (fresh) menu = { ...menu, doc: fresh };
    }
    try {
      tags = await listTags();
    } catch {
      // Non-fatal: the rail will catch up on the next full refresh.
    }
  }

  async function onAutoTagFor(doc: DocumentRecord) {
    menu = null;
    if (autoTaggingDocIds.has(doc.id)) return;
    const next = new Set(autoTaggingDocIds);
    next.add(doc.id);
    autoTaggingDocIds = next;
    error = null;
    autoTagSummary = null;
    try {
      const result = await autoTagRunOne(doc.id, null);
      applyAutoTagResult(result);
      if (result.error) {
        error = `Auto-tag failed for ${displayTitle(doc)}: ${result.error}`;
      } else {
        const added = result.tags_assigned.length;
        autoTagSummary = `Auto-tagged "${displayTitle(doc)}" (${added} tag${added === 1 ? "" : "s"})`;
        // Refresh the tag rail since new tags may have been created.
        try {
          tags = await listTags();
        } catch {
          // Non-fatal — the next full refresh picks it up.
        }
      }
    } catch (e) {
      error = String(e);
    } finally {
      const after = new Set(autoTaggingDocIds);
      after.delete(doc.id);
      autoTaggingDocIds = after;
    }
  }

  async function onAutoTagAll() {
    if (docs.length === 0) return;
    if (autoTaggingAll) return;
    const ok = window.confirm(
      `Auto-tag ${docs.length} document${docs.length === 1 ? "" : "s"}?\n\nThis sends each doc's text to your configured Beacon provider. Suggestions are added — your existing tags are never removed.`,
    );
    if (!ok) return;
    autoTaggingAll = true;
    error = null;
    autoTagSummary = null;
    try {
      const docIds = docs.map((d) => d.id);
      const results = await autoTagRunMany(docIds, null);
      for (const r of results) applyAutoTagResult(r);
      const succeeded = results.filter((r) => !r.error).length;
      const failed = results.filter((r) => r.error).length;
      autoTagSummary = `Auto-tag: ${succeeded} tagged, ${failed} failed (of ${results.length})`;
      if (failed > 0 && succeeded === 0) {
        error = `All ${failed} auto-tag attempts failed. Is your Beacon provider configured and reachable?`;
      }
      // Refresh tag rail — new tags may have been created.
      try {
        tags = await listTags();
      } catch {
        // Non-fatal.
      }
    } catch (e) {
      error = String(e);
    } finally {
      autoTaggingAll = false;
    }
  }

  // ---------- Bulk tag-apply actions (v3.41.0 Atlas) ----------

  function toggleSelectMode() {
    selecting = !selecting;
    if (!selecting) {
      selectedDocIds = new Set();
      bulkTagPickerOpen = false;
    }
  }

  function toggleDocSelected(docId: number) {
    const next = new Set(selectedDocIds);
    if (next.has(docId)) next.delete(docId);
    else next.add(docId);
    selectedDocIds = next;
  }

  function selectAllVisible() {
    if (allVisibleSelected) {
      selectedDocIds = new Set();
    } else {
      selectedDocIds = new Set(docs.map((d) => d.id));
    }
  }

  function clearSelection() {
    selectedDocIds = new Set();
    bulkTagPickerOpen = false;
    bulkCollectionPickerOpen = false;
  }

  // -------- Bulk "Add to collection" (v3.53.0 Slice 27) --------
  // Lazy-load the collection list on first picker open (rare path; the
  // backend list is single-query cheap but we still avoid the round-trip
  // for users who never crack open the bulk bar).
  async function openBulkCollectionPicker() {
    // If the user closed the tag picker by opening this one, mirror the
    // pattern from bulkTagPickerOpen and let the two be mutually exclusive.
    bulkTagPickerOpen = false;
    bulkCollectionPickerOpen = !bulkCollectionPickerOpen;
    if (bulkCollectionPickerOpen) {
      // Refresh on every open — single cheap query, and it catches new
      // collections created since the last open without bespoke
      // library-changed plumbing.
      bulkCollectionsLoading = true;
      try {
        bulkCollections = await collectionList();
      } catch (e) {
        error = (e as Error).message;
        bulkCollectionPickerOpen = false;
      } finally {
        bulkCollectionsLoading = false;
      }
    }
  }

  async function onBulkAddToCollection(target: CollectionRecord) {
    if (selectedDocIds.size === 0 || bulkBusy) return;
    bulkBusy = true;
    try {
      const ids = Array.from(selectedDocIds);
      const added = await collectionAddDocs(target.id, ids);
      const dupes = ids.length - added;
      bulkSummary =
        dupes > 0
          ? `Added ${added} doc${added === 1 ? "" : "s"} to “${target.name}” (${dupes} already in)`
          : `Added ${added} doc${added === 1 ? "" : "s"} to “${target.name}”`;
      // Keep the picker open so the user can chain into a second
      // collection without re-opening; the CollectionsSidebar self-heals
      // its count badge via the library-changed emit on collectionAddDocs.
    } catch (e) {
      error = (e as Error).message;
    } finally {
      bulkBusy = false;
    }
  }

  /** Apply `tagName` (existing or freshly typed) to every selected doc. */
  async function onBulkApplyTag(tagName: string) {
    const name = tagName.trim();
    if (!name || selectedDocIds.size === 0 || bulkBusy) return;
    bulkBusy = true;
    error = null;
    bulkSummary = null;
    try {
      const ids = Array.from(selectedDocIds);
      const res = await bulkApplyTag(name, ids);
      const skipped = res.total - res.affected;
      bulkSummary =
        `Applied "${res.tag.name}" to ${res.affected} doc${res.affected === 1 ? "" : "s"}` +
        (skipped > 0 ? ` (${skipped} already had it)` : "");
      bulkTagPickerOpen = false;
      bulkNewTagName = "";
      // New tag may have been created — refresh the rail; the grid
      // refresh comes via the library-changed event the command emits.
      try {
        tags = await listTags();
      } catch {
        // Non-fatal — next full refresh fills it in.
      }
      await refreshDocs();
    } catch (e) {
      error = String(e);
    } finally {
      bulkBusy = false;
    }
  }

  /** Remove a tag (by id) from every selected doc. Driven from the
   * picker's "remove" affordance next to each existing tag. */
  async function onBulkRemoveTag(tag: TagRecord) {
    if (selectedDocIds.size === 0 || bulkBusy) return;
    bulkBusy = true;
    error = null;
    bulkSummary = null;
    try {
      const ids = Array.from(selectedDocIds);
      const res = await bulkRemoveTag(tag.id, ids);
      bulkSummary = `Removed "${res.tag.name}" from ${res.affected} doc${res.affected === 1 ? "" : "s"}`;
      bulkTagPickerOpen = false;
      await refreshDocs();
    } catch (e) {
      error = String(e);
    } finally {
      bulkBusy = false;
    }
  }

  // ---------- Folder rail actions ----------

  function selectFolder(id: number | "all") {
    activeFolder = id;
    activeCollection = null;
  }
  async function onRemoveFolder(folder: FolderRecord) {
    const ok = window.confirm(
      `Stop tracking ${folder.path}?\n\nThe folder stays on disk — Slab just forgets about the PDFs inside it.`,
    );
    if (!ok) return;
    try {
      await removeFolder(folder.id);
      if (activeFolder === folder.id) activeFolder = "all";
      await refreshAll();
    } catch (e) {
      error = String(e);
    }
  }

  // ---------- Tag rail actions ----------

  function toggleTag(tag: TagRecord) {
    const next = new Set(activeTagIds);
    if (next.has(tag.id)) next.delete(tag.id);
    else next.add(tag.id);
    activeTagIds = next;
  }

  // v3.49.0 Atlas Tag-Filter-Clear — reset the entire tag-rail filter in one
  // click: drop every selected tag, clear the untagged-only toggle, and return
  // the match mode to its "all" default. Each assignment is a fresh value so
  // the reactive $effect that watches activeTagIds / untaggedOnly / tagMatch
  // re-queries once; no manual refresh call needed. No-op-safe (the button is
  // only shown when tagFilterActive), but resetting unconditionally is cheap
  // and keeps the slate fully clean.
  function clearTagFilter() {
    activeTagIds = new Set();
    untaggedOnly = false;
    tagMatch = "all";
  }

  // ---------- Saved views (v3.50.0 Atlas Saved Views) ----------
  //
  // The rail's "Save current filter" affordance snapshots the entire
  // filter (folder + tags + match mode + untagged toggle + sort) under
  // a user-given name. One click on a saved view later restores all
  // those dimensions in a single batch so the existing reactive $effect
  // re-queries exactly once.
  //
  // We deliberately DO NOT round-trip through the backend's stored
  // filter on restore — the SavedViewRecord we already have in memory
  // carries the decoded LibraryFilter, and the rail state is derived
  // from individual reactive primitives ($state) not a single Filter
  // object. So restoration is a small fan-out: unpack the saved fields
  // into the matching $state cells.

  function buildCurrentFilter(): LibraryFilter {
    // Mirror refreshDocs()'s filter construction so what we save is what
    // gets queried on restore. The untaggedOnly branch produces a clause
    // tree (the only way the backend lets us combine untagged with other
    // filters); the simple branch uses the flat folder/tag/title shape.
    const folderId = activeFolder === "all" ? null : activeFolder;
    const title = query.trim() ? query.trim() : null;
    if (untaggedOnly) {
      const clauses: FilterClause[] = [{ type: "untagged" }];
      if (folderId != null) clauses.push({ type: "folder", id: folderId });
      for (const id of activeTagIds) clauses.push({ type: "tag", id });
      if (title) clauses.push({ type: "title_contains", value: title });
      const combinator: FilterCombinator = "and";
      return { sort, clauses: { combinator, clauses } };
    }
    return {
      folder_id: folderId,
      tag_ids: Array.from(activeTagIds),
      tag_match: tagMatch,
      title_substring: title,
      sort,
    };
  }

  function openSaveViewForm() {
    saveViewError = null;
    // Seed with a plausible name: the active folder or tag name when
    // there's an obvious anchor, otherwise empty so the user types.
    let seed = "";
    if (activeFolder !== "all") {
      const f = folders.find((x) => x.id === activeFolder);
      if (f) seed = folderShortName(f.path);
    } else if (activeTagIds.size === 1) {
      const onlyId = [...activeTagIds][0];
      const t = tags.find((x) => x.id === onlyId);
      if (t) seed = t.name;
    } else if (untaggedOnly) {
      seed = "Untagged";
    }
    saveViewDraftName = seed;
  }

  function cancelSaveView() {
    saveViewDraftName = null;
    saveViewError = null;
  }

  async function commitSaveView() {
    if (saveViewDraftName === null || savedViewBusy) return;
    const name = saveViewDraftName.trim();
    if (name.length === 0) {
      saveViewError = "name required";
      return;
    }
    savedViewBusy = true;
    saveViewError = null;
    try {
      const saved = await savedViewSave({
        name,
        filter: buildCurrentFilter(),
      });
      savedViews = [...savedViews, saved];
      activeSavedViewId = saved.id;
      saveViewDraftName = null;
    } catch (e) {
      // UNIQUE name collisions surface here. Keep the form open with the
      // backend reason inline so the user can retype + retry.
      saveViewError = String(e).replace(/^Error:\s*/, "");
    } finally {
      savedViewBusy = false;
    }
  }

  function restoreSavedView(view: SavedViewRecord) {
    // Unpack the saved LibraryFilter into the individual rail $state
    // cells. We treat the clause-tree shape (used when untaggedOnly was
    // on at save time) as a thin envelope: pull `untagged` + `folder` +
    // every `tag` + `title_contains` clause back out, ignoring anything
    // exotic (a user couldn't have built it from this UI, so it's
    // forward-compat noise we don't need to reproduce in the rail).
    activeCollection = null;
    if (view.filter.clauses) {
      let untagged = false;
      let folderId: number | "all" = "all";
      const tagIds = new Set<number>();
      let title = "";
      for (const c of view.filter.clauses.clauses) {
        if (c.type === "untagged") untagged = true;
        else if (c.type === "folder") folderId = c.id;
        else if (c.type === "tag") tagIds.add(c.id);
        else if (c.type === "title_contains") title = c.value;
      }
      untaggedOnly = untagged;
      activeFolder = folderId;
      activeTagIds = tagIds;
      tagMatch = "all"; // clause tree always uses AND combinator
      query = title;
    } else {
      untaggedOnly = false;
      activeFolder = view.filter.folder_id ?? "all";
      activeTagIds = new Set(view.filter.tag_ids ?? []);
      tagMatch = view.filter.tag_match ?? "all";
      query = view.filter.title_substring ?? "";
    }
    sort = view.filter.sort ?? "added_desc";
    activeSavedViewId = view.id;
    // The reactive $effect on (activeFolder/activeTagIds/tagMatch/sort/
    // untaggedOnly) re-queries automatically; we don't call refreshDocs().
  }

  async function onDeleteSavedView(view: SavedViewRecord) {
    if (savedViewBusy) return;
    const ok = window.confirm(
      `Delete saved view "${view.name}"? The underlying docs are not affected.`,
    );
    if (!ok) return;
    savedViewBusy = true;
    try {
      await savedViewDelete(view.id);
      savedViews = savedViews.filter((v) => v.id !== view.id);
      if (activeSavedViewId === view.id) activeSavedViewId = null;
    } catch (e) {
      error = String(e);
    } finally {
      savedViewBusy = false;
    }
  }

  // ---------- Saved views v3.56.0 Atlas Saved-Views-Polish ----------
  //
  // Four new verbs added on top of the v3.50 CRUD:
  //   - update filter in place (re-pin the rail onto an existing view)
  //   - duplicate (fork the filter as "<name> (copy)" — Notion convention)
  //   - pin / unpin (anchor to top of rail, surfaces with a ★ glyph)
  //   - reorder (drag-handle target — full-list atomic re-stamp)
  //
  // The rail row gains a left-side pin glyph (gold when on, ghost otherwise)
  // and a right-side ⋯ menu surfacing Duplicate / Rename / Edit-here /
  // Delete. The body of the row stays a one-click restore (unchanged).

  /** Re-pin the active view onto the CURRENT filter shape. Visible only
   *  when there's a non-default current filter and a view is active. */
  async function onUpdateActiveSavedView() {
    if (activeSavedViewId === null || savedViewBusy) return;
    const view = savedViews.find((v) => v.id === activeSavedViewId);
    if (!view) return;
    const ok = window.confirm(
      `Update "${view.name}" with the current filter? The pinned shape will be replaced.`,
    );
    if (!ok) return;
    savedViewBusy = true;
    try {
      const updated = await savedViewUpdateFilter(view.id, buildCurrentFilter());
      savedViews = savedViews.map((v) => (v.id === updated.id ? updated : v));
      // activeSavedViewId stays — the row matches the current rail again.
    } catch (e) {
      error = String(e);
    } finally {
      savedViewBusy = false;
    }
  }

  async function onDuplicateSavedView(view: SavedViewRecord) {
    if (savedViewBusy) return;
    savedViewBusy = true;
    try {
      const dup = await savedViewDuplicate(view.id);
      // Insert by sort_order so the list rebuilds in pinned-first order.
      savedViews = [...savedViews, dup].sort(savedViewCompare);
    } catch (e) {
      error = String(e);
    } finally {
      savedViewBusy = false;
    }
  }

  async function onTogglePinSavedView(view: SavedViewRecord) {
    if (savedViewBusy) return;
    savedViewBusy = true;
    try {
      const updated = await savedViewSetPinned(view.id, !view.pinned);
      savedViews = savedViews
        .map((v) => (v.id === updated.id ? updated : v))
        .sort(savedViewCompare);
    } catch (e) {
      error = String(e);
    } finally {
      savedViewBusy = false;
    }
  }

  /** Bubble a view up/down by one slot in its current group (pinned vs
   *  unpinned). Reorder is restricted to within-group because pinned-first
   *  is the dominant sort key — letting an unpinned view "swap" with a
   *  pinned one above it would just be confusing. */
  async function onBumpSavedView(view: SavedViewRecord, dir: -1 | 1) {
    if (savedViewBusy) return;
    // Build the sorted slice of the same pin-group, find the view's slot,
    // swap with the neighbour in `dir` direction, send the WHOLE list as
    // the new order so the backend re-stamps every sort_order in one txn.
    const group = savedViews.filter((v) => !!v.pinned === !!view.pinned);
    const idx = group.findIndex((v) => v.id === view.id);
    const swap = idx + dir;
    if (idx < 0 || swap < 0 || swap >= group.length) return;
    [group[idx], group[swap]] = [group[swap], group[idx]];
    // Re-merge the two groups in the canonical pinned-first order and send.
    const ordered = [
      ...savedViews.filter((v) => v.pinned).map((v) => v.id),
      ...savedViews.filter((v) => !v.pinned).map((v) => v.id),
    ];
    // Replace the matching group's slice in `ordered` with the swapped
    // slice we just built. The two groups don't overlap so it's safe.
    const groupIds = group.map((v) => v.id);
    if (view.pinned) {
      ordered.splice(0, groupIds.length, ...groupIds);
    } else {
      const pinnedCount = savedViews.filter((v) => v.pinned).length;
      ordered.splice(pinnedCount, groupIds.length, ...groupIds);
    }
    savedViewBusy = true;
    try {
      await savedViewReorder(ordered);
      // Refresh from the backend so sort_order on the local copies matches
      // the new persisted order; cheaper than computing the new values
      // locally and easier to keep correct.
      savedViews = await savedViewList();
    } catch (e) {
      error = String(e);
    } finally {
      savedViewBusy = false;
    }
  }

  /** Local sort to keep the rail list in canonical pinned-first order
   *  after in-memory mutations, without an extra round-trip. Matches the
   *  backend `list_views` ORDER BY. */
  function savedViewCompare(a: SavedViewRecord, b: SavedViewRecord): number {
    const ap = a.pinned ? 1 : 0;
    const bp = b.pinned ? 1 : 0;
    if (ap !== bp) return bp - ap;
    if (a.sort_order !== b.sort_order) return a.sort_order - b.sort_order;
    return a.name.localeCompare(b.name);
  }

  function openSavedViewRename(view: SavedViewRecord) {
    savedViewMenuId = null;
    savedViewRenameId = view.id;
    savedViewRenameDraft = view.name;
    savedViewRenameError = null;
  }

  function cancelSavedViewRename() {
    savedViewRenameId = null;
    savedViewRenameDraft = "";
    savedViewRenameError = null;
  }

  async function commitSavedViewRename() {
    if (savedViewRenameId === null || savedViewBusy) return;
    const id = savedViewRenameId;
    const newName = savedViewRenameDraft.trim();
    if (newName.length === 0) {
      savedViewRenameError = "name required";
      return;
    }
    const existing = savedViews.find((v) => v.id === id);
    if (!existing) {
      cancelSavedViewRename();
      return;
    }
    if (existing.name === newName) {
      cancelSavedViewRename();
      return;
    }
    savedViewBusy = true;
    savedViewRenameError = null;
    try {
      const updated = await savedViewRename(id, newName);
      savedViews = savedViews
        .map((v) => (v.id === updated.id ? updated : v))
        .sort(savedViewCompare);
      cancelSavedViewRename();
    } catch (e) {
      savedViewRenameError = String(e).replace(/^Error:\s*/, "");
    } finally {
      savedViewBusy = false;
    }
  }

  // Clear the "active saved view" highlight as soon as the user diverges
  // from the saved snapshot — the rail row should only glow while the
  // current filter actually matches what's pinned. Cheap structural check.
  $effect(() => {
    if (activeSavedViewId === null) return;
    const v = savedViews.find((x) => x.id === activeSavedViewId);
    if (!v) {
      activeSavedViewId = null;
      return;
    }
    const f = v.filter;
    const cur = {
      folder: activeFolder === "all" ? null : activeFolder,
      tags: Array.from(activeTagIds).sort((a, b) => a - b),
      match: tagMatch,
      untagged: untaggedOnly,
      sort,
      query: query.trim(),
    };
    // Decode saved filter into the same comparable shape — branch on
    // whether it stored a clause tree or the flat fields.
    let saved: typeof cur;
    if (f.clauses) {
      let untagged = false;
      let folder: number | null = null;
      const tagIds: number[] = [];
      let title = "";
      for (const c of f.clauses.clauses) {
        if (c.type === "untagged") untagged = true;
        else if (c.type === "folder") folder = c.id;
        else if (c.type === "tag") tagIds.push(c.id);
        else if (c.type === "title_contains") title = c.value;
      }
      saved = {
        folder,
        tags: tagIds.sort((a, b) => a - b),
        match: "all",
        untagged,
        sort: f.sort ?? "added_desc",
        query: title,
      };
    } else {
      saved = {
        folder: f.folder_id ?? null,
        tags: [...(f.tag_ids ?? [])].sort((a, b) => a - b),
        match: f.tag_match ?? "all",
        untagged: false,
        sort: f.sort ?? "added_desc",
        query: f.title_substring ?? "",
      };
    }
    const same =
      cur.folder === saved.folder &&
      cur.match === saved.match &&
      cur.untagged === saved.untagged &&
      cur.sort === saved.sort &&
      cur.query === saved.query &&
      cur.tags.length === saved.tags.length &&
      cur.tags.every((id, i) => id === saved.tags[i]);
    if (!same) activeSavedViewId = null;
  });

  async function onCreateTopLevelTag() {
    pendingDocForTag = null;
    newTagName = "";
    newTagColor = TAG_PALETTE[0];
    newTagOpen = true;
  }

  async function onRemoveTag(tag: TagRecord) {
    const ok = window.confirm(
      `Delete tag "${tag.name}"? It will be removed from every document.`,
    );
    if (!ok) return;
    try {
      await removeTag(tag.id);
      const next = new Set(activeTagIds);
      next.delete(tag.id);
      activeTagIds = next;
      await refreshAll();
    } catch (e) {
      error = String(e);
    }
  }

  // ---------- Clean up unused tags (v3.47.0 Atlas Tag-Cleanup) ----------
  //
  // One click on the rail head deletes every tag attached to zero documents —
  // the residue merges and bulk-removes leave behind. We confirm with the exact
  // count so the action is never a surprise, prune any now-stale ids out of the
  // active filter, then refresh so the rail + counts self-heal off the backend.
  async function onCleanupUnusedTags() {
    if (cleaningTags || unusedTagCount === 0) return;
    const n = unusedTagCount;
    const ok = window.confirm(
      `Remove ${n} unused tag${n === 1 ? "" : "s"}? ` +
        `${n === 1 ? "This tag is" : "These tags are"} not on any document.`,
    );
    if (!ok) return;
    cleaningTags = true;
    try {
      // Snapshot the ids we're about to drop so we can prune the filter; the
      // backend deletes by the same zero-doc predicate this set is built from.
      const doomed = new Set(
        tags.filter((t) => (tagCounts.get(t.id) ?? 0) === 0).map((t) => t.id),
      );
      const removed = await deleteUnusedTags();
      if (doomed.size > 0) {
        const next = new Set(activeTagIds);
        for (const id of doomed) next.delete(id);
        activeTagIds = next;
      }
      bulkSummary = `Removed ${removed} unused tag${removed === 1 ? "" : "s"}`;
      await refreshAll();
    } catch (e) {
      error = String(e);
    } finally {
      cleaningTags = false;
    }
  }

  // ---------- Edit tag color (v3.42.0 Atlas Tag-Color editing) ----------

  function openEditColor(tag: TagRecord) {
    editColorTag = tag;
    // Seed the picker with the tag's current swatch if it's one of ours,
    // otherwise leave the palette unselected (custom hsl()/rgb() defaults).
    editColorValue = tag.color ?? null;
    editColorBusy = false;
  }

  async function commitEditColor() {
    if (!editColorTag || editColorBusy) return;
    editColorBusy = true;
    try {
      const updated = await setTagColor(editColorTag.id, editColorValue);
      // Swap the updated row into the rail and every doc card that carries it,
      // so colored chips repaint without a full refetch.
      tags = tags.map((t) => (t.id === updated.id ? updated : t));
      docs = docs.map((d) => ({
        ...d,
        tags: d.tags.map((t) => (t.id === updated.id ? updated : t)),
      }));
      editColorTag = null;
    } catch (e) {
      error = String(e);
    } finally {
      editColorBusy = false;
    }
  }

  // ---------- Edit tag description (v3.51.0 Atlas Tag-Descriptions) ----------

  function openEditDescription(tag: TagRecord) {
    editDescTag = tag;
    // Seed the textarea with the current description (empty string when unset
    // — the textarea binding works on strings, and an empty save semantically
    // clears the column back to null on the backend).
    editDescDraft = tag.description ?? "";
    editDescError = null;
    editDescBusy = false;
  }

  async function commitEditDescription() {
    if (!editDescTag || editDescBusy) return;
    editDescBusy = true;
    editDescError = null;
    try {
      // The backend trims and treats trimmed-empty as "clear back to null";
      // pass the raw draft through and let the single source of truth decide.
      const updated = await setTagDescription(editDescTag.id, editDescDraft);
      // Swap the updated row into the rail and every doc card that carries it,
      // so the new tooltip (or its absence) appears without a full refetch.
      tags = tags.map((t) => (t.id === updated.id ? updated : t));
      docs = docs.map((d) => ({
        ...d,
        tags: d.tags.map((t) => (t.id === updated.id ? updated : t)),
      }));
      editDescTag = null;
    } catch (e) {
      // Rejected save (e.g. oversized) keeps the modal open with the reason
      // inline so the user can trim and retry, mirroring the rename pattern.
      editDescError = String(e);
    } finally {
      editDescBusy = false;
    }
  }

  // ---------- Rename tag inline (v3.43.0 Atlas Tag-Rename) ----------

  function startRenameTag(tag: TagRecord) {
    renameTagId = tag.id;
    renameDraft = tag.name;
    renameError = null;
    renameBusy = false;
  }

  function cancelRenameTag() {
    renameTagId = null;
    renameDraft = "";
    renameError = null;
    renameBusy = false;
  }

  async function commitRenameTag() {
    if (renameTagId === null || renameBusy) return;
    const id = renameTagId;
    const next = renameDraft.trim();
    const current = tags.find((t) => t.id === id);
    // Empty, or unchanged — treat as a cancel, no backend round-trip.
    if (!next || (current && current.name === next)) {
      cancelRenameTag();
      return;
    }
    renameBusy = true;
    renameError = null;
    try {
      const updated = await renameTag(id, next);
      // Swap the renamed row into the rail and every doc card that carries it
      // so labels repaint in place without a full refetch.
      tags = tags.map((t) => (t.id === updated.id ? updated : t));
      docs = docs.map((d) => ({
        ...d,
        tags: d.tags.map((t) => (t.id === updated.id ? updated : t)),
      }));
      cancelRenameTag();
    } catch (e) {
      // Keep the row in edit mode so the user can fix a collision and retry.
      renameError = String(e).replace(/^.*?library:\s*/, "");
      renameBusy = false;
    }
  }

  // Svelte action: focus + select-all a freshly-mounted input. Used by the
  // inline tag-rename field so the whole name is highlighted on open.
  function focusSelect(node: HTMLInputElement) {
    node.focus();
    node.select();
  }

  // ---------- Merge tag (v3.45.0 Atlas Tag-Merge) ----------

  function openMergeTag(tag: TagRecord) {
    mergeSourceTag = tag;
    mergeTargetId = null;
    mergeError = null;
    mergeBusy = false;
  }

  function closeMergeTag() {
    mergeSourceTag = null;
    mergeTargetId = null;
    mergeError = null;
    mergeBusy = false;
  }

  async function commitMergeTag() {
    if (!mergeSourceTag || mergeTargetId === null || mergeBusy) return;
    const sourceId = mergeSourceTag.id;
    const targetId = mergeTargetId;
    mergeBusy = true;
    mergeError = null;
    try {
      const target = await mergeTags(sourceId, targetId);
      // Drop the folded-away source from the rail and any active filter, then
      // swap the surviving target row in place. Doc cards re-point their
      // source chip to the target and de-dupe, so the grid repaints without a
      // full refetch.
      tags = tags
        .filter((t) => t.id !== sourceId)
        .map((t) => (t.id === target.id ? target : t));
      if (activeTagIds.has(sourceId)) {
        const next = new Set(activeTagIds);
        next.delete(sourceId);
        next.add(target.id);
        activeTagIds = next;
      }
      docs = docs.map((d) => {
        if (!d.tags.some((t) => t.id === sourceId)) return d;
        const seen = new Set<number>();
        const merged: TagRecord[] = [];
        for (const t of d.tags) {
          const swapped = t.id === sourceId ? target : t;
          if (seen.has(swapped.id)) continue;
          seen.add(swapped.id);
          merged.push(swapped);
        }
        return { ...d, tags: merged };
      });
      // Recently-used chips may have inherited the source's recency.
      recentTags = await recentlyUsedTags(8);
      closeMergeTag();
    } catch (e) {
      mergeError = String(e).replace(/^.*?library:\s*/, "");
      mergeBusy = false;
    }
  }

  async function commitNewTag() {
    const name = newTagName.trim();
    if (!name) return;
    try {
      const created = await addTag(name, newTagColor);
      tags = await listTags();
      // If this was a per-doc "Add Tag → New tag…" flow, attach it.
      if (pendingDocForTag) {
        const existingIds = pendingDocForTag.tags.map((t) => t.id);
        await setDocumentTags(pendingDocForTag.id, [...existingIds, created.id]);
        pendingDocForTag = null;
        await refreshDocs();
      }
      newTagOpen = false;
      newTagName = "";
    } catch (e) {
      error = String(e);
    }
  }

  // ---------- Card / context menu actions ----------

  function openDocInTab(doc: DocumentRecord) {
    menu = null;
    requestOpen(doc.path);
  }

  // v3.55.0 Atlas Doc-Inspector — drawer state. Holds the single doc
  // currently being inspected (or null when closed). The drawer
  // component re-fetches a fresh row on open so it never edits a stale
  // copy. setInspectorDoc() is what the drawer calls back when it
  // successfully edits title/notes/starred so the LibraryPanel can
  // splice the freshly-mutated doc into the grid without a full
  // listDocuments round-trip.
  let inspectorDoc = $state<DocumentRecord | null>(null);
  // v3.39.0 Atlas Tag-Suggest slice 52 — bulk review drawer + the
  // badge stats it surfaces in the toolbar. Stats refresh on mount,
  // after every bulk apply, and whenever the drawer closes.
  let bulkPanelOpen = $state(false);
  let bulkBadge = $state<TagSuggestionStats | null>(null);
  async function refreshBulkBadge() {
    try {
      bulkBadge = await tagSuggestionStats(undefined);
    } catch {
      bulkBadge = null;
    }
  }
  function openBulkPanel() {
    bulkPanelOpen = true;
  }
  function closeBulkPanel() {
    bulkPanelOpen = false;
    void refreshBulkBadge();
  }
  function onBulkApplied(_attached: number) {
    void refreshDocs();
    void refreshBulkBadge();
  }
  function openInspectorFor(doc: DocumentRecord) {
    menu = null;
    inspectorDoc = doc;
  }
  function closeInspector() {
    inspectorDoc = null;
  }
  function onInspectorUpdated(updated: DocumentRecord) {
    // Splice the refreshed row into `docs` so the grid card updates
    // immediately (title, ★, notes-hint badge if we ever add one).
    inspectorDoc = updated;
    const idx = docs.findIndex((d) => d.id === updated.id);
    if (idx >= 0) {
      docs = [...docs.slice(0, idx), updated, ...docs.slice(idx + 1)];
    }
    // If the starred-only toggle is on and the doc just got unstarred,
    // it must drop out of the grid — the simplest correct path is a
    // refresh, which also re-applies sort.
    if (starredOnly && !updated.starred) void refreshDocs();
  }
  function onInspectorRemoved(removedId: number) {
    docs = docs.filter((d) => d.id !== removedId);
    closeInspector();
  }

  // Star/unstar from the context menu (or anywhere else) — wraps the
  // setDocumentStarred IPC, splices the fresh row into `docs`, no
  // intervening reload. Independent of the inspector state so the
  // user can star without opening the drawer.
  async function onToggleStar(doc: DocumentRecord) {
    menu = null;
    try {
      const updated = await setDocumentStarred(doc.id, !doc.starred);
      const idx = docs.findIndex((d) => d.id === updated.id);
      if (idx >= 0) {
        docs = [...docs.slice(0, idx), updated, ...docs.slice(idx + 1)];
      }
      if (inspectorDoc && inspectorDoc.id === updated.id) {
        inspectorDoc = updated;
      }
      // Same starred-only side-effect as the inspector.
      if (starredOnly && !updated.starred) void refreshDocs();
    } catch (e) {
      error = `Star failed: ${String(e)}`;
    }
  }

  function openMenuFor(e: MouseEvent, doc: DocumentRecord) {
    e.preventDefault();
    e.stopPropagation();
    menu = { doc, x: e.clientX, y: e.clientY, submenu: null };
    // Surface the recently-used tags at the top of the Tags section.
    loadRecentTags();
  }

  function onMenuItemClick(e: MouseEvent) {
    // Prevent the window-level click handler from closing the menu
    // before we handle it.
    e.stopPropagation();
  }
  function onMenuKeyDown(e: KeyboardEvent) {
    if (e.key === "Escape") menu = null;
    e.stopPropagation();
  }

  async function onMenuToggleTag(doc: DocumentRecord, tag: TagRecord) {
    const existing = doc.tags.map((t) => t.id);
    const next = existing.includes(tag.id)
      ? existing.filter((id) => id !== tag.id)
      : [...existing, tag.id];
    try {
      await setDocumentTags(doc.id, next);
      await refreshDocs();
      // A tag was just applied/removed — re-rank the recently-used chips.
      await loadRecentTags();
      // Re-load the menu's `doc` from the fresh list so future
      // toggles see the new tag set.
      const fresh = docs.find((d) => d.id === doc.id);
      if (fresh && menu) menu = { ...menu, doc: fresh };
    } catch (e) {
      error = String(e);
    }
  }

  async function onMenuRemoveDoc(doc: DocumentRecord) {
    menu = null;
    const ok = window.confirm(
      `Remove "${doc.title ?? basename(doc.path)}" from your library?\n\nThe file stays on disk; Slab just forgets about it until the next scan.`,
    );
    if (!ok) return;
    try {
      await removeDocument(doc.id);
      await refreshDocs();
    } catch (e) {
      error = String(e);
    }
  }

  function openNewTagForDoc(doc: DocumentRecord) {
    pendingDocForTag = doc;
    newTagName = "";
    newTagColor = TAG_PALETTE[0];
    newTagOpen = true;
    menu = null;
  }

  // ---------- Helpers ----------

  function displayTitle(d: DocumentRecord): string {
    return d.title ?? basename(d.path).replace(/\.pdf$/i, "");
  }
  function relPath(d: DocumentRecord): string {
    const f = folders.find((x) => x.id === d.folder_id);
    if (!f) return d.path;
    return d.path.startsWith(f.path)
      ? d.path.slice(f.path.length).replace(/^[/\\]+/, "")
      : d.path;
  }
  function folderShortName(p: string): string {
    return basename(p) || p;
  }
</script>

<header class="content-header">
  <h1>Library</h1>
  <p class="subtitle">
    Index folders of PDFs. Click any doc to open it in a new Reader tab.
  </p>
</header>

<section class="library">
  <!-- Toolbar -->
  <div class="toolbar">
    <button class="primary" onclick={onAddFolder} disabled={scanning}>
      + Add Folder
    </button>
    <button class="ghost" onclick={onRescanAll} disabled={scanning || folders.length === 0}>
      {scanning ? "Scanning…" : "↻ Rescan All"}
    </button>
    {#if pendingOcrCount > 0}
      <button
        class="ghost ocr-all"
        onclick={onRunOcrAll}
        disabled={ocringAll}
        title="Run OCR on every scanned/mixed document currently visible"
      >
        {ocringAll
          ? `OCR'ing ${pendingOcrCount}…`
          : `🔍 OCR ${pendingOcrCount} pending`}
      </button>
    {/if}
    {#if docs.length > 0}
      <button
        class="ghost autotag-all"
        onclick={onAutoTagAll}
        disabled={autoTaggingAll}
        title="Use the Beacon AI provider to suggest tags for every visible document. Existing tags are preserved."
      >
        {autoTaggingAll
          ? `Auto-tagging ${docs.length}…`
          : `🏷️ Auto-tag ${docs.length}`}
      </button>
    {/if}
    <div class="search">
      <span class="search-icon">⌕</span>
      <input
        type="search"
        placeholder="Search by title or filename…"
        aria-label="Search library"
        value={query}
        oninput={onQueryInput}
      />
    </div>
    <div class="sort">
      <label>
        Sort:
        <select bind:value={sort}>
          <option value="added_desc">Recently added</option>
          <option value="last_seen_desc">Recently seen</option>
          <option value="title_asc">Title (A-Z)</option>
        </select>
      </label>
    </div>
    <button
      class="untagged-toggle"
      class:active={untaggedOnly}
      onclick={() => (untaggedOnly = !untaggedOnly)}
      aria-pressed={untaggedOnly}
      title="Show only documents that have no tags yet"
    >
      <span class="glyph" aria-hidden="true">&#x2298;</span>
      Untagged
    </button>
    <button
      class="untagged-toggle"
      class:active={starredOnly}
      onclick={() => (starredOnly = !starredOnly)}
      aria-pressed={starredOnly}
      title="Show only starred documents"
    >
      <span class="glyph" aria-hidden="true">&#x2605;</span>
      Starred
    </button>
    <button
      class="untagged-toggle select-toggle"
      class:active={selecting}
      onclick={toggleSelectMode}
      aria-pressed={selecting}
      title="Select multiple documents to tag them all at once"
    >
      <span class="glyph" aria-hidden="true">&#x2611;</span>
      {selecting ? "Done" : "Select"}
    </button>
    {#if bulkBadge && bulkBadge.untagged_docs_with_suggestions > 0}
      <button
        class="untagged-toggle bulk-suggest-btn"
        onclick={openBulkPanel}
        title="Review the heuristic tag suggestions for your untagged docs"
      >
        <span class="glyph" aria-hidden="true">&#x2728;</span>
        Review {bulkBadge.untagged_docs_with_suggestions >= 200 ? "200+" : bulkBadge.untagged_docs_with_suggestions}
      </button>
    {/if}
  </div>

  {#if error}
    <div class="status err">✕ {error}</div>
  {/if}
  {#if lastScanSummary && !error && !scanning}
    <div class="status ok">✓ {lastScanSummary}</div>
  {/if}
  {#if ocrSummary && !error && !ocringAll}
    <div class="status ok">✓ {ocrSummary}</div>
  {/if}
  {#if autoTagSummary && !error && !autoTaggingAll}
    <div class="status ok">✓ {autoTagSummary}</div>
  {/if}

  <div class="layout">
    <!-- Left rail -->
    <aside class="rail">
      <div class="rail-section">
        <div class="rail-head">
          <span class="rail-title">Folders</span>
          <span class="rail-count">{folders.length}</span>
        </div>
        <button
          class="rail-row"
          class:active={activeFolder === "all"}
          onclick={() => selectFolder("all")}
        >
          <span class="rail-icon">▦</span>
          <span class="rail-label">All</span>
          <span class="rail-meta">{totalDocs}</span>
        </button>
        {#each folders as f (f.id)}
          <div class="rail-row-wrap">
            <button
              class="rail-row"
              class:active={activeFolder === f.id}
              onclick={() => selectFolder(f.id)}
              title={f.path}
            >
              <span class="rail-icon">❐</span>
              <span class="rail-label">{folderShortName(f.path)}</span>
              <span class="rail-meta">
                {docs.filter((d) => d.folder_id === f.id).length}
              </span>
            </button>
            <button
              class="rail-row-x"
              title="Stop tracking {f.path}"
              aria-label="Stop tracking folder"
              onclick={() => onRemoveFolder(f)}
            >×</button>
          </div>
        {/each}
        {#if folders.length === 0 && initialized}
          <div class="rail-empty">No folders yet</div>
        {/if}
      </div>

      <div class="rail-section">
        <div class="rail-head">
          <span class="rail-title">Saved views</span>
          {#if filterIsNonDefault && saveViewDraftName === null}
            <button
              class="rail-clear"
              title="Pin the current filter (folder + tags + match mode + untagged + sort) as a saved view"
              aria-label="Save current filter as view"
              disabled={savedViewBusy}
              onclick={openSaveViewForm}
            >Save filter</button>
          {/if}
          {#if activeSavedViewId !== null && filterIsNonDefault && saveViewDraftName === null}
            <button
              class="rail-clear"
              title="Replace the active saved view with the current filter shape"
              aria-label="Update active saved view with current filter"
              disabled={savedViewBusy}
              onclick={onUpdateActiveSavedView}
            >Update</button>
          {/if}
          <span class="rail-count">{savedViews.length}</span>
        </div>
        {#if saveViewDraftName !== null}
          <div class="rail-rename">
            <input
              class="rail-rename-input"
              class:invalid={saveViewError !== null}
              value={saveViewDraftName}
              aria-label="Saved view name"
              placeholder="View name"
              use:focusSelect
              oninput={(e) => {
                saveViewDraftName = e.currentTarget.value;
                saveViewError = null;
              }}
              onkeydown={(e) => {
                if (e.key === "Enter") commitSaveView();
                else if (e.key === "Escape") cancelSaveView();
              }}
            />
            {#if saveViewError}
              <span class="rail-rename-error" title={saveViewError}
                >{saveViewError}</span
              >
            {/if}
          </div>
        {/if}
        {#each savedViews as v (v.id)}
          {#if savedViewRenameId === v.id}
            <div class="rail-rename">
              <input
                class="rail-rename-input"
                class:invalid={savedViewRenameError !== null}
                value={savedViewRenameDraft}
                aria-label="Rename saved view"
                use:focusSelect
                oninput={(e) => {
                  savedViewRenameDraft = e.currentTarget.value;
                  savedViewRenameError = null;
                }}
                onkeydown={(e) => {
                  if (e.key === "Enter") commitSavedViewRename();
                  else if (e.key === "Escape") cancelSavedViewRename();
                }}
                onblur={() => commitSavedViewRename()}
              />
              {#if savedViewRenameError}
                <span class="rail-rename-error" title={savedViewRenameError}
                  >{savedViewRenameError}</span
                >
              {/if}
            </div>
          {:else}
            <div class="rail-row-wrap rail-row-view">
              <button
                class="rail-pin"
                class:on={v.pinned}
                title={v.pinned
                  ? `Unpin "${v.name}" (drop from the top of the rail)`
                  : `Pin "${v.name}" to the top of the rail`}
                aria-label="Toggle pin"
                disabled={savedViewBusy}
                onclick={() => onTogglePinSavedView(v)}
              >★</button>
              <button
                class="rail-row"
                class:active={activeSavedViewId === v.id}
                title="Restore this saved filter"
                onclick={() => restoreSavedView(v)}
              >
                <span class="rail-icon">◆</span>
                <span class="rail-label">{v.name}</span>
              </button>
              <button
                class="rail-row-menu"
                title="Saved-view actions"
                aria-label="Open saved-view menu"
                aria-expanded={savedViewMenuId === v.id}
                disabled={savedViewBusy}
                onclick={(e) => {
                  e.stopPropagation();
                  savedViewMenuId = savedViewMenuId === v.id ? null : v.id;
                }}
              >⋯</button>
              {#if savedViewMenuId === v.id}
                <div class="rail-row-popover" role="menu">
                  <button
                    role="menuitem"
                    onclick={() => {
                      savedViewMenuId = null;
                      onTogglePinSavedView(v);
                    }}
                  >{v.pinned ? "Unpin" : "Pin to top"}</button>
                  <button
                    role="menuitem"
                    onclick={() => {
                      savedViewMenuId = null;
                      openSavedViewRename(v);
                    }}
                  >Rename…</button>
                  <button
                    role="menuitem"
                    onclick={() => {
                      savedViewMenuId = null;
                      onDuplicateSavedView(v);
                    }}
                  >Duplicate</button>
                  {#if (() => {
                    const group = savedViews.filter((x) => !!x.pinned === !!v.pinned);
                    return group.findIndex((x) => x.id === v.id) > 0;
                  })()}
                    <button
                      role="menuitem"
                      onclick={() => {
                        savedViewMenuId = null;
                        onBumpSavedView(v, -1);
                      }}
                    >Move up</button>
                  {/if}
                  {#if (() => {
                    const group = savedViews.filter((x) => !!x.pinned === !!v.pinned);
                    const i = group.findIndex((x) => x.id === v.id);
                    return i >= 0 && i < group.length - 1;
                  })()}
                    <button
                      role="menuitem"
                      onclick={() => {
                        savedViewMenuId = null;
                        onBumpSavedView(v, 1);
                      }}
                    >Move down</button>
                  {/if}
                  <hr />
                  <button
                    role="menuitem"
                    class="danger"
                    onclick={() => {
                      savedViewMenuId = null;
                      onDeleteSavedView(v);
                    }}
                  >Delete view</button>
                </div>
              {/if}
            </div>
          {/if}
        {/each}
        {#if savedViews.length === 0 && saveViewDraftName === null && initialized}
          <div class="rail-empty">
            {#if filterIsNonDefault}
              No saved views yet
            {:else}
              Filter the library, then pin it as a view
            {/if}
          </div>
        {/if}
      </div>

      <div class="rail-section">
        <div class="rail-head">
          <span class="rail-title">Tags</span>
          {#if tagFilterActive}
            <button
              class="rail-clear"
              title="Clear the tag filter (deselect all tags, drop the untagged filter, reset match mode)"
              aria-label="Clear tag filter"
              onclick={clearTagFilter}
            >Clear</button>
          {/if}
          {#if activeTagIds.size > 1}
            <button
              class="rail-match"
              class:any={tagMatch === "any"}
              title={tagMatch === "all"
                ? "Showing docs with ALL selected tags - click to match ANY"
                : "Showing docs with ANY selected tag - click to require ALL"}
              aria-label="Toggle tag match mode"
              onclick={() => (tagMatch = tagMatch === "all" ? "any" : "all")}
            >{tagMatch === "all" ? "All tags" : "Any tag"}</button>
          {/if}
          {#if unusedTagCount > 0}
            <button
              class="rail-cleanup"
              title="Remove {unusedTagCount} tag{unusedTagCount === 1
                ? ''
                : 's'} attached to no documents"
              aria-label="Remove unused tags"
              disabled={cleaningTags}
              onclick={onCleanupUnusedTags}
            >Clean up {unusedTagCount}</button>
          {/if}
          {#if tags.length > 1}
            <button
              class="rail-sort"
              title={tagSort === "name"
                ? "Sorted A-Z - click to sort by most used"
                : "Sorted by most used - click to sort A-Z"}
              aria-label="Toggle tag sort order"
              onclick={() => (tagSort = tagSort === "name" ? "count" : "name")}
            >{tagSort === "name" ? "A-Z" : "Most used"}</button>
          {/if}
          <span class="rail-count">{tags.length}</span>
        </div>
        {#each sortedTags as t (t.id)}
          <div class="rail-row-wrap">
            {#if renameTagId === t.id}
              <div class="rail-rename">
                <input
                  class="rail-rename-input"
                  class:invalid={renameError !== null}
                  value={renameDraft}
                  aria-label="Rename tag {t.name}"
                  use:focusSelect
                  oninput={(e) => {
                    renameDraft = e.currentTarget.value;
                    renameError = null;
                  }}
                  onkeydown={(e) => {
                    if (e.key === "Enter") commitRenameTag();
                    else if (e.key === "Escape") cancelRenameTag();
                  }}
                  onblur={commitRenameTag}
                />
                {#if renameError}
                  <span class="rail-rename-error" title={renameError}>{renameError}</span>
                {/if}
              </div>
            {:else}
              <button
                class="rail-row tag"
                class:active={activeTagIds.has(t.id)}
                title={t.description ?? undefined}
                onclick={() => toggleTag(t)}
              >
                <span class="rail-icon dot" style:background={t.color ?? "var(--text-3)"}></span>
                <span class="rail-label">{t.name}</span>
                <span class="rail-meta">{tagCounts.get(t.id) ?? 0}</span>
              </button>
              <button
                class="rail-row-x"
                title="Rename tag"
                aria-label="Rename tag"
                onclick={() => startRenameTag(t)}
              >&#9998;</button>
              <button
                class="rail-row-x"
                title={t.description
                  ? "Edit notes — " + t.description
                  : "Add notes"}
                aria-label="Edit tag notes"
                class:has-notes={!!t.description}
                onclick={() => openEditDescription(t)}
              >&#182;</button>
              <button
                class="rail-row-x"
                title="Merge into another tag"
                aria-label="Merge tag"
                onclick={() => openMergeTag(t)}
              >&#8703;</button>
              <button
                class="rail-row-x"
                title="Edit color"
                aria-label="Edit tag color"
                onclick={() => openEditColor(t)}
              >&#9679;</button>
              <button
                class="rail-row-x"
                title="Delete tag"
                aria-label="Delete tag"
                onclick={() => onRemoveTag(t)}
              >×</button>
            {/if}
          </div>
        {/each}
        <button class="rail-add" onclick={onCreateTopLevelTag}>+ New tag</button>
      </div>

      <!-- v3.32.0 Atlas — Collections rail -->
      <CollectionsSidebar onSelect={onCollectionSelect} />
    </aside>

    <!-- Main grid -->
    <main class="main">
      {#if loading && !initialized}
        <div class="empty">
          <div class="empty-icon">⌛</div>
          <div class="empty-title">Loading library…</div>
        </div>
      {:else if folders.length === 0}
        <div class="empty">
          <div class="empty-icon">❐</div>
          <div class="empty-title">Build your library</div>
          <div class="empty-sub">
            Add a folder of PDFs to index. Slab will track them, tag them,
            and let you search across the whole collection.
          </div>
          <button class="primary" onclick={onAddFolder}>+ Add Folder</button>
        </div>
      {:else if docs.length === 0}
        <div class="empty">
          <div class="empty-icon">⌕</div>
          <div class="empty-title">
            {query.trim() ? "No matches" : "No PDFs in this view"}
          </div>
          <div class="empty-sub">
            {query.trim()
              ? `No documents match "${query.trim()}". Try clearing filters or searching differently.`
              : "This folder has been indexed but contains no PDFs yet, or all PDFs are filtered out."}
          </div>
        </div>
      {:else}
        <div class="grid">
          {#each docs as d, i (d.id)}
            <div
              bind:this={cardEls[i]}
              class="card"
              class:vim-focused={i === vimFocusIdx}
              class:selectable={selecting}
              class:selected={selecting && selectedDocIds.has(d.id)}
              role="button"
              tabindex="0"
              draggable={true}
              ondragstart={(e) => {
                if (!e.dataTransfer) return;
                e.dataTransfer.effectAllowed = "copy";
                // When a multi-selection is active, drag the whole set so a
                // drop onto a collection moves every selected doc at once.
                const dragIds =
                  selecting && selectedDocIds.has(d.id)
                    ? Array.from(selectedDocIds)
                    : [d.id];
                e.dataTransfer.setData(
                  "application/x-slab-doc-ids",
                  JSON.stringify(dragIds),
                );
                // Also stash the title for friendlier drag previews on
                // platforms that surface text/plain in the OS overlay.
                e.dataTransfer.setData("text/plain", displayTitle(d));
              }}
              oncontextmenu={(e) => openMenuFor(e, d)}
              onclick={() =>
                selecting ? toggleDocSelected(d.id) : openDocInTab(d)}
              onkeydown={(e) => {
                if (e.key === "Enter" || e.key === " ")
                  selecting ? toggleDocSelected(d.id) : openDocInTab(d);
              }}
            >
              <div class="card-head">
                {#if selecting}
                  <span
                    class="card-check"
                    class:on={selectedDocIds.has(d.id)}
                    aria-hidden="true"
                  >{selectedDocIds.has(d.id) ? "\u2713" : ""}</span>
                {/if}
                {#if d.starred}
                  <span
                    class="card-star"
                    aria-label="Starred"
                    title="Starred"
                  >&#x2605;</span>
                {/if}
                <div class="card-title" title={d.path}>{displayTitle(d)}</div>
                {#if d.notes}
                  <span
                    class="card-notes-hint"
                    aria-label="Has notes"
                    title={`Has notes — open inspector to read`}
                  >&#x270e;</span>
                {/if}
                <button
                  class="card-menu"
                  title="More"
                  aria-label="Open menu"
                  onclick={(e) => { e.stopPropagation(); openMenuFor(e, d); }}
                >⋯</button>
              </div>
              <div class="card-meta">
                <span class="card-pages">{d.pages ?? "?"} pages</span>
                <span class="card-sep">·</span>
                <span class="card-seen">{formatRelTime(d.last_seen_at * 1000)}</span>
                {#if showOcrBadge(d.ocr_state)}
                  <span class="card-sep">·</span>
                  <span class="ocr-badge ocr-badge-{d.ocr_state}" title={ocrStateLabel(d.ocr_state)}>
                    {ocrStateLabel(d.ocr_state)}
                  </span>
                {/if}
              </div>
              <div class="card-path" title={d.path}>{relPath(d)}</div>
              {#if isOcrCandidate(d.ocr_state) || d.ocr_state === "ocr_done"}
                <div class="card-actions" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()} role="toolbar" tabindex="-1">
                  {#if isOcrCandidate(d.ocr_state)}
                    <button
                      class="card-action"
                      disabled={ocringDocIds.has(d.id) || ocringAll}
                      onclick={() => onRunOcrFor(d)}
                      title="Run OCR on this document"
                    >
                      {ocringDocIds.has(d.id) ? "OCR'ing…" : "🔍 Run OCR"}
                    </button>
                  {/if}
                  {#if d.ocr_state === "ocr_done" && d.ocr_output_path}
                    <button
                      class="card-action"
                      onclick={() => openOcrOutput(d)}
                      title={d.ocr_output_path}
                    >
                      📄 Open OCR'd
                    </button>
                  {/if}
                  <button
                    class="card-action"
                    disabled={autoTaggingDocIds.has(d.id) || autoTaggingAll}
                    onclick={() => onAutoTagFor(d)}
                    title="Suggest 3–5 topical tags using the Beacon AI provider"
                  >
                    {autoTaggingDocIds.has(d.id) ? "Tagging…" : "🏷️ Auto-tag"}
                  </button>
                </div>
              {:else}
                <div class="card-actions" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()} role="toolbar" tabindex="-1">
                  <button
                    class="card-action"
                    disabled={autoTaggingDocIds.has(d.id) || autoTaggingAll}
                    onclick={() => onAutoTagFor(d)}
                    title="Suggest 3–5 topical tags using the Beacon AI provider"
                  >
                    {autoTaggingDocIds.has(d.id) ? "Tagging…" : "🏷️ Auto-tag"}
                  </button>
                </div>
              {/if}
              {#if d.tags.length > 0}
                <div class="card-tags">
                  {#each d.tags.slice(0, 3) as t (t.id)}
                    <span
                      class="chip"
                      style:border-left-color={t.color ?? "var(--text-3)"}
                      title={t.description ?? undefined}
                    >{t.name}</span>
                  {/each}
                  {#if d.tags.length > 3}
                    <span class="chip more">+{d.tags.length - 3}</span>
                  {/if}
                </div>
              {/if}
              <SuggestedTagsRow
                docId={d.id}
                onAccepted={(tag) => onTagSuggestionAccepted(d.id, tag)}
              />
            </div>
          {/each}
        </div>
      {/if}
    </main>
  </div>
</section>

<!-- v3.41.0 Atlas Bulk Tag-Apply — floating action bar -->
{#if selecting}
  <div class="bulk-bar" role="toolbar" tabindex="-1" aria-label="Bulk tag actions">
    <button
      class="bulk-select-all"
      onclick={selectAllVisible}
      title={allVisibleSelected ? "Deselect all" : "Select all visible"}
    >
      <span class="glyph" aria-hidden="true"
        >{allVisibleSelected ? "\u2612" : "\u2610"}</span>
      {allVisibleSelected ? "None" : "All"}
    </button>
    <span class="bulk-count">
      {selectedCount} selected
    </span>
    <div class="bulk-actions">
      <div class="bulk-tag-wrap">
        <button
          class="bulk-apply"
          disabled={selectedCount === 0 || bulkBusy}
          onclick={() => (bulkTagPickerOpen = !bulkTagPickerOpen)}
          aria-expanded={bulkTagPickerOpen}
          title="Apply or remove a tag across all selected documents"
        >
          {bulkBusy ? "Working\u2026" : "Tag selected\u2026"}
        </button>
        {#if bulkTagPickerOpen && selectedCount > 0}
          <div class="bulk-picker" role="menu" tabindex="-1">
            <div class="bulk-picker-section">Apply a tag</div>
            <div class="bulk-new-tag">
              <input
                type="text"
                placeholder="New or existing tag…"
                aria-label="Tag name to apply"
                bind:value={bulkNewTagName}
                onkeydown={(e) => {
                  if (e.key === "Enter") onBulkApplyTag(bulkNewTagName);
                }}
              />
              <button
                class="bulk-new-go"
                disabled={!bulkNewTagName.trim() || bulkBusy}
                onclick={() => onBulkApplyTag(bulkNewTagName)}
              >Apply</button>
            </div>
            {#if tags.length > 0}
              <div class="bulk-picker-list">
                {#each tags as t (t.id)}
                  <div class="bulk-picker-row">
                    <button
                      class="bulk-picker-tag"
                      disabled={bulkBusy}
                      onclick={() => onBulkApplyTag(t.name)}
                      title={`Apply "${t.name}" to ${selectedCount} doc${selectedCount === 1 ? "" : "s"}`}
                    >
                      <span
                        class="dot small"
                        style:background={t.color ?? "var(--text-3)"}
                      ></span>
                      <span class="bulk-picker-name">{t.name}</span>
                    </button>
                    <button
                      class="bulk-picker-x"
                      disabled={bulkBusy}
                      aria-label={`Remove ${t.name} from selection`}
                      title={`Remove "${t.name}" from the ${selectedCount} selected doc${selectedCount === 1 ? "" : "s"}`}
                      onclick={() => onBulkRemoveTag(t)}
                    >&minus;</button>
                  </div>
                {/each}
              </div>
            {/if}
          </div>
        {/if}
      </div>
      <!-- v3.53.0 Atlas Collections — Slice 27 bulk add-to-collection -->
      <div class="bulk-coll-wrap">
        <button
          class="bulk-apply"
          disabled={selectedCount === 0 || bulkBusy}
          onclick={openBulkCollectionPicker}
          aria-expanded={bulkCollectionPickerOpen}
          title="Add all selected documents to a collection"
        >
          Add to collection&hellip;
        </button>
        {#if bulkCollectionPickerOpen && selectedCount > 0}
          <div class="bulk-picker" role="menu" tabindex="-1">
            <div class="bulk-picker-section">Add to collection</div>
            {#if bulkCollectionsLoading}
              <div class="bulk-picker-empty">Loading collections&hellip;</div>
            {:else if bulkCollections.length === 0}
              <div class="bulk-picker-empty">
                No collections yet — create one from the left rail first.
              </div>
            {:else}
              <div class="bulk-picker-list">
                {#each bulkCollections as c (c.id)}
                  <div class="bulk-picker-row">
                    <button
                      class="bulk-picker-tag"
                      disabled={bulkBusy}
                      onclick={() => onBulkAddToCollection(c)}
                      title={`Add ${selectedCount} doc${
                        selectedCount === 1 ? "" : "s"
                      } to "${c.name}"`}
                    >
                      <span
                        class="dot small"
                        style:background={c.color ?? "var(--text-3)"}
                      ></span>
                      <span class="bulk-picker-name">{c.name}</span>
                      <span class="bulk-picker-count">{c.doc_count}</span>
                    </button>
                  </div>
                {/each}
              </div>
            {/if}
          </div>
        {/if}
      </div>
      <button
        class="bulk-clear"
        disabled={selectedCount === 0 || bulkBusy}
        onclick={clearSelection}
      >Clear</button>
    </div>
  </div>
{/if}
{#if bulkSummary && !error}
  <div class="bulk-toast" role="status">{bulkSummary}</div>
{/if}

<!-- Context menu (right-click / ⋯) -->
{#if menu}
  <div
    class="menu"
    role="menu"
    tabindex="-1"
    style:left="{menu.x}px"
    style:top="{menu.y}px"
    onclick={onMenuItemClick}
    onkeydown={onMenuKeyDown}
  >
    <button class="menu-item" onclick={() => openDocInTab(menu!.doc)}>
      <span>Open in Reader tab</span>
    </button>
    <button class="menu-item" onclick={() => openInspectorFor(menu!.doc)}>
      <span>Inspect&hellip;</span>
    </button>
    <button class="menu-item" onclick={() => onToggleStar(menu!.doc)}>
      <span>{menu.doc.starred ? "Unstar" : "Star"}</span>
    </button>
    {#if isOcrCandidate(menu.doc.ocr_state)}
      <button
        class="menu-item"
        disabled={ocringDocIds.has(menu.doc.id) || ocringAll}
        onclick={() => onRunOcrFor(menu!.doc)}
      >
        <span>
          {ocringDocIds.has(menu.doc.id) ? "OCR'ing…" : "🔍 Run OCR"}
        </span>
      </button>
    {/if}
    {#if menu.doc.ocr_state === "ocr_done" && menu.doc.ocr_output_path}
      <button class="menu-item" onclick={() => openOcrOutput(menu!.doc)}>
        <span>📄 Open OCR'd version</span>
      </button>
    {/if}
    <button
      class="menu-item"
      disabled={autoTaggingDocIds.has(menu.doc.id) || autoTaggingAll}
      onclick={() => onAutoTagFor(menu!.doc)}
    >
      <span>
        {autoTaggingDocIds.has(menu.doc.id) ? "Tagging…" : "🏷️ Auto-tag"}
      </span>
    </button>
    <div class="menu-sep"></div>
    <div class="menu-section">Tags</div>
    {#if recentChips.length > 0}
      <div class="recent-tags" role="group" aria-label="Recently used tags">
        {#each recentChips as rt (rt.id)}
          <button
            class="recent-chip"
            title={`Apply "${rt.name}"`}
            onclick={() => onMenuToggleTag(menu!.doc, rt)}
          >
            <span
              class="dot small"
              style:background={rt.color ?? "var(--text-3)"}
            ></span>
            <span class="recent-chip-label">{rt.name}</span>
          </button>
        {/each}
      </div>
    {/if}
    {#each tags as t (t.id)}
      {@const attached = menu.doc.tags.some((dt) => dt.id === t.id)}
      <button
        class="menu-item tag-toggle"
        onclick={() => onMenuToggleTag(menu!.doc, t)}
      >
        <span class="dot small" style:background={t.color ?? "var(--text-3)"}></span>
        <span class="menu-label">{t.name}</span>
        {#if attached}<span class="menu-check">✓</span>{/if}
      </button>
    {/each}
    <button class="menu-item" onclick={() => openNewTagForDoc(menu!.doc)}>
      <span class="menu-label">+ New tag…</span>
    </button>
    <div class="menu-sep"></div>
    <button class="menu-item danger" onclick={() => onMenuRemoveDoc(menu!.doc)}>
      <span>Remove from library</span>
    </button>
  </div>
{/if}

<!-- v3.55.0 Atlas Doc-Inspector slice 37 — slide-from-right drawer for
     editing a single doc's title/notes/star + viewing its metadata. -->
<DocInspectorPanel
  doc={inspectorDoc}
  onUpdate={onInspectorUpdated}
  onRemove={onInspectorRemoved}
  onClose={closeInspector}
/>

<!-- v3.39.0 Atlas Tag-Suggest slice 52 — bulk review drawer wiring
     slices 48-51 (bulk-accept + granular undismiss + filter-aware
     bulk + stats badge) into one demo-able surface. -->
<BulkTagSuggestionsPanel
  open={bulkPanelOpen}
  filter={buildCurrentFilter()}
  onApplied={onBulkApplied}
  onClose={closeBulkPanel}
/>

<!-- New tag modal -->
{#if newTagOpen}
  <div
    class="modal-backdrop"
    role="button"
    tabindex="-1"
    aria-label="Close"
    onclick={() => (newTagOpen = false)}
    onkeydown={(e) => { if (e.key === "Escape") newTagOpen = false; }}
  >
    <div class="modal" role="dialog" tabindex="-1" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()}>
      <div class="modal-title">New tag</div>
      <input
        type="text"
        placeholder="Tag name"
        aria-label="New tag name"
        bind:value={newTagName}
        autofocus
        onkeydown={(e) => { if (e.key === "Enter") commitNewTag(); }}
      />
      <div class="palette">
        {#each TAG_PALETTE as c (c)}
          <button
            class="swatch"
            class:active={newTagColor === c}
            style:background={c}
            aria-label="Pick color {c}"
            onclick={() => (newTagColor = c)}
          ></button>
        {/each}
      </div>
      <div class="modal-actions">
        <button class="ghost" onclick={() => (newTagOpen = false)}>Cancel</button>
        <button class="primary" onclick={commitNewTag} disabled={!newTagName.trim()}>
          Create
        </button>
      </div>
    </div>
  </div>
{/if}

<!-- Edit tag color modal (v3.42.0 Atlas Tag-Color editing) -->
{#if editColorTag}
  <div
    class="modal-backdrop"
    role="button"
    tabindex="-1"
    aria-label="Close"
    onclick={() => (editColorTag = null)}
    onkeydown={(e) => { if (e.key === "Escape") editColorTag = null; }}
  >
    <div class="modal" role="dialog" tabindex="-1" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()}>
      <div class="modal-title">Tag color</div>
      <div class="color-preview">
        <span class="dot preview" style:background={editColorValue ?? "var(--text-3)"}></span>
        <span class="color-preview-name">{editColorTag.name}</span>
      </div>
      <div class="palette">
        {#each TAG_PALETTE as c (c)}
          <button
            class="swatch"
            class:active={editColorValue === c}
            style:background={c}
            aria-label="Pick color {c}"
            onclick={() => (editColorValue = c)}
          ></button>
        {/each}
        <button
          class="swatch default-swatch"
          class:active={editColorValue === null}
          title="Default (automatic)"
          aria-label="Use default color"
          onclick={() => (editColorValue = null)}
        >&#8856;</button>
      </div>
      <div class="modal-actions">
        <button class="ghost" onclick={() => (editColorTag = null)}>Cancel</button>
        <button class="primary" onclick={commitEditColor} disabled={editColorBusy}>
          {editColorBusy ? "Saving…" : "Save"}
        </button>
      </div>
    </div>
  </div>
{/if}

<!-- Edit tag description modal (v3.51.0 Atlas Tag-Descriptions) -->
{#if editDescTag}
  <div
    class="modal-backdrop"
    role="button"
    tabindex="-1"
    aria-label="Close"
    onclick={() => (editDescTag = null)}
    onkeydown={(e) => { if (e.key === "Escape") editDescTag = null; }}
  >
    <div
      class="modal"
      role="dialog"
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
    >
      <div class="modal-title">Tag notes</div>
      <div class="desc-lead">
        <span class="dot" style:background={editDescTag.color ?? "var(--text-3)"}></span>
        <span class="desc-name">{editDescTag.name}</span>
      </div>
      <textarea
        class="desc-textarea"
        class:invalid={editDescError !== null}
        placeholder="Optional notes — shown as a tooltip on the rail row and every doc chip."
        rows="4"
        maxlength={MAX_TAG_DESCRIPTION_LEN}
        aria-label="Tag notes"
        bind:value={editDescDraft}
        oninput={() => (editDescError = null)}
        onkeydown={(e) => {
          if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
            e.preventDefault();
            commitEditDescription();
          }
        }}
      ></textarea>
      <div class="desc-meta">
        <span class="desc-count" class:near-limit={editDescDraft.length > MAX_TAG_DESCRIPTION_LEN - 50}>
          {editDescDraft.length} / {MAX_TAG_DESCRIPTION_LEN}
        </span>
        <span class="desc-hint">⌘↩ to save</span>
      </div>
      {#if editDescError}
        <div class="desc-error" title={editDescError}>{editDescError}</div>
      {/if}
      <div class="modal-actions">
        <button class="ghost" onclick={() => (editDescTag = null)}>Cancel</button>
        <button class="primary" onclick={commitEditDescription} disabled={editDescBusy}>
          {editDescBusy ? "Saving…" : editDescDraft.trim() ? "Save" : "Clear"}
        </button>
      </div>
    </div>
  </div>
{/if}

<!-- Merge tag modal (v3.45.0 Atlas Tag-Merge) -->
{#if mergeSourceTag}
  <div
    class="modal-backdrop"
    role="button"
    tabindex="-1"
    aria-label="Close"
    onclick={closeMergeTag}
    onkeydown={(e) => { if (e.key === "Escape") closeMergeTag(); }}
  >
    <div class="modal" role="dialog" tabindex="-1" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()}>
      <div class="modal-title">Merge tag</div>
      <div class="merge-lead">
        Fold <span class="merge-source">{mergeSourceTag.name}</span> into another
        tag. Every document tagged
        <span class="merge-source">{mergeSourceTag.name}</span> will be re-tagged,
        and <span class="merge-source">{mergeSourceTag.name}</span> will be deleted.
      </div>
      {#if mergeCandidates.length === 0}
        <div class="merge-empty">No other tags to merge into.</div>
      {:else}
        <div class="merge-list">
          {#each mergeCandidates as c (c.id)}
            <button
              class="merge-option"
              class:active={mergeTargetId === c.id}
              onclick={() => { mergeTargetId = c.id; mergeError = null; }}
            >
              <span class="dot" style:background={c.color ?? "var(--text-3)"}></span>
              <span class="merge-option-name">{c.name}</span>
              {#if mergeTargetId === c.id}<span class="merge-check">&#10003;</span>{/if}
            </button>
          {/each}
        </div>
      {/if}
      {#if mergeError}
        <div class="merge-error" title={mergeError}>{mergeError}</div>
      {/if}
      <div class="modal-actions">
        <button class="ghost" onclick={closeMergeTag}>Cancel</button>
        <button
          class="primary"
          onclick={commitMergeTag}
          disabled={mergeBusy || mergeTargetId === null}
        >
          {mergeBusy ? "Merging…" : "Merge"}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .library {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    padding: 12px 20px 20px;
    gap: 12px;
  }

  /* Toolbar */
  .toolbar {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }
  .toolbar .primary {
    background: var(--accent);
    border: 1px solid var(--accent);
    color: var(--bg);
    padding: 7px 14px;
    border-radius: var(--r-sm);
    font-size: 13px;
    font-weight: 600;
  }
  .toolbar .primary:hover:not(:disabled) { background: var(--accent-2); }
  .toolbar .primary:disabled { opacity: 0.55; }
  .toolbar .ghost {
    background: var(--bg-3);
    border: 1px solid var(--border);
    color: var(--text);
    padding: 7px 14px;
    border-radius: var(--r-sm);
    font-size: 13px;
  }
  .toolbar .ghost:hover:not(:disabled) { border-color: var(--border-strong); }
  .toolbar .ghost:disabled { opacity: 0.55; }

  .search {
    flex: 1;
    min-width: 200px;
    display: flex;
    align-items: center;
    gap: 6px;
    background: var(--bg-3);
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    padding: 0 10px;
  }
  .search-icon { color: var(--text-3); font-size: 14px; }
  .search input {
    flex: 1;
    background: transparent;
    border: 0;
    outline: 0;
    color: var(--text);
    font-size: 13px;
    padding: 7px 0;
  }
  .search input::placeholder { color: var(--text-3); }

  .sort label {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--text-3);
    font-size: 12px;
  }
  .sort select {
    background: var(--bg-3);
    border: 1px solid var(--border);
    color: var(--text);
    padding: 6px 8px;
    border-radius: var(--r-sm);
    font-size: 12px;
  }

  /* v3.40.0 Atlas Untagged-Filter toggle chip */
  .untagged-toggle {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    background: var(--bg-3);
    border: 1px solid var(--border);
    color: var(--text-3);
    padding: 6px 11px;
    border-radius: var(--r-sm);
    font-size: 12px;
    cursor: pointer;
    white-space: nowrap;
    transition:
      background 120ms ease,
      border-color 120ms ease,
      color 120ms ease;
  }
  .untagged-toggle:hover {
    color: var(--text);
    border-color: var(--text-3);
  }
  .untagged-toggle.active {
    background: color-mix(in oklab, var(--accent, #7c3aed) 18%, transparent);
    border-color: color-mix(in oklab, var(--accent, #7c3aed) 55%, transparent);
    color: var(--text);
  }
  .untagged-toggle .glyph {
    font-size: 13px;
    line-height: 1;
    opacity: 0.85;
  }

  /* v3.41.0 Atlas Bulk Tag-Apply — Select toggle reuses untagged-toggle
     chrome; active state mirrors the accent treatment. */
  .select-toggle.active {
    background: color-mix(in oklab, var(--accent, #7c3aed) 18%, transparent);
    border-color: color-mix(in oklab, var(--accent, #7c3aed) 55%, transparent);
    color: var(--text);
  }

  /* Layout */
  .layout {
    display: flex;
    flex: 1;
    min-height: 0;
    gap: 16px;
  }
  .rail {
    width: 220px;
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    gap: 16px;
    overflow-y: auto;
    padding-right: 4px;
  }
  .main {
    flex: 1;
    min-width: 0;
    overflow-y: auto;
    padding-right: 4px;
  }

  /* Rail */
  .rail-section { display: flex; flex-direction: column; gap: 2px; }
  .rail-head {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    padding: 0 6px 6px;
    color: var(--text-3);
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }
  .rail-title { font-weight: 600; }
  .rail-count {
    font-variant-numeric: tabular-nums;
    color: var(--text-3);
  }
  /* v3.46.0 Atlas Tag-Usage-Counts — alphabetical/most-used sort toggle on
     the Tags rail head. Sits between the title and the count; monochrome,
     uppercase like the rest of the head chrome. */
  .rail-sort {
    margin-left: auto;
    margin-right: 8px;
    background: transparent;
    border: 0;
    color: var(--text-3);
    cursor: pointer;
    font-size: 9px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    padding: 0;
  }
  .rail-sort:hover { color: var(--text); }
  /* v3.48.0 Atlas Tag-Combinator — All/Any toggle on the Tags rail head,
     shown only when >1 tag is selected (when AND vs OR changes the result).
     Same muted uppercase chrome as the sort toggle; `margin-left: auto`
     right-aligns the whole head chrome group when it's the first auto-margin
     element present (it precedes cleanup + sort in DOM order). The non-default
     "Any" mode gets an accent so the active union state is legible at a glance. */
  .rail-match {
    margin-left: auto;
    margin-right: 8px;
    background: transparent;
    border: 0;
    color: var(--text-3);
    cursor: pointer;
    font-size: 9px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    padding: 0;
  }
  .rail-match:hover { color: var(--text); }
  .rail-match.any { color: var(--accent, #6aa3ff); }
  /* v3.47.0 Atlas Tag-Cleanup — one-click prune of zero-document tags. Same
     muted uppercase chrome as the sort toggle, but a danger-tinted hover marks
     it destructive. `margin-left: auto` right-aligns the head group (cleanup +
     sort + count) when it's the first auto-margin element present. */
  .rail-cleanup {
    margin-left: auto;
    margin-right: 8px;
    background: transparent;
    border: 0;
    color: var(--text-3);
    cursor: pointer;
    font-size: 9px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    padding: 0;
  }
  .rail-cleanup:hover:not(:disabled) { color: var(--danger, #e54); }
  .rail-cleanup:disabled { opacity: 0.5; cursor: default; }
  /* v3.49.0 Atlas Tag-Filter-Clear — one-click reset of the whole tag filter
     (selected tags + untagged toggle + match mode). Same muted uppercase chrome
     as the sort/match toggles; this is a non-destructive reset (no data is
     deleted), so it gets the neutral hover-to-text treatment, NOT the danger
     tint the cleanup prune uses. It's first in the rail-head chrome group, so
     its `margin-left: auto` right-aligns the group like its siblings. */
  .rail-clear {
    margin-left: auto;
    margin-right: 8px;
    background: transparent;
    border: 0;
    color: var(--text-3);
    cursor: pointer;
    font-size: 9px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    padding: 0;
  }
  .rail-clear:hover { color: var(--text); }
  .rail-row-wrap {
    display: flex;
    align-items: stretch;
    border-radius: var(--r-sm);
  }
  .rail-row-wrap:hover { background: var(--bg-3); }
  .rail-row {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 8px;
    background: transparent;
    border: 1px solid transparent;
    color: var(--text-2);
    padding: 6px 8px;
    border-radius: var(--r-sm);
    font-size: 13px;
    text-align: left;
    min-width: 0;
  }
  .rail-row:hover:not(:disabled) { color: var(--text); }
  .rail-row.active {
    background: var(--bg-3);
    color: var(--text);
    border-color: var(--border);
  }
  .rail-icon {
    width: 14px;
    text-align: center;
    color: var(--accent);
    opacity: 0.85;
    flex-shrink: 0;
  }
  .rail-icon.dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    margin: 0 3px;
  }
  .rail-label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .rail-meta {
    font-size: 11px;
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
  }
  .rail-row-x {
    background: transparent;
    border: 0;
    color: var(--text-3);
    padding: 0 8px;
    cursor: pointer;
    opacity: 0;
    font-size: 14px;
  }
  .rail-row-wrap:hover .rail-row-x { opacity: 1; }
  .rail-row-x:hover { color: var(--accent); }

  /* Saved-views rail (v3.56.0 Atlas Saved-Views-Polish) — pin glyph +
     overflow menu, kept apart from the generic rail-row-x class so the
     other rails (folders, tags) stay untouched. The wrap is
     position:relative so the popover anchors to it. */
  .rail-row-view {
    position: relative;
  }
  .rail-pin {
    background: transparent;
    border: 0;
    color: var(--text-3);
    padding: 0 4px 0 8px;
    font-size: 11px;
    cursor: pointer;
    opacity: 0;
    line-height: 1;
  }
  .rail-row-view:hover .rail-pin { opacity: 0.5; }
  .rail-pin.on,
  .rail-row-view:hover .rail-pin.on { opacity: 1; color: #f7c948; }
  .rail-pin:hover:not(:disabled) { color: var(--text); }
  .rail-pin.on:hover:not(:disabled) { color: #ffd966; }
  .rail-row-menu {
    background: transparent;
    border: 0;
    color: var(--text-3);
    padding: 0 8px;
    cursor: pointer;
    opacity: 0;
    font-size: 16px;
    letter-spacing: 1px;
    line-height: 1;
  }
  .rail-row-view:hover .rail-row-menu { opacity: 1; }
  .rail-row-menu:hover:not(:disabled) { color: var(--text); }
  .rail-row-menu[aria-expanded="true"] { opacity: 1; color: var(--text); }
  .rail-row-popover {
    position: absolute;
    top: 100%;
    right: 4px;
    z-index: 20;
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: 6px;
    min-width: 160px;
    padding: 4px;
    display: flex;
    flex-direction: column;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
  }
  .rail-row-popover button {
    background: transparent;
    border: 0;
    color: var(--text-2);
    padding: 6px 10px;
    text-align: left;
    font-size: 12px;
    cursor: pointer;
    border-radius: 4px;
  }
  .rail-row-popover button:hover { background: var(--bg-3); color: var(--text); }
  .rail-row-popover button.danger { color: var(--danger, #ec6b6b); }
  .rail-row-popover button.danger:hover { background: rgba(236, 107, 107, 0.12); }
  .rail-row-popover hr {
    border: 0;
    border-top: 1px solid var(--border);
    margin: 4px 0;
  }
  /* Inline tag rename (v3.43.0 Atlas Tag-Rename) */
  .rail-rename {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 2px 4px;
  }
  .rail-rename-input {
    width: 100%;
    box-sizing: border-box;
    background: var(--bg);
    border: 1px solid var(--accent);
    color: var(--text);
    padding: 4px 7px;
    border-radius: var(--r-sm);
    font-size: 13px;
    outline: 0;
  }
  .rail-rename-input.invalid { border-color: var(--danger, #e54); }
  .rail-rename-error {
    font-size: 11px;
    color: var(--danger, #e54);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .rail-add {
    background: transparent;
    border: 1px dashed var(--border);
    color: var(--text-3);
    padding: 6px 8px;
    border-radius: var(--r-sm);
    font-size: 12px;
    margin-top: 4px;
  }
  .rail-add:hover {
    color: var(--text);
    border-color: var(--border-strong);
  }
  .rail-empty {
    color: var(--text-3);
    font-size: 12px;
    padding: 4px 8px;
    font-style: italic;
  }

  /* Grid */
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: 12px;
  }
  .card {
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    cursor: pointer;
    transition: background 80ms ease, border-color 80ms ease;
  }
  .card:hover {
    background: var(--bg-3);
    border-color: var(--border-strong);
  }
  /* Glass II Vim adapter — currently-focused row gets a distinct ring. */
  .card.vim-focused {
    border-color: var(--accent, #ff7a59);
    box-shadow: 0 0 0 1px var(--accent, #ff7a59) inset;
  }
  /* v3.41.0 Atlas Bulk Tag-Apply — multi-select mode. */
  .card.selectable {
    cursor: default;
  }
  .card.selected {
    border-color: color-mix(in oklab, var(--accent, #ff7a59) 60%, var(--border));
    background: color-mix(in oklab, var(--accent, #ff7a59) 10%, var(--bg-2));
  }
  .card-check {
    flex-shrink: 0;
    width: 16px;
    height: 16px;
    border: 1px solid var(--border-strong);
    border-radius: 4px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-size: 11px;
    line-height: 1;
    color: var(--bg);
    margin-top: 1px;
    transition: background 80ms ease, border-color 80ms ease;
  }
  .card-check.on {
    background: var(--accent, #ff7a59);
    border-color: var(--accent, #ff7a59);
  }
  .card-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 6px;
  }
  .card-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--text);
    line-height: 1.3;
    overflow: hidden;
    text-overflow: ellipsis;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
    word-break: break-word;
  }
  .card-menu {
    background: transparent;
    border: 0;
    color: var(--text-3);
    font-size: 16px;
    line-height: 1;
    padding: 0 4px;
    cursor: pointer;
    opacity: 0;
  }
  .card:hover .card-menu { opacity: 1; }
  .card-menu:hover { color: var(--text); }
  /* v3.55.0 Atlas Doc-Inspector — star + notes-hint glyphs on the card head. */
  .card-star {
    color: #f7c948;
    font-size: 13px;
    line-height: 1;
    flex-shrink: 0;
  }
  .card-notes-hint {
    color: var(--text-3);
    font-size: 12px;
    line-height: 1;
    flex-shrink: 0;
    opacity: 0.75;
  }
  .card:hover .card-notes-hint { opacity: 1; }
  .card-meta {
    font-size: 11px;
    color: var(--text-3);
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .card-sep { color: var(--text-3); opacity: 0.5; }
  .card-path {
    font-size: 11px;
    color: var(--text-3);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }
  .card-tags {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin-top: 2px;
  }
  .chip {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    background: var(--bg);
    color: var(--text-2);
    border: 1px solid var(--border);
    border-left: 2px solid var(--text-3);
    padding: 1px 6px;
    border-radius: 3px;
  }
  .chip.more {
    border-left-color: var(--text-3);
    color: var(--text-3);
  }

  /* OCR queue badges + actions */
  .ocr-badge {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.4px;
    padding: 1px 6px;
    border-radius: 3px;
    border: 1px solid transparent;
    line-height: 1.5;
    white-space: nowrap;
  }
  .ocr-badge-scanned {
    background: rgba(245, 197, 24, 0.12);
    color: #f5c518;
    border-color: rgba(245, 197, 24, 0.35);
  }
  .ocr-badge-mixed {
    background: rgba(192, 132, 252, 0.12);
    color: #c084fc;
    border-color: rgba(192, 132, 252, 0.35);
  }
  .ocr-badge-ocr_pending {
    background: rgba(106, 183, 255, 0.12);
    color: #6ab7ff;
    border-color: rgba(106, 183, 255, 0.35);
  }
  .ocr-badge-ocr_done {
    background: rgba(126, 231, 135, 0.12);
    color: #7ee787;
    border-color: rgba(126, 231, 135, 0.35);
  }
  .ocr-badge-ocr_failed {
    background: rgba(244, 114, 114, 0.12);
    color: #f59292;
    border-color: rgba(244, 114, 114, 0.35);
  }
  .card-actions {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
    margin-top: 2px;
  }
  .card-action {
    background: var(--bg);
    border: 1px solid var(--border);
    color: var(--text-2);
    font-size: 11px;
    padding: 3px 8px;
    border-radius: var(--r-sm);
    cursor: pointer;
    transition: border-color 80ms ease, color 80ms ease;
  }
  .card-action:hover:not(:disabled) {
    color: var(--text);
    border-color: var(--border-strong);
  }
  .card-action:disabled { opacity: 0.55; cursor: progress; }
  .toolbar .ocr-all {
    background: rgba(245, 197, 24, 0.08);
    border-color: rgba(245, 197, 24, 0.35);
    color: #f5c518;
  }
  .toolbar .ocr-all:hover:not(:disabled) {
    border-color: rgba(245, 197, 24, 0.6);
  }
  .toolbar .ocr-all:disabled { opacity: 0.55; }

  /* Status messages (reused look from other panels) */
  .status {
    font-size: 12px;
    padding: 6px 10px;
    border-radius: var(--r-sm);
  }
  .status.ok {
    background: rgba(126, 231, 135, 0.08);
    border: 1px solid rgba(126, 231, 135, 0.3);
    color: #9ae8a1;
  }
  .status.err {
    background: rgba(244, 114, 114, 0.08);
    border: 1px solid rgba(244, 114, 114, 0.3);
    color: #f59292;
  }

  /* Empty state */
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    text-align: center;
    padding: 60px 20px;
    color: var(--text-2);
    gap: 8px;
  }
  .empty-icon {
    font-size: 32px;
    color: var(--accent);
    opacity: 0.8;
    margin-bottom: 4px;
  }
  .empty-title {
    font-size: 16px;
    font-weight: 600;
    color: var(--text);
  }
  .empty-sub {
    font-size: 13px;
    color: var(--text-3);
    max-width: 380px;
    line-height: 1.5;
  }
  .empty .primary {
    margin-top: 8px;
    background: var(--accent);
    border: 1px solid var(--accent);
    color: var(--bg);
    padding: 8px 16px;
    border-radius: var(--r-sm);
    font-size: 13px;
    font-weight: 600;
  }

  /* Context menu */
  .menu {
    position: fixed;
    z-index: 1000;
    background: var(--bg-3);
    border: 1px solid var(--border-strong);
    border-radius: var(--r-sm);
    padding: 4px;
    min-width: 200px;
    max-height: 70vh;
    overflow-y: auto;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
  }
  .menu-item {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    background: transparent;
    border: 0;
    color: var(--text);
    padding: 6px 10px;
    border-radius: 4px;
    font-size: 12px;
    text-align: left;
    cursor: pointer;
  }
  .menu-item:hover { background: var(--bg-2); }
  .menu-item.danger { color: #f59292; }
  .menu-item.danger:hover { background: rgba(244, 114, 114, 0.1); }
  .menu-label { flex: 1; }
  .menu-check { color: var(--accent); font-size: 12px; }
  .menu-section {
    font-size: 9px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--text-3);
    padding: 4px 10px 2px;
  }
  .menu-sep {
    height: 1px;
    background: var(--border);
    margin: 4px 6px;
  }
  .dot.small {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  /* v3.44.0 Atlas Recent-Tags — quick-apply chips at the top of the Tags
     section. A wrapped row of compact pills; clicking one applies that tag. */
  .recent-tags {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    padding: 2px 8px 6px;
  }
  .recent-chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    max-width: 140px;
    background: var(--bg-2);
    border: 1px solid var(--border);
    color: var(--text-2);
    padding: 2px 8px;
    border-radius: 999px;
    font-size: 11px;
    line-height: 1.4;
    cursor: pointer;
  }
  .recent-chip:hover {
    color: var(--text);
    border-color: var(--text-3);
  }
  .recent-chip-label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* Modal */
  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 999;
  }
  .modal {
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 16px 18px;
    min-width: 320px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.5);
  }
  .modal-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--text);
  }
  .modal input {
    background: var(--bg);
    border: 1px solid var(--border);
    color: var(--text);
    padding: 7px 10px;
    border-radius: var(--r-sm);
    font-size: 13px;
    outline: 0;
  }
  .modal input:focus { border-color: var(--accent); }
  .palette {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .swatch {
    width: 22px;
    height: 22px;
    border-radius: 50%;
    border: 2px solid transparent;
    cursor: pointer;
    padding: 0;
  }
  .swatch.active {
    border-color: var(--text);
    transform: scale(1.1);
  }
  .default-swatch {
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--bg-3);
    border: 2px solid var(--border);
    color: var(--text-3);
    font-size: 13px;
    line-height: 1;
  }
  .default-swatch.active {
    border-color: var(--text);
    color: var(--text);
  }
  .color-preview {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 2px 0;
  }
  .color-preview .dot.preview {
    width: 14px;
    height: 14px;
    border-radius: 50%;
    flex-shrink: 0;
  }
  .color-preview-name {
    font-size: 13px;
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .modal-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  .modal-actions .ghost,
  .modal-actions .primary {
    padding: 6px 14px;
    border-radius: var(--r-sm);
    font-size: 12px;
  }
  .modal-actions .ghost {
    background: var(--bg-3);
    border: 1px solid var(--border);
    color: var(--text);
  }
  .modal-actions .primary {
    background: var(--accent);
    border: 1px solid var(--accent);
    color: var(--bg);
    font-weight: 600;
  }
  .modal-actions .primary:disabled { opacity: 0.5; }

  /* v3.45.0 Atlas Tag-Merge — merge modal */
  .merge-lead {
    font-size: 12px;
    line-height: 1.5;
    color: var(--text-2);
  }
  .merge-source {
    color: var(--text);
    font-weight: 600;
  }
  .merge-empty {
    font-size: 12px;
    color: var(--text-3);
    padding: 8px 0;
  }
  .merge-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-height: 240px;
    overflow-y: auto;
    margin: 2px 0;
  }
  .merge-option {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    background: transparent;
    border: 1px solid transparent;
    color: var(--text-2);
    padding: 7px 9px;
    border-radius: var(--r-sm);
    font-size: 13px;
    text-align: left;
    cursor: pointer;
  }
  .merge-option:hover { background: var(--bg-3); color: var(--text); }
  .merge-option.active {
    background: var(--bg-3);
    border-color: var(--accent);
    color: var(--text);
  }
  .merge-option .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex-shrink: 0;
  }
  .merge-option-name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .merge-check { color: var(--accent); font-size: 12px; }
  .merge-error {
    font-size: 11px;
    color: var(--danger, #e54);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* v3.51.0 Atlas Tag-Descriptions — notes modal + has-notes affordance */
  .desc-lead {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    color: var(--text);
  }
  .desc-lead .dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    flex-shrink: 0;
  }
  .desc-name {
    color: var(--text);
    font-weight: 600;
  }
  .desc-textarea {
    width: 100%;
    box-sizing: border-box;
    font-family: inherit;
    font-size: 13px;
    line-height: 1.5;
    color: var(--text);
    background: var(--bg-3);
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    padding: 8px 10px;
    resize: vertical;
    min-height: 84px;
  }
  .desc-textarea:focus {
    outline: none;
    border-color: var(--accent);
  }
  .desc-textarea.invalid { border-color: var(--danger, #e54); }
  .desc-textarea::placeholder { color: var(--text-3); }
  .desc-meta {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 11px;
    color: var(--text-3);
    margin-top: -2px;
  }
  .desc-count.near-limit { color: var(--danger, #e54); }
  .desc-hint { font-variant: small-caps; letter-spacing: 0.04em; }
  .desc-error {
    font-size: 11px;
    color: var(--danger, #e54);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  /* A muted accent on the notes glyph when the tag actually carries a note,
     so a glance at the rail tells you which tags have lore behind them. */
  .rail-row-x.has-notes { color: var(--text-2); }
  .rail-row-x.has-notes:hover { color: var(--accent); }

  /* v3.41.0 Atlas Bulk Tag-Apply — floating action bar + tag picker. */
  .bulk-bar {
    position: fixed;
    left: 50%;
    bottom: 22px;
    transform: translateX(-50%);
    z-index: 60;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 10px 8px 12px;
    background: var(--bg-2);
    border: 1px solid var(--border-strong);
    border-radius: var(--r-md, 10px);
    box-shadow: 0 8px 28px rgba(0, 0, 0, 0.4);
  }
  .bulk-select-all {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    background: transparent;
    border: 1px solid var(--border);
    color: var(--text-2);
    padding: 5px 10px;
    border-radius: var(--r-sm);
    font-size: 12px;
    cursor: pointer;
  }
  .bulk-select-all:hover { color: var(--text); border-color: var(--border-strong); }
  .bulk-select-all .glyph { font-size: 13px; line-height: 1; }
  .bulk-count {
    font-size: 12px;
    color: var(--text-2);
    font-variant-numeric: tabular-nums;
    min-width: 78px;
  }
  .bulk-actions { display: flex; align-items: center; gap: 8px; }
  .bulk-tag-wrap { position: relative; }
  /* v3.53.0 Atlas Collections — Slice 27 bulk add-to-collection */
  .bulk-coll-wrap { position: relative; }
  .bulk-picker-empty {
    padding: 10px 12px;
    color: var(--text-3);
    font-size: 12px;
    font-style: italic;
  }
  .bulk-picker-count {
    margin-left: auto;
    padding-left: 8px;
    color: var(--text-3);
    font-size: 11px;
    font-variant-numeric: tabular-nums;
  }
  .bulk-apply {
    background: var(--accent);
    border: 1px solid var(--accent);
    color: var(--bg);
    padding: 6px 14px;
    border-radius: var(--r-sm);
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
  }
  .bulk-apply:hover:not(:disabled) { background: var(--accent-2, var(--accent)); }
  .bulk-apply:disabled { opacity: 0.5; cursor: default; }
  .bulk-clear {
    background: var(--bg-3);
    border: 1px solid var(--border);
    color: var(--text-2);
    padding: 6px 12px;
    border-radius: var(--r-sm);
    font-size: 13px;
    cursor: pointer;
  }
  .bulk-clear:hover:not(:disabled) { color: var(--text); border-color: var(--border-strong); }
  .bulk-clear:disabled { opacity: 0.5; cursor: default; }

  .bulk-picker {
    position: absolute;
    bottom: calc(100% + 8px);
    left: 0;
    width: 250px;
    max-height: 320px;
    overflow-y: auto;
    background: var(--bg-2);
    border: 1px solid var(--border-strong);
    border-radius: var(--r-sm);
    box-shadow: 0 8px 28px rgba(0, 0, 0, 0.4);
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .bulk-picker-section {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--text-3);
    padding: 2px 2px 0;
  }
  .bulk-new-tag { display: flex; gap: 6px; }
  .bulk-new-tag input {
    flex: 1;
    min-width: 0;
    background: var(--bg-3);
    border: 1px solid var(--border);
    color: var(--text);
    border-radius: var(--r-sm);
    padding: 6px 8px;
    font-size: 12px;
    outline: 0;
  }
  .bulk-new-tag input:focus { border-color: var(--border-strong); }
  .bulk-new-go {
    background: var(--bg-3);
    border: 1px solid var(--border);
    color: var(--text);
    border-radius: var(--r-sm);
    padding: 6px 10px;
    font-size: 12px;
    cursor: pointer;
  }
  .bulk-new-go:hover:not(:disabled) { border-color: var(--border-strong); }
  .bulk-new-go:disabled { opacity: 0.5; cursor: default; }
  .bulk-picker-list {
    display: flex;
    flex-direction: column;
    gap: 1px;
    border-top: 1px solid var(--border);
    padding-top: 6px;
  }
  .bulk-picker-row {
    display: flex;
    align-items: stretch;
    border-radius: var(--r-sm);
  }
  .bulk-picker-row:hover { background: var(--bg-3); }
  .bulk-picker-tag {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 8px;
    background: transparent;
    border: 0;
    color: var(--text-2);
    padding: 6px 8px;
    border-radius: var(--r-sm);
    font-size: 13px;
    text-align: left;
    min-width: 0;
    cursor: pointer;
  }
  .bulk-picker-tag:hover:not(:disabled) { color: var(--text); }
  .bulk-picker-tag:disabled { opacity: 0.5; cursor: default; }
  .bulk-picker-tag .dot.small {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex-shrink: 0;
  }
  .bulk-picker-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .bulk-picker-x {
    background: transparent;
    border: 0;
    color: var(--text-3);
    padding: 0 10px;
    font-size: 16px;
    cursor: pointer;
    opacity: 0;
  }
  .bulk-picker-row:hover .bulk-picker-x { opacity: 1; }
  .bulk-picker-x:hover:not(:disabled) { color: var(--accent); }
  .bulk-picker-x:disabled { opacity: 0; }

  .bulk-toast {
    position: fixed;
    left: 50%;
    bottom: 78px;
    transform: translateX(-50%);
    z-index: 59;
    background: var(--bg-3);
    border: 1px solid var(--border-strong);
    border-radius: var(--r-sm);
    padding: 7px 14px;
    font-size: 12px;
    color: var(--text);
    box-shadow: 0 6px 20px rgba(0, 0, 0, 0.35);
  }
</style>
