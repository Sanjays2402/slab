<!--
  Doc Inspector Panel (v3.55.0 "Atlas Doc-Inspector" slice 37).

  A slide-from-right drawer that turns a library document row from an
  opaque cell on the grid into a full editable surface:

    - title override (the LibraryPanel card falls back to the basename
      when this is null; the editor lets the user give `scan_001.pdf`
      a real label without renaming the file on disk)
    - star toggle (mirrors the toolbar Starred filter)
    - freeform notes (textarea, cap is enforced server-side at 4000
      Unicode scalars; the drawer surfaces the count as it grows)
    - searchable inline tag editor with optimistic add/remove + rollback
    - metadata block (path / pages / size / added / last-seen /
      ocr-state, all read-only)
    - Open in Reader + Reveal on disk actions
    - Delete-from-library action (danger-styled, with confirm)

  Unlike OcrQueuePanel / BeaconCachePanel (which are FULL modals
  taking the whole viewport), the inspector is a 420px-wide drawer
  pinned to the right edge so the underlying doc grid stays visible.
  Click-outside or Escape closes; the surface auto-saves title/notes
  on blur (or Cmd/Ctrl-Enter) so changes never get lost on accidental
  dismiss.

  Two callback props:
    - onUpdate(updated) — called for confirmed backend rows.
    - onPatch(docId, patch) — applies field-scoped optimistic changes and
      rollbacks against the parent's freshest row.
    - onRemove(docId) — called when the user confirms delete. The
      parent prunes the row and the drawer closes.

  Reuses the existing document setters and the atomic setDocumentTag
  command so a stale drawer never replaces unrelated tag links.
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import {
    type DocumentRecord,
    type TagRecord,
    setDocumentTitle,
    setDocumentNotes,
    setDocumentStarred,
    setDocumentTag,
    removeDocument,
  } from "$lib/library";
  import {
    filterInspectorTagOptions,
    planInspectorTagAssignment,
    planInspectorTagMutation,
    planInspectorTitleMutation,
    rollbackInspectorTagMutation,
  } from "$lib/docInspectorView";
  import { notify } from "$lib/notify";
  import { basename, type CmdResult } from "$lib/types";
  import { formatRelTime } from "$lib/recent";

  type Props = {
    /** The document to inspect. `null` closes the drawer. The component
     * snapshots this on open and edits its own internal copy; the
     * parent should call onUpdate to keep its `docs` array in sync. */
    doc: DocumentRecord | null;
    /** Library-wide tags available for the inline add picker. */
    availableTags?: TagRecord[];
    /** Called when the inspector successfully writes a fresh row. */
    onUpdate?: (updated: DocumentRecord) => void;
    /** Applies a field-scoped optimistic patch or rollback to the freshest
     * parent-owned row and returns the result. */
    onPatch?: (
      docId: number,
      patch: (current: DocumentRecord) => DocumentRecord,
    ) => DocumentRecord | null;
    /** Called when the user confirms delete; the parent removes the
     * row from its grid and closes the drawer. */
    onRemove?: (docId: number) => void;
    /** Called for Escape/backdrop click/Close button. The parent sets
     * its inspector state to null. */
    onClose?: () => void;
  };

  let {
    doc,
    availableTags = [],
    onUpdate = () => {},
    onPatch = () => null,
    onRemove = () => {},
    onClose = () => {},
  }: Props = $props();

  // Local mirror of `doc` so the form fields are unconditionally
  // bindable strings. Re-syncs every time `doc` changes (i.e. the
  // parent reassigns inspectorDoc to a different row).
  let title = $state("");
  let notes = $state("");
  let starred = $state(false);
  let busyTitle = $state(false);
  let busyNotes = $state(false);
  let busyStar = $state(false);
  let busyTags = $state(false);
  let busyDelete = $state(false);
  let busyClose = $state(false);
  let busyReveal = $state(false);
  const mutationBusy = $derived(
    busyTitle ||
      busyNotes ||
      busyStar ||
      busyTags ||
      busyDelete ||
      busyClose ||
      busyReveal,
  );
  const interactionLocked = $derived(busyDelete || busyClose);
  let tagQuery = $state("");
  let error = $state<string | null>(null);
  let okToast = $state<string | null>(null);
  let confirmDelete = $state(false);
  let syncedTitle = $state("");
  let syncedNotes = $state("");
  let mutationTail: Promise<void> = Promise.resolve();
  let mutationFailureVersion = 0;

  // Maximum notes length — kept in sync with the backend constant in
  // src-tauri/src/pdf/library/registry.rs::MAX_DOC_NOTES_LEN.
  const NOTES_MAX = 4000;
  const TITLE_MAX = 500;

  // Reset the form state whenever a different doc lands. Without this,
  // the user would see stale fields if they inspect doc A, edit a bit,
  // then jump to doc B from the grid menu.
  let currentDocId = $state<number | null>(null);
  $effect(() => {
    if (doc) {
      const nextTitle = doc.title ?? "";
      const nextNotes = doc.notes ?? "";
      if (doc.id !== currentDocId) {
        currentDocId = doc.id;
        title = nextTitle;
        notes = nextNotes;
        syncedTitle = nextTitle;
        syncedNotes = nextNotes;
        starred = doc.starred;
        error = null;
        okToast = null;
        confirmDelete = false;
        tagQuery = "";
      } else {
        const titleWasClean = title === syncedTitle;
        const notesWereClean = notes === syncedNotes;
        syncedTitle = nextTitle;
        syncedNotes = nextNotes;
        if (titleWasClean && !busyTitle) title = nextTitle;
        if (notesWereClean && !busyNotes) notes = nextNotes;
        if (!busyStar) starred = doc.starred;
      }
    } else if (!doc) {
      currentDocId = null;
    }
  });

  // Close on Escape from anywhere inside the drawer (form fields too).
  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      void requestClose();
    }
  }

  // Listen to library-changed so a backend mutation from elsewhere
  // (auto-tag, bulk action, scanner re-upsert) refreshes the doc
  // we're showing. Avoids stale display if the user runs OCR from
  // the card menu while the inspector is open.
  let unlisten: UnlistenFn | null = null;
  $effect(() => {
    if (!doc) {
      if (unlisten) {
        unlisten();
        unlisten = null;
      }
      return;
    }
    void (async () => {
      try {
        unlisten = await listen("slab://library-changed", () => {
          // Don't auto-rebind doc — that would clobber unsaved edits.
          // Just clear any toast so it doesn't go stale.
          okToast = null;
        });
      } catch (e) {
        // Browser-mode (no Tauri) — silently skip the listener.
        unlisten = null;
      }
    })();
    return () => {
      if (unlisten) {
        unlisten();
        unlisten = null;
      }
    };
  });

  // -------- save handlers --------

  function enqueueMutation(run: () => Promise<void>): Promise<void> {
    const scheduled = mutationTail.then(run, run);
    mutationTail = scheduled.catch(() => {});
    return scheduled;
  }

  function patchDocument(
    docId: number,
    patch: (current: DocumentRecord) => DocumentRecord,
  ): DocumentRecord | null {
    const patched = onPatch(docId, patch);
    if (patched) return patched;
    if (doc?.id !== docId) return null;
    const fallback = patch(doc);
    onUpdate(fallback);
    return fallback;
  }

  async function saveTitleIfChanged() {
    const source = doc;
    if (!source || busyTitle || busyDelete) return;
    const mutation = planInspectorTitleMutation(source, title);
    if (!mutation) return;
    if (title.trim().length > TITLE_MAX) {
      error = `Title too long (max ${TITLE_MAX} chars).`;
      mutationFailureVersion++;
      return;
    }
    busyTitle = true;
    const docId = mutation.before.id;
    await enqueueMutation(async () => {
      let previousTitle = mutation.before.title;
      error = null;
      okToast = null;
      patchDocument(docId, (current) => {
        previousTitle = current.title;
        return current.title === mutation.title
          ? current
          : { ...current, title: mutation.title };
      });
      try {
        const updated = await setDocumentTitle(docId, mutation.title);
        onUpdate(updated);
        if (doc?.id === docId) {
          title = updated.title ?? "";
          okToast = "Title saved";
        }
      } catch (e) {
        const rollback = patchDocument(docId, (current) =>
          current.title === mutation.title
            ? { ...current, title: previousTitle }
            : current,
        );
        const detail = String(e);
        mutationFailureVersion++;
        if (doc?.id === docId) {
          title = rollback?.title ?? previousTitle ?? "";
          error = `Title change rolled back: ${detail}`;
        }
        notify.error("Title change rolled back", { detail });
      } finally {
        busyTitle = false;
      }
    });
  }

  async function saveNotesIfChanged() {
    const source = doc;
    if (!source || busyNotes || busyDelete) return;
    const docId = source.id;
    const trimmed = notes;
    const current = source.notes ?? "";
    // Trim-equality so an extra trailing newline isn't a "change".
    if (trimmed.trim() === current.trim()) return;
    if (trimmed.length > NOTES_MAX) {
      error = `Notes too long (max ${NOTES_MAX} chars).`;
      mutationFailureVersion++;
      return;
    }
    busyNotes = true;
    await enqueueMutation(async () => {
      error = null;
      try {
        const updated = await setDocumentNotes(docId, trimmed || null);
        onUpdate(updated);
        if (doc?.id === docId) {
          notes = updated.notes ?? "";
          okToast = "Notes saved";
        }
      } catch (e) {
        mutationFailureVersion++;
        if (doc?.id === docId) error = `Notes save failed: ${String(e)}`;
      } finally {
        busyNotes = false;
      }
    });
  }

  async function toggleStar() {
    const source = doc;
    if (!source || busyStar || busyDelete) return;
    const docId = source.id;
    busyStar = true;
    const next = !starred;
    await enqueueMutation(async () => {
      error = null;
      try {
        const updated = await setDocumentStarred(docId, next);
        onUpdate(updated);
        if (doc?.id === docId) starred = updated.starred;
      } catch (e) {
        mutationFailureVersion++;
        if (doc?.id === docId) error = `Star failed: ${String(e)}`;
      } finally {
        busyStar = false;
      }
    });
  }

  async function toggleTag(tag: TagRecord) {
    const source = doc;
    if (!source || busyTags || busyDelete) return;
    const mutation = planInspectorTagMutation(source, tag);
    const docId = mutation.before.id;
    busyTags = true;
    await enqueueMutation(async () => {
      let activeMutation = mutation;
      error = null;
      okToast = null;
      patchDocument(docId, (current) => {
        activeMutation = planInspectorTagAssignment(
          current,
          tag,
          mutation.attached,
        );
        return activeMutation.optimistic;
      });
      try {
        const updated = await setDocumentTag(docId, tag.id, mutation.attached);
        onUpdate(updated);
        if (doc?.id === docId) {
          if (mutation.attached) tagQuery = "";
          okToast = mutation.attached ? `Added “${tag.name}”` : `Removed “${tag.name}”`;
        }
      } catch (e) {
        patchDocument(docId, (current) =>
          rollbackInspectorTagMutation(current, activeMutation),
        );
        const detail = String(e);
        mutationFailureVersion++;
        if (doc?.id === docId) {
          error = `Tag change rolled back: ${detail}`;
        }
        notify.error("Tag change rolled back", { detail });
      } finally {
        busyTags = false;
      }
    });
  }

  async function onConfirmDelete() {
    if (!doc || mutationBusy) return;
    busyDelete = true;
    error = null;
    try {
      await removeDocument(doc.id);
      const removedId = doc.id;
      busyDelete = false;
      onRemove(removedId);
    } catch (e) {
      error = `Delete failed: ${String(e)}`;
      busyDelete = false;
    }
  }

  // Reveal in Finder / Explorer — uses the cross-platform Tauri shell
  // open command. Fails silently with a toast if the path vanished.
  async function reveal() {
    const source = doc;
    if (!source || busyReveal || interactionLocked) return;
    busyReveal = true;
    await enqueueMutation(async () => {
      try {
        // Try the platform-specific reveal first via our own command if
        // present; fall back to opening the path in the default opener.
        const res = await invoke<CmdResult<null>>("slab_reveal_in_finder", {
          path: source.path,
        }).catch(() => ({ kind: "err", message: "no reveal command" }) as CmdResult<null>);
        if (res.kind === "ok") return;
        // Fallback: open the parent folder.
        const idx = source.path.lastIndexOf("/");
        const parent = idx >= 0 ? source.path.slice(0, idx) : source.path;
        await invoke("slab_open_external", { path: parent });
      } catch (e) {
        if (doc?.id === source.id) error = `Reveal failed: ${String(e)}`;
      } finally {
        busyReveal = false;
      }
    });
  }

  async function runAfterDraftSaves(action: () => void | Promise<void>) {
    if (busyClose || busyDelete) return;
    busyClose = true;
    const failureVersion = mutationFailureVersion;
    try {
      await Promise.all([saveTitleIfChanged(), saveNotesIfChanged()]);
      await mutationTail;
      if (mutationFailureVersion === failureVersion) await action();
    } finally {
      busyClose = false;
    }
  }

  async function requestClose() {
    await runAfterDraftSaves(onClose);
  }

  async function openInReader() {
    const source = doc;
    if (!source || busyClose || busyDelete) return;
    await runAfterDraftSaves(async () => {
      // Same path the LibraryPanel uses — we dispatch the request the
      // App routes to a Reader tab.
      await invoke("slab_request_open_in_main", { path: source.path }).catch(() => {
        // Browser mode — no-op; nothing else to do.
      });
      onClose();
    });
  }

  // -------- derived display --------

  const sizeMb = $derived(() => {
    if (!doc) return "";
    if (doc.size_bytes < 1024) return `${doc.size_bytes} B`;
    if (doc.size_bytes < 1024 * 1024)
      return `${(doc.size_bytes / 1024).toFixed(1)} KB`;
    return `${(doc.size_bytes / (1024 * 1024)).toFixed(2)} MB`;
  });

  const basenameTitle = $derived(doc ? basename(doc.path) : "");
  const titlePlaceholder = $derived(
    doc ? `Falls back to filename: ${basenameTitle}` : "",
  );
  const notesCount = $derived(notes.length);
  const notesNearMax = $derived(notesCount > NOTES_MAX * 0.9);
  const notesOver = $derived(notesCount > NOTES_MAX);
  const matchingTagOptions = $derived(
    filterInspectorTagOptions(availableTags, doc?.tags ?? [], tagQuery),
  );
  const visibleTagOptions = $derived(matchingTagOptions.slice(0, 8));

  function ocrLabel(state: string): string {
    switch (state) {
      case "text_native":
        return "Text-native (no OCR needed)";
      case "scanned":
        return "Scanned (OCR pending)";
      case "mixed":
        return "Mixed text + scans";
      case "ocr_pending":
        return "OCR running";
      case "ocr_done":
        return "OCR'd";
      case "ocr_failed":
        return "OCR failed";
      case "unknown":
      default:
        return "Unknown";
    }
  }
