<script lang="ts">
  /**
   * QuillDesignerPanel — v3.26.0 "Quill Designer".
   *
   * Author AcroForm fields on flat PDFs. Adobe Acrobat Pro charges $239/yr
   * for the "Prepare Form" tool; Slab ships it offline + free.
   *
   * Flow:
   *   1. Pick a PDF template (typically flat with no fields).
   *   2. Add drafts via the field-type buttons. Each draft has a name,
   *      page, rect (in PDF user-space points), and kind-specific config.
   *   3. Inspect the current PDF on the right (read-only) so you see what
   *      fields already exist.
   *   4. "Apply" calls slab_forms_design_add → writes a new PDF and
   *      automatically re-inspects to confirm.
   *   5. Existing fields can be edited (default value, required flag,
   *      tooltip) or deleted.
   *
   * Backend: src-tauri/src/pdf/forms_design.rs (15 unit tests).
   */
  import { invoke } from "@tauri-apps/api/core";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { idle, basename, type CmdResult, type Status } from "$lib/types";

  // ---- Types mirror src-tauri/src/pdf/forms_design.rs ---------------------

  type TextDraft = {
    kind: "text";
    multiline: boolean;
    max_len: number | null;
    default: string | null;
  };
  type CheckboxDraft = {
    kind: "checkbox";
    default_checked: boolean;
  };
  type RadioDraft = {
    kind: "radio";
    value: string;
    default_selected: boolean;
  };
  type DropdownDraft = {
    kind: "dropdown";
    options: string[];
    default: string | null;
    editable: boolean;
  };
  type SignatureDraft = { kind: "signature" };
  type Draft = TextDraft | CheckboxDraft | RadioDraft | DropdownDraft | SignatureDraft;

  type FieldDraft = Draft & {
    name: string;
    page: number;
    rect: [number, number, number, number];
    required: boolean;
    read_only: boolean;
    tooltip: string | null;
  };

  type FieldEdit = {
    current_name: string;
    new_name?: string | null;
    new_default?: string | null;
    required?: boolean | null;
    read_only?: boolean | null;
    tooltip?: string | null;
  };

  type DesignReport = {
    added: string[];
    edited: string[];
    deleted: string[];
    unknown: string[];
    errors: string[];
  };

  type FormsReport = {
    has_acroform: boolean;
    need_appearances: boolean;
    has_xfa: boolean;
    fields: Array<{
      name: string;
      type: string;
      value: string | null;
      page: number | null;
      rect: [number, number, number, number] | null;
      options: string[];
      read_only: boolean;
    }>;
  };

  // ---- State -------------------------------------------------------------

  let input = $state<string | null>(null);
  let inspected = $state<FormsReport | null>(null);
  let drafts = $state<FieldDraft[]>([]);
  let status = $state<Status>(idle);
  let report = $state<DesignReport | null>(null);

  // Pending edit form state (for the "Edit selected field" card).
  let editingName = $state<string | null>(null);
  let editNewDefault = $state("");
  let editTooltip = $state("");
  let editRequired = $state<boolean | null>(null);

  // ---- Derived -----------------------------------------------------------

  let existingFields = $derived(inspected?.fields ?? []);
  let totalDrafts = $derived(drafts.length);

  // ---- Helpers -----------------------------------------------------------

  async function pickInput() {
    const picked = await open({
      multiple: false,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof picked !== "string") return;
    input = picked;
    drafts = [];
    report = null;
    status = idle;
    await runInspect(picked);
  }

  async function runInspect(path: string) {
    status = { kind: "working", msg: "Inspecting…" };
    try {
      const res = await invoke<CmdResult<FormsReport>>("slab_forms_inspect", { input: path });
      if (res.kind === "ok") {
        inspected = res.value;
        status = idle;
      } else {
        status = { kind: "err", msg: res.message };
      }
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }

  function nextDraftName(prefix: string): string {
    const taken = new Set<string>([
      ...drafts.map((d) => d.name),
      ...(inspected?.fields ?? []).map((f) => f.name),
    ]);
    for (let i = 1; i < 1000; i++) {
      const candidate = `${prefix}_${i}`;
      if (!taken.has(candidate)) return candidate;
    }
    return `${prefix}_${Date.now()}`;
  }

  function addTextDraft() {
    drafts = [
      ...drafts,
      {
        name: nextDraftName("text"),
        kind: "text",
        multiline: false,
        max_len: null,
        default: null,
        page: 1,
        rect: [72, 700, 300, 720],
        required: false,
        read_only: false,
        tooltip: null,
      },
    ];
  }

  function addCheckboxDraft() {
    drafts = [
      ...drafts,
      {
        name: nextDraftName("check"),
        kind: "checkbox",
        default_checked: false,
        page: 1,
        rect: [72, 600, 92, 620],
        required: false,
        read_only: false,
        tooltip: null,
      },
    ];
  }

  function addDropdownDraft() {
    drafts = [
      ...drafts,
      {
        name: nextDraftName("choice"),
        kind: "dropdown",
        options: ["Option A", "Option B", "Option C"],
        default: "Option A",
        editable: false,
        page: 1,
        rect: [72, 500, 250, 520],
        required: false,
        read_only: false,
        tooltip: null,
      },
    ];
  }

  function addSignatureDraft() {
    drafts = [
      ...drafts,
      {
        name: nextDraftName("sig"),
        kind: "signature",
        page: 1,
        rect: [72, 400, 320, 450],
        required: false,
        read_only: false,
        tooltip: null,
      },
    ];
  }

  function removeDraft(idx: number) {
    drafts = drafts.filter((_, i) => i !== idx);
  }

  async function applyDrafts() {
    if (!input) return;
    if (drafts.length === 0) {
      status = { kind: "err", msg: "Add at least one field draft first." };
      return;
    }
    const output = await save({
      title: "Save form-enabled PDF as…",
      defaultPath: input.replace(/\.pdf$/i, "") + "-form.pdf",
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (!output) return;

    status = { kind: "working", msg: `Adding ${drafts.length} field(s)…` };
    try {
      const res = await invoke<CmdResult<DesignReport>>("slab_forms_design_add", {
        input,
        drafts,
        output,
      });
      if (res.kind === "ok") {
        report = res.value;
        status = {
          kind: "ok",
          msg: `Added ${res.value.added.length} field(s) → ${basename(output)}`,
        };
        // Re-inspect the new PDF so the user sees the result immediately.
        input = output;
        drafts = [];
        await runInspect(output);
      } else {
        status = { kind: "err", msg: res.message };
      }
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }

  function startEdit(name: string) {
    editingName = name;
    const f = (inspected?.fields ?? []).find((x) => x.name === name);
    editNewDefault = f?.value ?? "";
    editTooltip = "";
    editRequired = null;
  }

  async function commitEdit() {
    if (!input || !editingName) return;
    const output = await save({
      title: "Save edited PDF as…",
      defaultPath: input.replace(/\.pdf$/i, "") + "-edit.pdf",
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (!output) return;
    const edit: FieldEdit = {
      current_name: editingName,
      new_default: editNewDefault || null,
      required: editRequired,
      tooltip: editTooltip || null,
    };
    status = { kind: "working", msg: `Editing "${editingName}"…` };
    try {
      const res = await invoke<CmdResult<DesignReport>>("slab_forms_design_edit", {
        input,
        edits: [edit],
        output,
      });
      if (res.kind === "ok") {
        report = res.value;
        status = {
          kind: "ok",
          msg: `Edited ${res.value.edited.length} field(s) → ${basename(output)}`,
        };
        input = output;
        editingName = null;
        await runInspect(output);
      } else {
        status = { kind: "err", msg: res.message };
      }
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }

  async function deleteField(name: string) {
    if (!input) return;
    const output = await save({
      title: "Save trimmed PDF as…",
      defaultPath: input.replace(/\.pdf$/i, "") + "-trimmed.pdf",
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (!output) return;
    status = { kind: "working", msg: `Deleting "${name}"…` };
    try {
      const res = await invoke<CmdResult<DesignReport>>("slab_forms_design_delete", {
        input,
        names: [name],
        output,
      });
      if (res.kind === "ok") {
        report = res.value;
        status = {
          kind: "ok",
          msg: `Deleted ${res.value.deleted.length} field(s) → ${basename(output)}`,
        };
        input = output;
        await runInspect(output);
      } else {
        status = { kind: "err", msg: res.message };
      }
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }
</script>

<section class="panel" aria-labelledby="quill-designer-heading">
  <header class="head">
    <h1 id="quill-designer-heading">Quill Designer</h1>
    <p class="sub">
      Add, edit, and delete AcroForm fields on any PDF. Acrobat Pro charges
      $239/yr for this. Slab does it offline, free.
    </p>
  </header>

  <div class="row">
    <button class="btn primary" type="button" onclick={pickInput}>
      {input ? "Change PDF…" : "Pick a PDF…"}
    </button>
    <span class="path" title={input ?? ""}>
      {input ? basename(input) : "no file selected"}
    </span>
  </div>

  {#if input}
    <div class="grid">
      <!-- LEFT: Draft new fields -->
      <article class="card">
        <h2>New fields</h2>
        <div class="add-row">
          <button class="chip" type="button" onclick={addTextDraft}>+ Text</button>
          <button class="chip" type="button" onclick={addCheckboxDraft}>+ Checkbox</button>
          <button class="chip" type="button" onclick={addDropdownDraft}>+ Dropdown</button>
          <button class="chip" type="button" onclick={addSignatureDraft}>+ Signature</button>
        </div>

        {#if drafts.length === 0}
          <p class="empty">No drafts yet. Tap a chip above to start.</p>
        {:else}
          <ul class="drafts">
            {#each drafts as d, i (i)}
              <li class="draft">
                <div class="draft-row">
                  <span class="kind">{d.kind}</span>
                  <input
                    class="name-input"
                    type="text"
                    bind:value={d.name}
                    aria-label="Field name"
                  />
                  <button
                    class="btn ghost danger"
                    type="button"
                    onclick={() => removeDraft(i)}
                    aria-label="Remove draft"
                  >
                    ✕
                  </button>
                </div>
                <div class="draft-row sub-row">
                  <label>
                    Page
                    <input
                      type="number"
                      min="1"
                      bind:value={d.page}
                      class="num"
                    />
                  </label>
                  <label class="rect">
                    Rect
                    <input type="number" bind:value={d.rect[0]} class="num" />
                    <input type="number" bind:value={d.rect[1]} class="num" />
                    <input type="number" bind:value={d.rect[2]} class="num" />
                    <input type="number" bind:value={d.rect[3]} class="num" />
                  </label>
                </div>
                {#if d.kind === "text"}
                  <div class="draft-row sub-row">
                    <label>
                      <input type="checkbox" bind:checked={d.multiline} />
                      Multiline
                    </label>
                    <label>
                      Default
                      <input type="text" bind:value={d.default} class="flex" />
                    </label>
                  </div>
                {:else if d.kind === "checkbox"}
                  <div class="draft-row sub-row">
                    <label>
                      <input type="checkbox" bind:checked={d.default_checked} />
                      Default checked
                    </label>
                  </div>
                {:else if d.kind === "dropdown"}
                  <div class="draft-row sub-row">
                    <label class="flex">
                      Options (comma-separated)
                      <input
                        type="text"
                        value={d.options.join(", ")}
                        oninput={(e) => {
                          const v = (e.target as HTMLInputElement).value;
                          d.options = v.split(",").map((s) => s.trim()).filter(Boolean);
                        }}
                        class="flex"
                      />
                    </label>
                  </div>
                  <div class="draft-row sub-row">
                    <label>
                      Default
                      <input type="text" bind:value={d.default} class="flex" />
                    </label>
                    <label>
                      <input type="checkbox" bind:checked={d.editable} />
                      Editable
                    </label>
                  </div>
                {/if}
                <div class="draft-row sub-row">
                  <label>
                    <input type="checkbox" bind:checked={d.required} />
                    Required
                  </label>
                  <label>
                    <input type="checkbox" bind:checked={d.read_only} />
                    Read-only
                  </label>
                  <label class="flex">
                    Tooltip
                    <input type="text" bind:value={d.tooltip} class="flex" />
                  </label>
                </div>
              </li>
            {/each}
          </ul>
        {/if}

        <div class="actions">
          <button
            class="btn primary"
            type="button"
            onclick={applyDrafts}
            disabled={status.kind === "working" || drafts.length === 0}
          >
            Apply ({totalDrafts}) →
          </button>
        </div>
      </article>

      <!-- RIGHT: Existing fields -->
      <article class="card">
        <h2>Existing fields</h2>
        {#if !inspected}
          <p class="empty">Inspecting…</p>
        {:else if existingFields.length === 0}
          <p class="empty">
            This PDF has no AcroForm fields yet. Add some on the left.
          </p>
        {:else}
          <ul class="existing">
            {#each existingFields as f (f.name)}
              <li class="existing-row">
                <div class="existing-meta">
                  <span class="kind">{f.type}</span>
                  <span class="exname">{f.name}</span>
                  {#if f.page}<span class="page">p{f.page}</span>{/if}
                  {#if f.value}<span class="val">= {f.value}</span>{/if}
                </div>
                <div class="existing-actions">
                  <button
                    class="btn ghost"
                    type="button"
                    onclick={() => startEdit(f.name)}
                  >
                    Edit
                  </button>
                  <button
                    class="btn ghost danger"
                    type="button"
                    onclick={() => deleteField(f.name)}
                  >
                    Delete
                  </button>
                </div>
              </li>
            {/each}
          </ul>
        {/if}

        {#if editingName}
          <div class="edit-card">
            <h3>Edit "{editingName}"</h3>
            <label>
              Default value
              <input type="text" bind:value={editNewDefault} class="flex" />
            </label>
            <label>
              Tooltip
              <input type="text" bind:value={editTooltip} class="flex" />
            </label>
            <label>
              Required
              <select
                value={editRequired === null ? "" : String(editRequired)}
                onchange={(e) => {
                  const v = (e.target as HTMLSelectElement).value;
                  editRequired = v === "" ? null : v === "true";
                }}
              >
                <option value="">(no change)</option>
                <option value="true">Yes</option>
                <option value="false">No</option>
              </select>
            </label>
            <div class="actions">
              <button
                class="btn primary"
                type="button"
                onclick={commitEdit}
                disabled={status.kind === "working"}
              >
                Save edit →
              </button>
              <button
                class="btn ghost"
                type="button"
                onclick={() => (editingName = null)}
              >
                Cancel
              </button>
            </div>
          </div>
        {/if}
      </article>
    </div>
  {/if}

  {#if status.kind !== "idle"}
    <div class="status status-{status.kind}" role="status">
      {status.msg}
    </div>
  {/if}

  {#if report && (report.errors.length > 0 || report.unknown.length > 0)}
    <div class="report-errors">
      {#if report.errors.length > 0}
        <h3>Errors</h3>
        <ul>{#each report.errors as e}<li>{e}</li>{/each}</ul>
      {/if}
      {#if report.unknown.length > 0}
        <h3>Unknown fields</h3>
        <ul>{#each report.unknown as u}<li>{u}</li>{/each}</ul>
      {/if}
    </div>
  {/if}
</section>

<style>
  .panel {
    padding: 24px 28px;
    max-width: 1100px;
    margin: 0 auto;
    color: var(--text, #1b1b1f);
  }
  .head h1 {
    font-size: 22px;
    font-weight: 600;
    margin: 0 0 4px;
    letter-spacing: -0.01em;
  }
  .head .sub {
    margin: 0 0 18px;
    color: var(--text-muted, #5a5a66);
    font-size: 13px;
    line-height: 1.45;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-bottom: 18px;
  }
  .path {
    color: var(--text-muted, #5a5a66);
    font-size: 12px;
    font-family: ui-monospace, monospace;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 60ch;
  }
  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 16px;
  }
  @media (max-width: 880px) {
    .grid {
      grid-template-columns: 1fr;
    }
  }
  .card {
    background: var(--panel-bg, rgba(255, 255, 255, 0.6));
    backdrop-filter: blur(12px) saturate(120%);
    -webkit-backdrop-filter: blur(12px) saturate(120%);
    border: 1px solid var(--border, rgba(0, 0, 0, 0.08));
    border-radius: 14px;
    padding: 16px 18px;
  }
  .card h2 {
    margin: 0 0 12px;
    font-size: 14px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-muted, #5a5a66);
  }
  .add-row {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
    margin-bottom: 12px;
  }
  .chip {
    border: 1px solid var(--border, rgba(0, 0, 0, 0.1));
    background: var(--chip-bg, rgba(255, 255, 255, 0.7));
    color: var(--text, #1b1b1f);
    padding: 5px 10px;
    border-radius: 999px;
    font-size: 12px;
    cursor: pointer;
    transition: background 0.12s;
  }
  .chip:hover {
    background: var(--chip-hover, rgba(0, 0, 0, 0.05));
  }
  .empty {
    color: var(--text-muted, #5a5a66);
    font-size: 13px;
    padding: 18px 4px;
    text-align: center;
    margin: 0;
  }
  .drafts,
  .existing {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .draft {
    border: 1px solid var(--border, rgba(0, 0, 0, 0.08));
    border-radius: 10px;
    padding: 10px 12px;
    background: var(--draft-bg, rgba(255, 255, 255, 0.5));
  }
  .draft-row {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .sub-row {
    margin-top: 6px;
    font-size: 12px;
    color: var(--text-muted, #5a5a66);
  }
  .draft-row label {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .kind {
    text-transform: uppercase;
    font-size: 10px;
    letter-spacing: 0.08em;
    font-weight: 600;
    color: var(--accent, #4a59ff);
    border: 1px solid currentColor;
    border-radius: 4px;
    padding: 1px 5px;
  }
  .name-input {
    flex: 1;
    min-width: 8ch;
    padding: 4px 8px;
    font-size: 13px;
    border: 1px solid var(--border, rgba(0, 0, 0, 0.1));
    border-radius: 6px;
    background: var(--input-bg, white);
    color: inherit;
  }
  .num {
    width: 7ch;
    padding: 3px 6px;
    font-size: 12px;
    font-family: ui-monospace, monospace;
    border: 1px solid var(--border, rgba(0, 0, 0, 0.1));
    border-radius: 5px;
    background: var(--input-bg, white);
    color: inherit;
  }
  .flex {
    flex: 1;
    min-width: 0;
  }
  .rect {
    flex: 1;
  }
  .actions {
    display: flex;
    gap: 10px;
    margin-top: 14px;
    justify-content: flex-end;
  }
  .btn {
    padding: 6px 14px;
    font-size: 13px;
    font-weight: 500;
    border-radius: 8px;
    border: 1px solid transparent;
    cursor: pointer;
    transition: filter 0.12s, transform 0.06s;
  }
  .btn:active:not(:disabled) {
    transform: translateY(1px);
  }
  .btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .btn.primary {
    background: var(--accent, #4a59ff);
    color: white;
  }
  .btn.primary:hover:not(:disabled) {
    filter: brightness(1.07);
  }
  .btn.ghost {
    background: transparent;
    border-color: var(--border, rgba(0, 0, 0, 0.12));
    color: inherit;
  }
  .btn.ghost:hover:not(:disabled) {
    background: var(--ghost-hover, rgba(0, 0, 0, 0.04));
  }
  .btn.danger {
    color: #c2304a;
  }
  .btn.ghost.danger:hover:not(:disabled) {
    background: rgba(194, 48, 74, 0.08);
  }
  .existing-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    border: 1px solid var(--border, rgba(0, 0, 0, 0.06));
    border-radius: 8px;
    background: var(--draft-bg, rgba(255, 255, 255, 0.5));
  }
  .existing-meta {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    font-size: 12px;
  }
  .exname {
    font-family: ui-monospace, monospace;
    font-size: 12px;
  }
  .page,
  .val {
    color: var(--text-muted, #5a5a66);
  }
  .existing-actions {
    display: flex;
    gap: 6px;
  }
  .edit-card {
    margin-top: 14px;
    padding: 12px;
    border: 1px dashed var(--accent, #4a59ff);
    border-radius: 10px;
    background: rgba(74, 89, 255, 0.05);
  }
  .edit-card h3 {
    margin: 0 0 10px;
    font-size: 13px;
  }
  .edit-card label {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    margin-bottom: 8px;
  }
  .edit-card label input,
  .edit-card label select {
    flex: 1;
    padding: 4px 8px;
    font-size: 12px;
    border: 1px solid var(--border, rgba(0, 0, 0, 0.1));
    border-radius: 6px;
    background: var(--input-bg, white);
    color: inherit;
  }
  .status {
    margin-top: 14px;
    padding: 10px 14px;
    border-radius: 8px;
    font-size: 13px;
  }
  .status-working {
    background: rgba(74, 89, 255, 0.08);
    color: var(--accent, #4a59ff);
  }
  .status-ok {
    background: rgba(40, 160, 80, 0.1);
    color: #1e7e44;
  }
  .status-err {
    background: rgba(194, 48, 74, 0.1);
    color: #b3263e;
  }
  .report-errors {
    margin-top: 12px;
    padding: 10px 14px;
    background: rgba(194, 48, 74, 0.06);
    border: 1px solid rgba(194, 48, 74, 0.18);
    border-radius: 8px;
    font-size: 12px;
  }
  .report-errors h3 {
    margin: 4px 0;
    font-size: 12px;
  }
  .report-errors ul {
    margin: 0;
    padding-left: 18px;
  }
</style>
