<script lang="ts">
  /**
   * v3.30.0 "Quill Smart Fill" — the AI-powered form-filling wow.
   *
   * The flow:
   *   1. User picks (or drag-drops) a target AcroForm PDF.
   *   2. User picks (or drag-drops) a source document (resume PDF,
   *      contact-card markdown, last year's tax form, a CSV row, …).
   *   3. We call `slab_quill_smart_fill_propose` — a 100% local AI
   *      call through the user's configured Beacon provider (Ollama by
   *      default).
   *   4. The proposal lands in this panel as a diff list: each target
   *      field with the model's suggested value, a confidence chip,
   *      and a per-row accept toggle.
   *   5. User clicks "Apply" → we pipe the accepted name→value map
   *      into the existing `slab_forms_fill` engine, which writes a
   *      new PDF with `/NeedAppearances=true`.
   *
   * Adobe Acrobat charges extra for AI form filling and ships your
   * file to their cloud. PDF Expert and Foxit don't ship this at all.
   * Slab does it on-device, free.
   */
  import { invoke } from "@tauri-apps/api/core";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { idle, basename, stripExt, type CmdResult, type Status } from "$lib/types";
  import {
    quill,
    setInput as quillSetInput,
    recordFormsReport as quillRecordReport,
  } from "$lib/quill";

  // --- Wire types — mirror src-tauri/src/pdf/forms_smart_fill/mapper.rs ----

  type ProposalEntry = {
    field: string;
    value: string | null;
    confidence: number;
    source_span: string | null;
  };

  type SmartFillProposal = {
    entries: ProposalEntry[];
    warnings: string[];
    provider: string;
    model: string;
  };

  type FillReport = {
    filled: string[];
    unknown: string[];
    read_only_skipped: string[];
    need_appearances: boolean;
  };

  // --- State -------------------------------------------------------------

  // The target PDF mirrors the Hub-wide input via the `quill` store.
  // Reading the store lets the user drop a target in the Detect/Fill
  // tab first, then walk over here without re-picking.
  let hubInput = $derived($quill.input);

  let sourceDoc = $state<string | null>(null);
  let proposal = $state<SmartFillProposal | null>(null);
  let accepted = $state<Record<string, boolean>>({});
  let edited = $state<Record<string, string>>({});
  let status = $state<Status>(idle);

  // Drag-state for the two drop zones.
  let dragOverTarget = $state(false);
  let dragOverSource = $state(false);

  // Derived counts.
  let totalRows = $derived(proposal?.entries.length ?? 0);
  let acceptedRows = $derived(
    proposal ? proposal.entries.filter((e) => accepted[e.field] !== false && e.value).length : 0,
  );
  let lowConfidenceRows = $derived(
    proposal ? proposal.entries.filter((e) => e.confidence < 0.5 && e.value).length : 0,
  );

  // --- Picking ------------------------------------------------------------

  async function pickTarget() {
    const picked = await open({
      multiple: false,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof picked !== "string") return;
    setTarget(picked);
  }

  function setTarget(path: string) {
    quillSetInput(path);
    proposal = null;
    accepted = {};
    edited = {};
    status = idle;
  }

  async function pickSource() {
    const picked = await open({
      multiple: false,
      filters: [
        { name: "Document", extensions: ["pdf", "txt", "md", "csv", "json"] },
        { name: "All Files", extensions: ["*"] },
      ],
    });
    if (typeof picked !== "string") return;
    setSource(picked);
  }

  function setSource(path: string) {
    sourceDoc = path;
    proposal = null;
    accepted = {};
    edited = {};
    status = idle;
  }

  // --- Drag & drop --------------------------------------------------------

  // Tauri's webview gives us the raw FS path inside `dataTransfer.files[0].path`
  // (added by the `tauri-plugin-fs` runtime). We accept both that and the
  // standard browser `name`-only path for graceful dev-server fallback.
  function pathFromDrop(e: DragEvent): string | null {
    const f = e.dataTransfer?.files?.[0];
    if (!f) return null;
    // Tauri injects `.path` on the File object. In dev fall back to name.
    return (f as File & { path?: string }).path ?? f.name ?? null;
  }

  function onDropTarget(e: DragEvent) {
    e.preventDefault();
    dragOverTarget = false;
    const p = pathFromDrop(e);
    if (p && p.toLowerCase().endsWith(".pdf")) setTarget(p);
    else status = { kind: "err", msg: "Target must be a .pdf file." };
  }

  function onDropSource(e: DragEvent) {
    e.preventDefault();
    dragOverSource = false;
    const p = pathFromDrop(e);
    if (p) setSource(p);
  }

  // --- The wow: propose -------------------------------------------------

  async function runPropose() {
    if (!hubInput || !sourceDoc) {
      status = { kind: "err", msg: "Pick both a target PDF and a source document first." };
      return;
    }
    status = { kind: "working", msg: "Reading source, asking the model…" };
    proposal = null;
    accepted = {};
    edited = {};
    try {
      const res = await invoke<CmdResult<SmartFillProposal>>(
        "slab_quill_smart_fill_propose",
        { targetPdf: hubInput, sourceDoc },
      );
      if (res.kind === "ok") {
        proposal = res.value;
        // Default: accept any row the model gave us a value for.
        const acc: Record<string, boolean> = {};
        const ed: Record<string, string> = {};
        for (const e of res.value.entries) {
          acc[e.field] = e.value != null && e.value !== "";
          if (e.value != null) ed[e.field] = e.value;
        }
        accepted = acc;
        edited = ed;
        const note =
          res.value.warnings.length > 0
            ? ` · ${res.value.warnings.length} warning${res.value.warnings.length === 1 ? "" : "s"}`
            : "";
        status = {
          kind: "ok",
          msg: `Proposed ${res.value.entries.length} field${res.value.entries.length === 1 ? "" : "s"} via ${res.value.provider}${res.value.model ? ` (${res.value.model})` : ""}${note}.`,
        };
      } else {
        status = { kind: "err", msg: res.message };
      }
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }

  // --- Apply: pipe through the existing forms_fill engine ----------------

  async function applyAccepted() {
    if (!hubInput || !proposal) return;
    const values: Record<string, string> = {};
    for (const e of proposal.entries) {
      if (accepted[e.field] !== false && edited[e.field] != null && edited[e.field] !== "") {
        values[e.field] = edited[e.field];
      }
    }
    if (Object.keys(values).length === 0) {
      status = { kind: "err", msg: "Nothing accepted to apply." };
      return;
    }
    const base = stripExt(basename(hubInput));
    const output = await save({
      defaultPath: `${base}-smartfilled.pdf`,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof output !== "string") return;

    status = { kind: "working", msg: "Writing filled PDF…" };
    try {
      const res = await invoke<CmdResult<FillReport>>("slab_forms_fill", {
        input: hubInput,
        values,
        output,
      });
      if (res.kind === "ok") {
        const r = res.value;
        // Mirror to the hub so the Fill tab's report stays in sync.
        quillRecordReport({
          has_acroform: true,
          need_appearances: r.need_appearances,
          has_xfa: false,
          fields: Object.entries(values).map(([name, value]) => ({
            name,
            value,
            type: "text",
          })),
        });
        const skipped =
          r.read_only_skipped.length > 0
            ? ` (${r.read_only_skipped.length} read-only skipped)`
            : "";
        status = {
          kind: "ok",
          msg: `Filled ${r.filled.length} field${r.filled.length === 1 ? "" : "s"}${skipped} → ${basename(output)}`,
        };
      } else {
        status = { kind: "err", msg: res.message };
      }
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }

  function confidenceClass(c: number): string {
    if (c >= 0.85) return "conf-high";
    if (c >= 0.5) return "conf-med";
    return "conf-low";
  }

  function confidenceLabel(c: number): string {
    if (c >= 0.85) return "high";
    if (c >= 0.5) return "med";
    return "low";
  }

  function toggleAll(on: boolean) {
    if (!proposal) return;
    const next: Record<string, boolean> = {};
    for (const e of proposal.entries) next[e.field] = on && e.value != null && e.value !== "";
    accepted = next;
  }
</script>

<header class="content-header">
  <h1>Smart Fill</h1>
  <p class="subtitle">
    Drop a resume on a job application, last year's tax form on this year's, or
    a contact card on any AcroForm — Slab's local AI maps source → fields and
    you accept/reject row-by-row. Nothing leaves your machine.
  </p>
</header>

<section class="panel">
  <div class="drops">
    <button
      type="button"
      class="dropzone"
      class:filled={!!hubInput}
      class:over={dragOverTarget}
      onclick={pickTarget}
      ondragenter={(e) => {
        e.preventDefault();
        dragOverTarget = true;
      }}
      ondragover={(e) => {
        e.preventDefault();
      }}
      ondragleave={() => (dragOverTarget = false)}
      ondrop={onDropTarget}
    >
      <span class="dz-num">1</span>
      <span class="dz-title">{hubInput ? basename(hubInput) : "Target PDF"}</span>
      <span class="dz-hint"
        >{hubInput ? "Click to change" : "Drop the fillable form here"}</span
      >
    </button>

    <span class="arrow" aria-hidden="true">→</span>

    <button
      type="button"
      class="dropzone"
      class:filled={!!sourceDoc}
      class:over={dragOverSource}
      onclick={pickSource}
      ondragenter={(e) => {
        e.preventDefault();
        dragOverSource = true;
      }}
      ondragover={(e) => {
        e.preventDefault();
      }}
      ondragleave={() => (dragOverSource = false)}
      ondrop={onDropSource}
    >
      <span class="dz-num">2</span>
      <span class="dz-title">{sourceDoc ? basename(sourceDoc) : "Source doc"}</span>
      <span class="dz-hint"
        >{sourceDoc ? "Click to change" : "Resume · CSV · prior form · markdown"}</span
      >
    </button>
  </div>

  <div class="actions-row">
    <button
      class="primary"
      onclick={runPropose}
      disabled={!hubInput || !sourceDoc || status.kind === "working"}
      data-testid="smartfill-propose"
    >
      {status.kind === "working" && !proposal ? status.msg : "Propose with AI"}
    </button>
    {#if proposal}
      <span class="counter">
        {acceptedRows} of {totalRows} accepted
        {#if lowConfidenceRows > 0}
          <span class="low-warn">· {lowConfidenceRows} low-confidence</span>
        {/if}
      </span>
      <div class="spacer"></div>
      <button class="ghost" onclick={() => toggleAll(true)}>Accept all</button>
      <button class="ghost" onclick={() => toggleAll(false)}>Reject all</button>
      <button
        class="primary"
        onclick={applyAccepted}
        disabled={acceptedRows === 0 || status.kind === "working"}
        data-testid="smartfill-apply"
      >
        {status.kind === "working" && proposal ? status.msg : `Apply (${acceptedRows})`}
      </button>
    {/if}
  </div>

  {#if proposal}
    {#if proposal.warnings.length > 0}
      <div class="warn-banner">
        {#each proposal.warnings as w}
          <div>⚠ {w}</div>
        {/each}
      </div>
    {/if}

    <div class="table-wrap" role="region" aria-label="Smart fill proposal">
      <table class="rows">
        <thead>
          <tr>
            <th class="col-accept">Accept</th>
            <th class="col-field">Field</th>
            <th class="col-value">Proposed Value</th>
            <th class="col-conf">Confidence</th>
          </tr>
        </thead>
        <tbody>
          {#each proposal.entries as e (e.field)}
            <tr
              class:rejected={accepted[e.field] === false}
              class:empty-row={e.value == null || e.value === ""}
            >
              <td class="col-accept">
                <input
                  type="checkbox"
                  bind:checked={accepted[e.field]}
                  disabled={e.value == null || e.value === ""}
                  aria-label={`Accept ${e.field}`}
                />
              </td>
              <td class="col-field">
                <code>{e.field}</code>
              </td>
              <td class="col-value">
                {#if e.value != null}
                  <input
                    class="value-input"
                    type="text"
                    bind:value={edited[e.field]}
                    disabled={accepted[e.field] === false}
                  />
                {:else}
                  <span class="no-value">— no match —</span>
                {/if}
              </td>
              <td class="col-conf">
                {#if e.value != null}
                  <span class={`conf-chip ${confidenceClass(e.confidence)}`}>
                    {confidenceLabel(e.confidence)} · {Math.round(e.confidence * 100)}%
                  </span>
                {:else}
                  <span class="conf-chip conf-low">—</span>
                {/if}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>

    <div class="note">
      Generated by <strong>{proposal.provider}</strong>{proposal.model
        ? ` (${proposal.model})`
        : ""}. Edit any value inline, untick rows you don't trust, then click
      Apply to write a new PDF. Nothing was sent to the cloud.
    </div>
  {/if}

  {#if status.kind === "ok"}
    <div class="status ok">✓ {status.msg}</div>
  {:else if status.kind === "err"}
    <div class="status err">✕ {status.msg}</div>
  {/if}
</section>

<style>
  .panel {
    padding: 16px 24px 32px;
  }
  .drops {
    display: grid;
    grid-template-columns: 1fr auto 1fr;
    align-items: stretch;
    gap: 12px;
    margin-bottom: 16px;
  }
  .arrow {
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 24px;
    opacity: 0.4;
  }
  .dropzone {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    justify-content: center;
    gap: 6px;
    padding: 22px 24px;
    border-radius: var(--r-md, 12px);
    border: 1.5px dashed var(--border, rgba(255, 255, 255, 0.15));
    background: var(--bg-2, rgba(255, 255, 255, 0.02));
    color: inherit;
    text-align: left;
    cursor: pointer;
    transition:
      background 120ms ease,
      border-color 120ms ease,
      transform 120ms ease;
    min-height: 88px;
  }
  .dropzone:hover {
    background: var(--bg-3, rgba(255, 255, 255, 0.04));
  }
  .dropzone.over {
    border-color: var(--accent-strong, #88b0ff);
    background: var(--accent-soft, rgba(120, 160, 255, 0.12));
    transform: scale(1.01);
  }
  .dropzone.filled {
    border-style: solid;
    border-color: var(--accent-strong, #88b0ff);
    background: var(--accent-soft, rgba(120, 160, 255, 0.08));
  }
  .dz-num {
    position: absolute;
    top: 8px;
    right: 12px;
    font-size: 11px;
    font-weight: 700;
    color: var(--text-3, rgba(255, 255, 255, 0.5));
    background: var(--bg-1, rgba(0, 0, 0, 0.2));
    border-radius: 999px;
    padding: 2px 8px;
  }
  .dz-title {
    font-size: 14px;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 100%;
  }
  .dz-hint {
    font-size: 12px;
    opacity: 0.6;
  }
  .actions-row {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 8px 0 14px;
    flex-wrap: wrap;
  }
  .counter {
    font-size: 12.5px;
    color: var(--text-2, rgba(255, 255, 255, 0.75));
  }
  .low-warn {
    color: #c0561e;
    margin-left: 4px;
  }
  .spacer {
    flex: 1 1 auto;
  }
  .primary {
    padding: 7px 16px;
    border-radius: var(--r-sm, 6px);
    border: 1px solid var(--accent-strong, #88b0ff);
    background: var(--accent-soft, rgba(120, 160, 255, 0.18));
    color: var(--accent-strong, #88b0ff);
    font-weight: 600;
    cursor: pointer;
    font-size: 13px;
  }
  .primary:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
  .ghost {
    padding: 6px 12px;
    border-radius: var(--r-sm, 6px);
    border: 1px solid var(--border, rgba(255, 255, 255, 0.1));
    background: transparent;
    color: inherit;
    cursor: pointer;
    font-size: 13px;
  }
  .warn-banner {
    border: 1px solid rgba(192, 142, 30, 0.5);
    background: rgba(192, 142, 30, 0.12);
    color: #d6a04a;
    padding: 8px 12px;
    border-radius: var(--r-sm, 6px);
    font-size: 12.5px;
    margin-bottom: 10px;
  }
  .table-wrap {
    border: 1px solid var(--border);
    border-radius: var(--r-md, 8px);
    background: var(--bg-2);
    overflow: auto;
    max-height: 520px;
  }
  table.rows {
    width: 100%;
    border-collapse: collapse;
    font-size: 13px;
  }
  table.rows thead th {
    position: sticky;
    top: 0;
    background: var(--bg-1);
    text-align: left;
    padding: 9px 12px;
    font-weight: 600;
    color: var(--text-3, rgba(255, 255, 255, 0.55));
    border-bottom: 1px solid var(--border);
    font-size: 11px;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    z-index: 1;
  }
  table.rows tbody td {
    padding: 8px 12px;
    border-bottom: 1px solid var(--border);
    vertical-align: middle;
  }
  table.rows tbody tr:last-child td {
    border-bottom: none;
  }
  tr.rejected {
    opacity: 0.5;
  }
  tr.empty-row {
    opacity: 0.55;
  }
  .col-accept {
    width: 60px;
    text-align: center;
  }
  .col-conf {
    width: 130px;
  }
  .col-field code {
    font-family: var(--font-mono, ui-monospace, "SF Mono", monospace);
    font-size: 12px;
    color: var(--text-1);
  }
  .value-input {
    width: 100%;
    padding: 6px 8px;
    border-radius: var(--r-sm, 4px);
    border: 1px solid var(--border);
    background: var(--bg-1);
    color: var(--text-1);
    font-size: 13px;
    font-family: inherit;
  }
  .value-input:focus {
    outline: 2px solid var(--accent, #88b0ff);
    outline-offset: -1px;
  }
  .no-value {
    color: var(--text-3, rgba(255, 255, 255, 0.5));
    font-style: italic;
    font-size: 12px;
  }
  .conf-chip {
    display: inline-block;
    padding: 2px 8px;
    border-radius: 999px;
    font-size: 11px;
    font-weight: 600;
    border: 1px solid currentColor;
  }
  .conf-high {
    color: #2b8a3e;
  }
  .conf-med {
    color: #c0561e;
  }
  .conf-low {
    color: #b03030;
  }
  .note {
    margin-top: 12px;
    font-size: 12px;
    color: var(--text-3, rgba(255, 255, 255, 0.55));
    line-height: 1.5;
  }
  .status {
    margin-top: 12px;
    padding: 8px 12px;
    border-radius: var(--r-sm, 6px);
    font-size: 13px;
  }
  .status.ok {
    background: rgba(43, 138, 62, 0.12);
    color: #2b8a3e;
    border: 1px solid rgba(43, 138, 62, 0.3);
  }
  .status.err {
    background: rgba(176, 48, 48, 0.12);
    color: #c63030;
    border: 1px solid rgba(176, 48, 48, 0.35);
  }
</style>
