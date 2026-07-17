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
    - tag chips (read-only — tag editing already lives on the card
      via the existing context menu; the inspector just summarises)
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
    - onUpdate(updated) — called whenever the inspector successfully
      mutates title/notes/starred. The parent (LibraryPanel) splices
      `updated` into its `docs` array so the grid card repaints.
    - onRemove(docId) — called when the user confirms delete. The
      parent prunes the row and the drawer closes.

  Pure frontend slice: no new Tauri commands, no schema, no
  backend types. Reuses setDocumentTitle / setDocumentNotes /
  setDocumentStarred (slice 33 / 34 / 35 wire) + removeDocument
  (pre-existing).
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import {
    type DocumentRecord,
    setDocumentTitle,
    setDocumentNotes,
    setDocumentStarred,
    removeDocument,
  } from "$lib/library";
  import { basename, type CmdResult } from "$lib/types";
  import { formatRelTime } from "$lib/recent";

  type Props = {
    /** The document to inspect. `null` closes the drawer. The component
     * snapshots this on open and edits its own internal copy; the
     * parent should call onUpdate to keep its `docs` array in sync. */
    doc: DocumentRecord | null;
    /** Called when the inspector successfully writes a fresh row. */
    onUpdate?: (updated: DocumentRecord) => void;
    /** Called when the user confirms delete; the parent removes the
     * row from its grid and closes the drawer. */
    onRemove?: (docId: number) => void;
    /** Called for Escape/backdrop click/Close button. The parent sets
     * its inspector state to null. */
    onClose?: () => void;
  };

  let { doc, onUpdate = () => {}, onRemove = () => {}, onClose = () => {} }: Props = $props();

  // Local mirror of `doc` so the form fields are unconditionally
  // bindable strings. Re-syncs every time `doc` changes (i.e. the
  // parent reassigns inspectorDoc to a different row).
  let title = $state("");
  let notes = $state("");
  let starred = $state(false);
  let busyTitle = $state(false);
  let busyNotes = $state(false);
  let busyStar = $state(false);
  let busyDelete = $state(false);
  let error = $state<string | null>(null);
  let okToast = $state<string | null>(null);
  let confirmDelete = $state(false);

  // Maximum notes length — kept in sync with the backend constant in
  // src-tauri/src/pdf/library/registry.rs::MAX_DOC_NOTES_LEN.
  const NOTES_MAX = 4000;
  const TITLE_MAX = 500;

  // Reset the form state whenever a different doc lands. Without this,
  // the user would see stale fields if they inspect doc A, edit a bit,
  // then jump to doc B from the grid menu.
  let currentDocId = $state<number | null>(null);
  $effect(() => {
    if (doc && doc.id !== currentDocId) {
      currentDocId = doc.id;
      title = doc.title ?? "";
      notes = doc.notes ?? "";
      starred = doc.starred;
      error = null;
      okToast = null;
      confirmDelete = false;
    } else if (!doc) {
      currentDocId = null;
    }
  });

  // Close on Escape from anywhere inside the drawer (form fields too).
  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape" && !busyTitle && !busyNotes && !busyStar && !busyDelete) {
      onClose();
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

  async function saveTitleIfChanged() {
    if (!doc) return;
    const trimmed = title.trim();
    const current = doc.title ?? "";
    // No-op if the field unchanged (including empty -> empty).
    if (trimmed === current) return;
    if (trimmed.length > TITLE_MAX) {
      error = `Title too long (max ${TITLE_MAX} chars).`;
      return;
    }
    busyTitle = true;
    error = null;
    try {
      const updated = await setDocumentTitle(doc.id, trimmed || null);
      title = updated.title ?? "";
      onUpdate(updated);
      okToast = "Title saved";
    } catch (e) {
      error = `Title save failed: ${String(e)}`;
    } finally {
      busyTitle = false;
    }
  }

  async function saveNotesIfChanged() {
    if (!doc) return;
    const trimmed = notes;
    const current = doc.notes ?? "";
    // Trim-equality so an extra trailing newline isn't a "change".
    if (trimmed.trim() === current.trim()) return;
    if (trimmed.length > NOTES_MAX) {
      error = `Notes too long (max ${NOTES_MAX} chars).`;
      return;
    }
    busyNotes = true;
    error = null;
    try {
      const updated = await setDocumentNotes(doc.id, trimmed || null);
      notes = updated.notes ?? "";
      onUpdate(updated);
      okToast = "Notes saved";
    } catch (e) {
      error = `Notes save failed: ${String(e)}`;
    } finally {
      busyNotes = false;
    }
  }

  async function toggleStar() {
    if (!doc) return;
    busyStar = true;
    error = null;
    const next = !starred;
    try {
      const updated = await setDocumentStarred(doc.id, next);
      starred = updated.starred;
      onUpdate(updated);
    } catch (e) {
      error = `Star failed: ${String(e)}`;
    } finally {
      busyStar = false;
    }
  }

  async function onConfirmDelete() {
    if (!doc) return;
    busyDelete = true;
    error = null;
    try {
      await removeDocument(doc.id);
      const removedId = doc.id;
      onRemove(removedId);
    } catch (e) {
      error = `Delete failed: ${String(e)}`;
      busyDelete = false;
    }
  }

  // Reveal in Finder / Explorer — uses the cross-platform Tauri shell
  // open command. Fails silently with a toast if the path vanished.
  async function reveal() {
    if (!doc) return;
    try {
      // Try the platform-specific reveal first via our own command if
      // present; fall back to opening the path in the default opener.
      const res = await invoke<CmdResult<null>>("slab_reveal_in_finder", {
        path: doc.path,
      }).catch(() => ({ kind: "err", message: "no reveal command" }) as CmdResult<null>);
      if (res.kind === "ok") return;
      // Fallback: open the parent folder.
      const idx = doc.path.lastIndexOf("/");
      const parent = idx >= 0 ? doc.path.slice(0, idx) : doc.path;
      await invoke("slab_open_external", { path: parent });
    } catch (e) {
      error = `Reveal failed: ${String(e)}`;
    }
  }

  function openInReader() {
    if (!doc) return;
    // Same path the LibraryPanel uses — we dispatch the request the
    // App routes to a Reader tab.
    void invoke("slab_request_open_in_main", { path: doc.path }).catch(() => {
      // Browser mode — no-op; nothing else to do.
    });
    onClose();
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
      if (e.target === e.currentTarget) onClose();
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
            disabled={busyStar}
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
          onclick={onClose}
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
            disabled={busyTitle}
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
            disabled={busyNotes}
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

      {#if doc.tags.length > 0}
        <section class="di-section">
          <div class="di-label">Tags</div>
          <div class="di-tags">
            {#each doc.tags as t (t.id)}
              <span
                class="di-tag"
                style:background={t.color ?? "var(--surface-2)"}
                title={t.description ?? t.name}
              >{t.name}</span>
            {/each}
          </div>
          <div class="di-hint">
            Tags are edited from the doc-card context menu (right-click the card).
          </div>
        </section>
      {/if}

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
        <button class="di-btn primary" onclick={openInReader} disabled={busyDelete}>
          Open in Reader
        </button>
        <button class="di-btn ghost" onclick={reveal} disabled={busyDelete}>
          Reveal on disk
        </button>
        <span class="di-foot-spacer"></span>
        {#if confirmDelete}
          <button
            class="di-btn danger"
            onclick={onConfirmDelete}
            disabled={busyDelete}
          >
            {busyDelete ? "Removing…" : "Confirm remove"}
          </button>
          <button
            class="di-btn ghost"
            onclick={() => (confirmDelete = false)}
            disabled={busyDelete}
          >
            Cancel
          </button>
        {:else}
          <button
            class="di-btn ghost danger-ghost"
            onclick={() => (confirmDelete = true)}
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
    font-size: 11px;
    padding: 2px 8px;
    border-radius: 999px;
    color: var(--text-1);
    border: 1px solid var(--border);
    line-height: 1.4;
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