</script>

{#if doc}
  <div
    class="di-backdrop"
    role="dialog"
    aria-modal="false"
    aria-label="Document inspector"
    onclick={(e) => {
      if (e.target === e.currentTarget) void requestClose();
    }}
    onkeydown={onKey}
    tabindex="-1"
  >
    <aside class="di-drawer" role="document" aria-label="Document details">
      <header class="di-head">
        <div class="di-head-title">
          <button
            class="di-star"
            class:on={starred}
            onclick={toggleStar}
            disabled={busyStar || interactionLocked}
            aria-pressed={starred}
            title={starred ? "Unstar this document" : "Star this document"}
            aria-label={starred ? "Unstar" : "Star"}
          >★</button>
          <div class="di-title-block">
            <h2 class="di-name" title={doc.path}>
              {doc.title ?? basenameTitle}
            </h2>
            <div class="di-filename">{basenameTitle}</div>
          </div>
        </div>
        <button
          class="di-close"
          onclick={requestClose}
          disabled={interactionLocked}
          aria-label="Close inspector"
          title="Close (Esc)"
        >✕</button>
      </header>

      {#if error}
        <div class="di-err" role="alert">{error}</div>
      {/if}
      {#if okToast && !error}
        <div class="di-ok" role="status">✓ {okToast}</div>
      {/if}

      <section class="di-section">
        <label class="di-field">
          <span class="di-label">Title override</span>
          <input
            type="text"
            class="di-input"
            bind:value={title}
            placeholder={titlePlaceholder}
            maxlength={TITLE_MAX + 1}
            disabled={busyTitle || interactionLocked}
            onblur={saveTitleIfChanged}
            onkeydown={(e) => {
              if (e.key === "Enter") {
                (e.currentTarget as HTMLInputElement).blur();
              }
            }}
          />
          {#if title.trim() === "" && doc.title}
            <span class="di-hint">Clear to fall back to filename.</span>
          {:else if title.length > TITLE_MAX}
            <span class="di-hint err">{title.length} / {TITLE_MAX} chars — too long</span>
          {/if}
        </label>
      </section>

      <section class="di-section">
        <label class="di-field">
          <span class="di-label">Notes</span>
          <textarea
            class="di-textarea"
            bind:value={notes}
            placeholder="Provenance, follow-ups, context. Saved on blur or ⌘↵."
            rows={6}
            disabled={busyNotes || interactionLocked}
            onblur={saveNotesIfChanged}
            onkeydown={(e) => {
              if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
                e.preventDefault();
                void saveNotesIfChanged();
              }
            }}
          ></textarea>
          <div class="di-counter" class:near={notesNearMax} class:over={notesOver}>
            {notesCount} / {NOTES_MAX}
          </div>
        </label>
      </section>

      <section class="di-section">
        <div class="di-section-head">
          <div class="di-label">Tags</div>
          <span class="di-count">{doc.tags.length} attached</span>
        </div>
        {#if doc.tags.length > 0}
          <div class="di-tags">
            {#each doc.tags as t (t.id)}
              <button
                type="button"
                class="di-tag"
                style:background={t.color ?? "var(--surface-2)"}
                title={t.description ?? t.name}
                aria-label={`Remove tag ${t.name}`}
                disabled={busyTags || interactionLocked}
                onclick={() => toggleTag(t)}
              >
                <span>{t.name}</span>
                <span class="di-tag-remove" aria-hidden="true">×</span>
              </button>
            {/each}
          </div>
        {:else}
          <div class="di-tag-empty">No tags attached yet.</div>
        {/if}

        <input
          type="search"
          class="di-input di-tag-search"
          bind:value={tagQuery}
          placeholder="Find a tag to add…"
          aria-label="Find a tag to add"
          disabled={busyTags || interactionLocked || availableTags.length === 0}
          onkeydown={(e) => {
            if (e.key === "Escape" && tagQuery) {
              e.stopPropagation();
              tagQuery = "";
            }
          }}
        />

        {#if visibleTagOptions.length > 0}
          <div class="di-tag-options" role="group" aria-label="Available tags">
            {#each visibleTagOptions as option (option.tag.id)}
              <button
                type="button"
                class="di-tag-option"
                title={option.tag.description ?? `Add ${option.tag.name}`}
                aria-label={`Add tag ${option.tag.name}`}
                disabled={busyTags || interactionLocked}
                onclick={() => toggleTag(option.tag)}
              >
                <span
                  class="di-tag-dot"
                  style:background={option.tag.color ?? "var(--text-3)"}
                  aria-hidden="true"
                ></span>
                <span class="di-tag-option-name">
                  {#each option.segments as segment}
                    {#if segment.hit}<mark>{segment.text}</mark>{:else}{segment.text}{/if}
                  {/each}
                </span>
                <span class="di-tag-add" aria-hidden="true">+</span>
              </button>
            {/each}
          </div>
          {#if matchingTagOptions.length > visibleTagOptions.length}
            <div class="di-hint">
              Showing {visibleTagOptions.length} of {matchingTagOptions.length}; keep typing to narrow.
            </div>
          {/if}
        {:else if availableTags.length === 0}
          <div class="di-hint">Create a tag from the Library rail, then attach it here.</div>
        {:else if tagQuery.trim()}
          <div class="di-hint">No available tags match “{tagQuery.trim()}”.</div>
        {:else}
          <div class="di-hint">All available tags are attached.</div>
        {/if}
      </section>

      <section class="di-section di-meta">
        <div class="di-label">Details</div>
        <dl class="di-meta-list">
          <div class="di-meta-row">
            <dt>Path</dt>
            <dd class="di-mono" title={doc.path}>{doc.path}</dd>
          </div>
          <div class="di-meta-row">
            <dt>Pages</dt>
            <dd>{doc.pages ?? "?"}</dd>
          </div>
          <div class="di-meta-row">
            <dt>Size</dt>
            <dd>{sizeMb()}</dd>
          </div>
          <div class="di-meta-row">
            <dt>Added</dt>
            <dd>{formatRelTime(doc.added_at * 1000)}</dd>
          </div>
          <div class="di-meta-row">
            <dt>Last seen</dt>
            <dd>{formatRelTime(doc.last_seen_at * 1000)}</dd>
          </div>
          <div class="di-meta-row">
            <dt>OCR state</dt>
            <dd>
              {ocrLabel(doc.ocr_state)}
              {#if doc.ocr_error}
                <div class="di-ocr-err" title={doc.ocr_error}>
                  {doc.ocr_error}
                </div>
              {/if}
            </dd>
          </div>
        </dl>
      </section>

      <footer class="di-foot">
        <button class="di-btn primary" onclick={openInReader} disabled={interactionLocked}>
          Open in Reader
        </button>
        <button
          class="di-btn ghost"
          onclick={reveal}
          disabled={busyReveal || interactionLocked}
        >
          Reveal on disk
        </button>
        <span class="di-foot-spacer"></span>
        {#if confirmDelete}
          <button
            class="di-btn danger"
            onclick={onConfirmDelete}
            disabled={mutationBusy}
          >
            {busyDelete ? "Removing…" : "Confirm remove"}
          </button>
          <button
            class="di-btn ghost"
            onclick={() => (confirmDelete = false)}
            disabled={mutationBusy}
          >
            Cancel
          </button>
        {:else}
          <button
            class="di-btn ghost danger-ghost"
            onclick={() => (confirmDelete = true)}
            disabled={interactionLocked}
            title="Remove from library (file on disk is untouched)"
          >
            Remove from library
          </button>
        {/if}
      </footer>
    </aside>
  </div>
{/if}

<style>
  .di-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.32);
    display: flex;
    justify-content: flex-end;
    z-index: 90;
    animation: di-fade 140ms ease-out;
  }
  @keyframes di-fade {
    from {
      background: rgba(0, 0, 0, 0);
    }
    to {
      background: rgba(0, 0, 0, 0.32);
    }
  }
  .di-drawer {
    width: 460px;
    max-width: 92vw;
    height: 100vh;
    background: var(--bg-1);
    border-left: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    overflow-y: auto;
    box-shadow: -8px 0 32px rgba(0, 0, 0, 0.28);
    animation: di-slide 180ms cubic-bezier(0.16, 1, 0.3, 1);
  }
  @keyframes di-slide {
    from {
      transform: translateX(40px);
      opacity: 0;
    }
    to {
      transform: translateX(0);
      opacity: 1;
    }
  }
  .di-head {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 12px;
    padding: 16px 18px 12px;
    border-bottom: 1px solid var(--border);
    position: sticky;
    top: 0;
    background: var(--bg-1);
    z-index: 1;
  }
  .di-head-title {
    display: flex;
    gap: 10px;
    align-items: flex-start;
    flex: 1;
    min-width: 0;
  }
  .di-star {
    background: transparent;
    border: 1px solid var(--border);
    color: var(--text-3);
    width: 30px;
    height: 30px;
    border-radius: 6px;
    font-size: 16px;
    line-height: 1;
    cursor: pointer;
    flex-shrink: 0;
    transition: color 100ms ease, background 100ms ease, border-color 100ms ease;
  }
  .di-star:hover {
    background: var(--surface-2);
    color: var(--text-1);
  }
  .di-star.on {
    color: #f7c948;
    border-color: #b78a18;
    background: rgba(247, 201, 72, 0.08);
  }
  .di-title-block {
    flex: 1;
    min-width: 0;
  }
  .di-name {
    font-size: 15px;
    font-weight: 600;
    margin: 0 0 2px 0;
    color: var(--text-1);
    line-height: 1.3;
    word-break: break-word;
  }
  .di-filename {
    font-size: 11px;
    color: var(--text-3);
    font-family: var(--mono, ui-monospace, monospace);
    word-break: break-all;
  }
  .di-close {
    background: transparent;
    border: none;
    color: var(--text-3);
    font-size: 14px;
    cursor: pointer;
    width: 28px;
    height: 28px;
    border-radius: 6px;
    flex-shrink: 0;
    transition: background 100ms ease, color 100ms ease;
  }
  .di-close:hover {
    background: var(--surface-2);
    color: var(--text-1);
  }
  .di-close:disabled {
    cursor: wait;
    opacity: 0.5;
  }
  .di-err {
    margin: 10px 18px 0;
    padding: 8px 12px;
    background: rgba(239, 68, 68, 0.08);
    border: 1px solid rgba(239, 68, 68, 0.2);
    border-radius: var(--r-sm);
    color: #ef4444;
    font-size: 12px;
  }
  .di-ok {
    margin: 10px 18px 0;
    padding: 6px 12px;
    background: rgba(34, 197, 94, 0.08);
    border: 1px solid rgba(34, 197, 94, 0.2);
    border-radius: var(--r-sm);
    color: #22c55e;
    font-size: 12px;
  }
  .di-section {
    padding: 14px 18px;
    border-bottom: 1px solid var(--border);
  }
  .di-field {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .di-label {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-3);
    font-weight: 600;
  }
  .di-input {
    background: var(--surface-2);
    border: 1px solid var(--border);
    color: var(--text-1);
    padding: 8px 10px;
    border-radius: var(--r-sm);
    font-size: 13px;
    box-sizing: border-box;
    width: 100%;
    transition: border-color 100ms ease;
  }
  .di-input:focus {
    outline: none;
    border-color: var(--accent);
  }
  .di-textarea {
    background: var(--surface-2);
    border: 1px solid var(--border);
    color: var(--text-1);
    padding: 8px 10px;
    border-radius: var(--r-sm);
    font-size: 13px;
    font-family: inherit;
    line-height: 1.5;
    resize: vertical;
    min-height: 100px;
    transition: border-color 100ms ease;
  }
  .di-textarea:focus {
    outline: none;
    border-color: var(--accent);
  }
  .di-counter {
    align-self: flex-end;
    font-size: 11px;
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
  }
  .di-counter.near {
    color: #f59e0b;
  }
  .di-counter.over {
    color: #ef4444;
  }
  .di-hint {
    font-size: 11px;
    color: var(--text-3);
  }
  .di-hint.err {
    color: #ef4444;
  }
  .di-tags {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin: 4px 0 8px;
  }
  .di-tag {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 11px;
    padding: 2px 8px;
    border-radius: 999px;
    color: var(--text-1);
    border: 1px solid var(--border);
    line-height: 1.4;
    font-family: inherit;
    cursor: pointer;
    transition: border-color 100ms ease, filter 100ms ease;
  }
  .di-tag:hover:not(:disabled) {
    border-color: var(--text-2);
    filter: brightness(1.1);
  }
  .di-tag:disabled {
    cursor: wait;
    opacity: 0.6;
  }
  .di-tag-remove {
    font-size: 13px;
    line-height: 1;
    opacity: 0.72;
  }
  .di-section-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }
  .di-count {
    color: var(--text-3);
    font-size: 11px;
    font-variant-numeric: tabular-nums;
  }
  .di-tag-empty {
    color: var(--text-3);
    font-size: 12px;
    margin: 7px 0 9px;
  }
  .di-tag-search {
    margin-top: 8px;
  }
  .di-tag-options {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 5px;
    margin: 7px 0;
  }
  .di-tag-option {
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 6px 8px;
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    background: var(--surface-2);
    color: var(--text-1);
    font: inherit;
    font-size: 11px;
    text-align: left;
    cursor: pointer;
    transition: border-color 100ms ease, background 100ms ease;
  }
  .di-tag-option:hover:not(:disabled) {
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 8%, var(--surface-2));
  }
  .di-tag-option:disabled {
    cursor: wait;
    opacity: 0.6;
  }
  .di-tag-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    flex: 0 0 auto;
  }
  .di-tag-option-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .di-tag-option-name mark {
    color: var(--text-1);
    background: color-mix(in srgb, var(--accent) 28%, transparent);
    border-radius: 2px;
  }
  .di-tag-add {
    margin-left: auto;
    color: var(--accent);
    font-size: 14px;
    line-height: 1;
  }
  .di-meta {
    padding-bottom: 18px;
  }
  .di-meta-list {
    margin: 8px 0 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .di-meta-row {
    display: grid;
    grid-template-columns: 84px 1fr;
    gap: 12px;
    font-size: 12px;
  }
  .di-meta-row dt {
    color: var(--text-3);
    font-weight: 500;
  }
  .di-meta-row dd {
    color: var(--text-1);
    margin: 0;
    word-break: break-word;
  }
  .di-mono {
    font-family: var(--mono, ui-monospace, monospace);
    font-size: 11px;
  }
  .di-ocr-err {
    font-size: 11px;
    color: #ef4444;
    margin-top: 4px;
    background: rgba(239, 68, 68, 0.06);
    padding: 4px 8px;
    border-radius: var(--r-sm);
  }
  .di-foot {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 14px 18px;
    border-top: 1px solid var(--border);
    background: var(--bg-1);
    position: sticky;
    bottom: 0;
  }
  .di-foot-spacer {
    flex: 1;
  }
  .di-btn {
    background: var(--surface-2);
    border: 1px solid var(--border);
    color: var(--text-1);
    padding: 6px 12px;
    border-radius: var(--r-sm);
    font-size: 12px;
    cursor: pointer;
    transition: background 100ms ease, border-color 100ms ease;
  }
  .di-btn:hover:not(:disabled) {
    background: var(--surface-3, var(--surface-2));
    border-color: var(--text-3);
  }
  .di-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .di-btn.primary {
    background: var(--accent);
    color: white;
    border-color: var(--accent);
  }
  .di-btn.primary:hover:not(:disabled) {
    background: var(--accent-hover, var(--accent));
    border-color: var(--accent-hover, var(--accent));
  }
  .di-btn.ghost {
    background: transparent;
  }
  .di-btn.danger {
    background: #ef4444;
    color: white;
    border-color: #b91c1c;
  }
  .di-btn.danger-ghost {
    color: #ef4444;
  }
  .di-btn.danger-ghost:hover:not(:disabled) {
    background: rgba(239, 68, 68, 0.08);
    border-color: rgba(239, 68, 68, 0.4);
  }
</style>
