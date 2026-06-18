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
    setTagColor,
    renameTag,
    type AutoTagRunResult,
    type DocumentRecord,
    type FilterClause,
    type FilterCombinator,
    type FolderRecord,
    type LibraryFilter,
    type LibrarySortBy,
    type OcrQueueResult,
    type OcrState,
    type TagRecord,
  } from "$lib/library";
  import { basename } from "$lib/types";
  import { formatRelTime } from "$lib/recent";
  import { registerLibraryNav } from "$lib/vim/library-adapter";
  import CollectionsSidebar from "$lib/panels/CollectionsSidebar.svelte";
  import SuggestedTagsRow from "$lib/panels/SuggestedTagsRow.svelte";

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
  let query = $state("");
  let sort = $state<LibrarySortBy>("added_desc");
  // v3.40.0 Atlas Untagged-Filter — when on, restrict the grid to docs
  // that carry no tags (the "cleanup queue"). Composes with the active
  // folder / tag / search filters via the clause tree.
  let untaggedOnly = $state(false);
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
  }

  // ---------- Data loaders ----------

  async function refreshAll() {
    loading = true;
    error = null;
    try {
      const [f, t] = await Promise.all([listFolders(), listTags()]);
      folders = f;
      tags = t;
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
      filter = { sort, clauses: { combinator, clauses } };
    } else {
      filter = {
        folder_id: folderId,
        tag_ids: Array.from(activeTagIds),
        title_substring: title,
        sort,
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
    sort;
    untaggedOnly;
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

  function openMenuFor(e: MouseEvent, doc: DocumentRecord) {
    e.preventDefault();
    e.stopPropagation();
    menu = { doc, x: e.clientX, y: e.clientY, submenu: null };
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
      class="untagged-toggle select-toggle"
      class:active={selecting}
      onclick={toggleSelectMode}
      aria-pressed={selecting}
      title="Select multiple documents to tag them all at once"
    >
      <span class="glyph" aria-hidden="true">&#x2611;</span>
      {selecting ? "Done" : "Select"}
    </button>
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
          <span class="rail-title">Tags</span>
          <span class="rail-count">{tags.length}</span>
        </div>
        {#each tags as t (t.id)}
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
                onclick={() => toggleTag(t)}
              >
                <span class="rail-icon dot" style:background={t.color ?? "var(--text-3)"}></span>
                <span class="rail-label">{t.name}</span>
              </button>
              <button
                class="rail-row-x"
                title="Rename tag"
                aria-label="Rename tag"
                onclick={() => startRenameTag(t)}
              >&#9998;</button>
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
                <div class="card-title" title={d.path}>{displayTitle(d)}</div>
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
