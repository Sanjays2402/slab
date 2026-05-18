<script lang="ts">
  // EditTextPanel — Slice 7 of v0.11.0 "Lathe".
  //
  // The UI half of in-place PDF text editing.
  //
  // Flow:
  //   1. Pick a PDF (or drag/drop later).
  //   2. We call `slab_find_text_spans` and get back per-page span lists,
  //      each with: id, text, editable, reason, x/y, font_resource.
  //   3. Render spans grouped by page, with an inline text input for every
  //      editable span and a read-only badge (with reason tooltip) for the
  //      rest.
  //   4. As the user edits, we track dirty spans in `pendingEdits` keyed by
  //      span id.
  //   5. Apply → call `slab_replace_text_span` once per dirty span, against
  //      a chain of temp files so each edit composes on top of the last.
  //   6. The final output is saved next to the input as `<name>-edited.pdf`
  //      (or wherever the user picks).
  //
  // Why a panel and not click-to-edit-in-Reader: chaining content-stream
  // rewrites is easier to reason about with a flat list. Reader-overlay
  // editing is Slice 7.5 and depends on this panel landing first to prove
  // the backend round-trip.

  import { invoke } from "@tauri-apps/api/core";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { idle, basename, type CmdResult, type Status } from "$lib/types";

  type TextSpan = {
    id: string;
    page: number;
    text: string;
    font_resource: string;
    font_size: number;
    x: number;
    y: number;
    editable: boolean;
    reason: string | null;
  };
  type PageSpans = {
    page: number;
    spans: TextSpan[];
  };

  let input = $state<string | null>(null);
  let pageGroups = $state<PageSpans[]>([]);
  let pendingEdits = $state<Record<string, string>>({});
  let status = $state<Status>(idle);
  let activePage = $state<number | null>(null);
  let filter = $state<"editable" | "all">("editable");
  let scanning = $state(false);

  // Derived: total spans / editable count / dirty count.
  let totalSpans = $derived(pageGroups.reduce((acc, g) => acc + g.spans.length, 0));
  let editableSpans = $derived(
    pageGroups.reduce((acc, g) => acc + g.spans.filter((s) => s.editable).length, 0)
  );
  let dirtyCount = $derived(Object.keys(pendingEdits).length);
  let activeGroup = $derived(
    activePage !== null ? pageGroups.find((g) => g.page === activePage) ?? null : null
  );

  async function pickInput() {
    const picked = await open({
      multiple: false,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof picked !== "string") return;
    await loadPdf(picked);
  }

  async function loadPdf(path: string) {
    input = path;
    pageGroups = [];
    pendingEdits = {};
    activePage = null;
    scanning = true;
    status = { kind: "working", msg: "Scanning text spans…" };
    try {
      const res = await invoke<CmdResult<PageSpans[]>>("slab_find_text_spans", {
        input: path,
      });
      if (res.kind === "ok") {
        pageGroups = res.value;
        const firstNonEmpty = pageGroups.find((g) => g.spans.length > 0);
        activePage = firstNonEmpty?.page ?? pageGroups[0]?.page ?? null;
        const ed = res.value.reduce(
          (acc, g) => acc + g.spans.filter((s) => s.editable).length,
          0
        );
        const tot = res.value.reduce((acc, g) => acc + g.spans.length, 0);
        if (tot === 0) {
          status = {
            kind: "err",
            msg: "No text spans found. This PDF might be image-only — run OCR first.",
          };
        } else if (ed === 0) {
          status = {
            kind: "err",
            msg: `Found ${tot} span${tot === 1 ? "" : "s"}, but none are editable yet (likely CID/Unicode fonts).`,
          };
        } else {
          status = {
            kind: "ok",
            msg: `Found ${ed} editable span${ed === 1 ? "" : "s"} across ${res.value.length} page${res.value.length === 1 ? "" : "s"}.`,
          };
        }
      } else {
        status = { kind: "err", msg: res.message };
      }
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    } finally {
      scanning = false;
    }
  }

  function setEdit(id: string, newText: string, original: string) {
    if (newText === original) {
      // Reverted to original — drop from pending.
      // eslint-disable-next-line @typescript-eslint/no-unused-vars
      const { [id]: _drop, ...rest } = pendingEdits;
      pendingEdits = rest;
    } else {
      pendingEdits = { ...pendingEdits, [id]: newText };
    }
  }

  function revertAll() {
    pendingEdits = {};
  }

  async function applyEdits() {
    if (!input) return;
    if (dirtyCount === 0) {
      status = { kind: "err", msg: "Nothing to apply — edit at least one span first." };
      return;
    }

    // Default output: <input dir>/<basename>-edited.pdf, but let the user
    // override with a save dialog so they can pick a different folder.
    const dot = input.lastIndexOf(".");
    const stem = dot > 0 ? input.slice(0, dot) : input;
    const suggested = `${stem}-edited.pdf`;
    const picked = await save({
      defaultPath: suggested,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof picked !== "string") return;

    status = { kind: "working", msg: `Applying ${dirtyCount} edit${dirtyCount === 1 ? "" : "s"}…` };
    try {
      // Chain edits: each replace_text_span call rewrites the file. We
      // pass the previous output as the next input so writes compose.
      // We write everything to the same picked path; if the user picked
      // the same file as input we still ping-pong via the picked path
      // because Tauri saves an absolute path.
      let current = input;
      const ids = Object.keys(pendingEdits);
      for (let i = 0; i < ids.length; i++) {
        const id = ids[i];
        const newText = pendingEdits[id];
        const dest = picked;
        const res = await invoke<CmdResult<null>>("slab_replace_text_span", {
          input: current,
          output: dest,
          spanId: id,
          newText,
        });
        if (res.kind === "err") {
          status = { kind: "err", msg: `Edit ${i + 1}/${ids.length} failed: ${res.message}` };
          return;
        }
        current = dest;
      }
      status = {
        kind: "ok",
        msg: `Wrote ${dirtyCount} edit${dirtyCount === 1 ? "" : "s"} → ${picked}`,
      };
      // After a successful save, swap the input over to the new file so
      // the user can continue editing the result. We also re-scan so
      // span ids stay accurate (the content stream got rewritten).
      pendingEdits = {};
      await loadPdf(picked);
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }

  // Convenience: clamp displayed text to a sensible length so a multi-
  // hundred-char Tj doesn't blow out the row.
  function truncate(s: string, n = 280): string {
    if (s.length <= n) return s;
    return s.slice(0, n) + "…";
  }
</script>

<header class="content-header">
  <h1>Edit Text</h1>
  <p class="subtitle">
    Click any text span on the page to retype it. Slab rewrites the PDF's
    content stream in place — no flattening, no font re-embedding, no
    external server round-trip.
  </p>
</header>

<section class="panel">
  {#if !input}
    <button class="dropzone" onclick={pickInput}>
      <span class="dz-icon">✎</span>
      <span class="dz-title">Choose a PDF to edit</span>
      <span class="dz-hint">Works best on PDFs with ASCII text and standard fonts (Type1, TrueType).</span>
    </button>
  {:else}
    <div class="file-card">
      <div>
        <div class="file-name">{basename(input)}</div>
        <div class="file-meta">
          {#if scanning}Scanning…{:else}{editableSpans} of {totalSpans} span{totalSpans === 1 ? "" : "s"} editable · {pageGroups.length} page{pageGroups.length === 1 ? "" : "s"}{/if}
        </div>
      </div>
      <button class="ghost" onclick={pickInput}>Change</button>
    </div>

    {#if pageGroups.length > 0}
      <div class="page-bar">
        <div class="page-tabs" role="tablist">
          {#each pageGroups as g (g.page)}
            {@const editableHere = g.spans.filter((s) => s.editable).length}
            <button
              class="page-tab"
              class:active={activePage === g.page}
              class:has-edits={g.spans.some((s) => pendingEdits[s.id] !== undefined)}
              role="tab"
              aria-selected={activePage === g.page}
              onclick={() => (activePage = g.page)}
              title={`Page ${g.page} — ${editableHere} editable / ${g.spans.length} total`}
            >
              p{g.page}
              {#if g.spans.length > 0}
                <span class="page-tab-count">{editableHere}</span>
              {/if}
            </button>
          {/each}
        </div>
        <div class="filter-toggle">
          <button class:active={filter === "editable"} onclick={() => (filter = "editable")}>
            Editable only
          </button>
          <button class:active={filter === "all"} onclick={() => (filter = "all")}>
            All spans
          </button>
        </div>
      </div>

      {#if activeGroup}
        {@const visible = filter === "editable" ? activeGroup.spans.filter((s) => s.editable) : activeGroup.spans}
        {#if visible.length === 0}
          <div class="empty-page">
            {filter === "editable"
              ? "No editable spans on this page."
              : "No text spans on this page."}
          </div>
        {:else}
          <div class="span-list">
            {#each visible as span (span.id)}
              {@const dirty = pendingEdits[span.id] !== undefined}
              {@const currentValue = pendingEdits[span.id] ?? span.text}
              <div class="span-row" class:dirty class:locked={!span.editable}>
                <div class="span-head">
                  <span class="span-id" title={`Stream-order span on page ${span.page}`}>
                    {span.id}
                  </span>
                  <span class="span-font" title="Font + size at this span">
                    {span.font_resource || "?"} · {span.font_size.toFixed(0)}pt
                  </span>
                  {#if !span.editable}
                    <span class="span-lock" title={span.reason ?? "Read-only"}>read-only</span>
                  {:else if dirty}
                    <span class="span-dirty" title="Edited locally — Apply to save">edited</span>
                  {/if}
                </div>
                {#if span.editable}
                  <input
                    class="span-input"
                    type="text"
                    value={currentValue}
                    aria-label="Edit text for span on page {span.page}"
                    oninput={(e) => setEdit(span.id, (e.currentTarget as HTMLInputElement).value, span.text)}
                    spellcheck="false"
                    autocapitalize="off"
                    autocorrect="off"
                  />
                  {#if dirty}
                    <button
                      class="span-revert"
                      onclick={() => setEdit(span.id, span.text, span.text)}
                      title="Revert to original"
                    >
                      ↶ revert
                    </button>
                  {/if}
                {:else}
                  <div class="span-readonly" title={span.reason ?? "Read-only"}>
                    {truncate(span.text)}
                  </div>
                  {#if span.reason}
                    <div class="span-reason">{span.reason}</div>
                  {/if}
                {/if}
              </div>
            {/each}
          </div>
        {/if}
      {/if}
    {/if}

    <div class="actions">
      <button class="ghost" onclick={revertAll} disabled={dirtyCount === 0}>
        Revert all
      </button>
      <button class="primary" onclick={applyEdits} disabled={dirtyCount === 0 || status.kind === "working"}>
        {status.kind === "working" ? "Applying…" : `Apply ${dirtyCount || ""} edit${dirtyCount === 1 ? "" : "s"}`.trim()}
      </button>
    </div>
  {/if}

  {#if status.kind === "ok"}
    <div class="status ok">✓ {status.msg}</div>
  {:else if status.kind === "err"}
    <div class="status err">✕ {status.msg}</div>
  {/if}

  <details class="caveats">
    <summary>What's editable in v0.11?</summary>
    <ul>
      <li><strong>ASCII rewrites only.</strong> Replacing with characters above U+007F is rejected for now — would corrupt PDFs with non-Unicode fonts.</li>
      <li><strong>Type1 / TrueType fonts.</strong> CID Type-0 (used for CJK + Unicode-rich docs) is read-only until v0.12 — span shows a lock badge.</li>
      <li><strong>No kerning gaps.</strong> Spans drawn with positioning adjustments inside a TJ array show as read-only — straight rewriting would smush together. Convert to plain text in your source first.</li>
      <li><strong>Layout reflow is not promised.</strong> The new text is drawn at the same x/y as the old. A longer string will run past the original right edge.</li>
    </ul>
  </details>
</section>

<style>
  .page-bar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
    margin-top: 4px;
  }
  .page-tabs {
    display: flex;
    gap: 4px;
    flex-wrap: wrap;
    flex: 1;
  }
  .page-tab {
    background: var(--bg-3);
    color: var(--text-2);
    border: 1px solid var(--border);
    padding: 4px 10px;
    border-radius: 6px;
    font-family: var(--mono, monospace);
    font-size: 11px;
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .page-tab:hover {
    color: var(--text);
    border-color: var(--accent);
  }
  .page-tab.active {
    background: var(--accent);
    color: var(--bg-1, #000);
    border-color: var(--accent);
  }
  .page-tab.has-edits::after {
    content: "•";
    color: var(--warning, #f5a623);
    margin-left: 2px;
  }
  .page-tab-count {
    background: rgba(0, 0, 0, 0.18);
    color: inherit;
    padding: 0 6px;
    border-radius: 999px;
    font-size: 10px;
    font-weight: 600;
  }
  .filter-toggle {
    display: inline-flex;
    background: var(--bg-3);
    border: 1px solid var(--border);
    border-radius: 6px;
    overflow: hidden;
  }
  .filter-toggle button {
    background: transparent;
    color: var(--text-2);
    border: 0;
    padding: 4px 10px;
    font-size: 11px;
  }
  .filter-toggle button.active {
    background: var(--accent);
    color: var(--bg-1, #000);
  }
  .empty-page {
    padding: 16px;
    text-align: center;
    color: var(--text-3);
    font-size: 12px;
    border: 1px dashed var(--border);
    border-radius: var(--r-sm);
  }
  .span-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: 6px;
  }
  .span-row {
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    padding: 8px 10px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    transition: border-color 0.12s ease;
  }
  .span-row.dirty {
    border-color: var(--warning, #f5a623);
    background: color-mix(in srgb, var(--warning, #f5a623) 6%, var(--bg-2));
  }
  .span-row.locked {
    opacity: 0.78;
  }
  .span-head {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 11px;
    color: var(--text-3);
  }
  .span-id {
    font-family: var(--mono, monospace);
    background: var(--bg-3);
    padding: 1px 6px;
    border-radius: 4px;
    color: var(--text-2);
  }
  .span-font {
    font-family: var(--mono, monospace);
    color: var(--text-3);
  }
  .span-lock {
    background: var(--bg-3);
    color: var(--text-3);
    padding: 1px 6px;
    border-radius: 999px;
    font-size: 10px;
  }
  .span-dirty {
    background: var(--warning, #f5a623);
    color: #000;
    padding: 1px 6px;
    border-radius: 999px;
    font-size: 10px;
    font-weight: 600;
  }
  .span-input {
    background: var(--bg-1, #111);
    color: var(--text);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 6px 8px;
    font-size: 13px;
    width: 100%;
    font-family: inherit;
  }
  .span-input:focus {
    border-color: var(--accent);
    outline: none;
  }
  .span-revert {
    align-self: flex-start;
    background: transparent;
    color: var(--text-3);
    border: 0;
    font-size: 11px;
    padding: 2px 4px;
  }
  .span-revert:hover {
    color: var(--text);
  }
  .span-readonly {
    font-size: 13px;
    color: var(--text-2);
    padding: 6px 0;
    line-height: 1.4;
  }
  .span-reason {
    font-size: 11px;
    color: var(--text-3);
    font-style: italic;
  }
  .caveats {
    margin-top: 12px;
    font-size: 12px;
    color: var(--text-3);
  }
  .caveats summary {
    cursor: pointer;
    color: var(--text-2);
    padding: 6px 0;
  }
  .caveats ul {
    margin: 0;
    padding-left: 20px;
    line-height: 1.6;
  }
</style>
