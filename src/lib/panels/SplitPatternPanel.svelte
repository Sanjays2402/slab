<script lang="ts">
  // SplitPatternPanel — chapter-aware PDF splitting.
  //
  // Two modes:
  //   (a) Regex   — supply a pattern (e.g. ^Chapter\s+\d+) and Slab finds every
  //                 page whose extracted text matches; each match starts a new
  //                 output chunk.
  //   (b) Outline — Slab walks the PDF's top-level outline (bookmarks) and
  //                 produces one chunk per heading. Works on any book that
  //                 already has a TOC.
  //
  // The "Preview" step shows which pages the current selection would land on,
  // so the user can adjust their regex before committing to a file write.

  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { idle, basename, type CmdResult, type Status } from "$lib/types";

  type Mode = "regex" | "outline";

  let input = $state<string | null>(null);
  let pageCount = $state<number | null>(null);
  let mode = $state<Mode>("regex");
  let pattern = $state("^Chapter\\s+\\d+");
  let outDir = $state<string | null>(null);
  let status = $state<Status>(idle);
  let outputs = $state<string[]>([]);
  let preview = $state<number[] | null>(null);
  let previewBusy = $state(false);

  // A couple of one-click presets that cover ~80% of real books.
  const presets: { label: string; pattern: string }[] = [
    { label: "Chapter N", pattern: "^Chapter\\s+\\d+" },
    { label: "CHAPTER N", pattern: "^CHAPTER\\s+[IVXLCDM\\d]+" },
    { label: "Part N", pattern: "^Part\\s+[IVXLCDM\\d]+" },
    { label: "Section N", pattern: "^Section\\s+\\d+" },
    { label: "Numbered headings", pattern: "^\\d+\\.\\s+[A-Z]" },
  ];

  async function pickInput() {
    const picked = await open({
      multiple: false,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof picked !== "string") return;
    input = picked;
    outputs = [];
    preview = null;
    status = idle;
    const res = await invoke<CmdResult<number>>("slab_page_count", { input: picked });
    pageCount = res.kind === "ok" ? res.value : null;
  }

  async function pickOutDir() {
    const picked = await open({ directory: true, multiple: false });
    if (typeof picked !== "string") return;
    outDir = picked;
  }

  async function runPreview() {
    if (!input) {
      status = { kind: "err", msg: "Pick a PDF first." };
      return;
    }
    preview = null;
    previewBusy = true;
    status = { kind: "working", msg: "Scanning pages…" };
    try {
      if (mode === "regex") {
        const trimmed = pattern.trim();
        if (!trimmed) {
          status = { kind: "err", msg: "Enter a regex pattern (or switch to Outline mode)." };
          return;
        }
        const res = await invoke<CmdResult<number[]>>("slab_find_matching_pages", {
          input,
          pattern: trimmed,
        });
        if (res.kind === "ok") {
          preview = res.value;
          if (res.value.length === 0) {
            status = { kind: "err", msg: "Regex matched zero pages. Try a different pattern or switch to Outline mode." };
          } else {
            const chunks = previewChunkCount(res.value);
            status = { kind: "ok", msg: `Would produce ${chunks} chunk${chunks === 1 ? "" : "s"} (matches on ${res.value.length} page${res.value.length === 1 ? "" : "s"}).` };
          }
        } else {
          status = { kind: "err", msg: friendlyRegexError(res.message) };
        }
      } else {
        const res = await invoke<CmdResult<number[]>>("slab_outline_starts", { input });
        if (res.kind === "ok") {
          preview = res.value;
          if (res.value.length === 0) {
            status = { kind: "err", msg: "This PDF has no top-level outline. Switch to Regex mode and supply a pattern." };
          } else {
            const chunks = previewChunkCount(res.value);
            status = { kind: "ok", msg: `Outline has ${res.value.length} top-level heading${res.value.length === 1 ? "" : "s"} → ${chunks} chunk${chunks === 1 ? "" : "s"}.` };
          }
        } else {
          status = { kind: "err", msg: res.message };
        }
      }
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    } finally {
      previewBusy = false;
    }
  }

  // The backend bundles "everything before the first start page" into the
  // first chunk, so chunk count = match count + (1 if matches don't start at page 1).
  function previewChunkCount(starts: number[]): number {
    if (starts.length === 0) return 0;
    if (starts[0] === 1) return starts.length;
    return starts.length + 1;
  }

  function friendlyRegexError(raw: string): string {
    const m = raw.match(/invalid regex: (.+)/i);
    if (m) return `Invalid regex: ${m[1]}`;
    return raw;
  }

  async function runSplit() {
    if (!input) {
      status = { kind: "err", msg: "Pick a PDF first." };
      return;
    }
    let dest = outDir;
    if (!dest) {
      const picked = await open({ directory: true, multiple: false });
      if (typeof picked !== "string") return;
      dest = picked;
      outDir = dest;
    }

    if (mode === "regex" && pattern.trim().length === 0) {
      status = { kind: "err", msg: "Enter a regex pattern (or switch to Outline mode)." };
      return;
    }

    status = { kind: "working", msg: "Splitting…" };
    outputs = [];

    try {
      const res = await invoke<CmdResult<string[]>>("slab_split_by_pattern", {
        input,
        pattern: mode === "regex" ? pattern.trim() : null,
        outDir: dest,
      });
      if (res.kind === "ok") {
        outputs = res.value;
        status = { kind: "ok", msg: `Wrote ${res.value.length} chunk${res.value.length === 1 ? "" : "s"} → ${dest}` };
      } else {
        status = { kind: "err", msg: mode === "regex" ? friendlyRegexError(res.message) : res.message };
      }
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }

  function usePreset(p: string) {
    pattern = p;
    preview = null;
  }

  function switchMode(m: Mode) {
    mode = m;
    preview = null;
    status = idle;
  }
</script>

<header class="content-header">
  <h1>Split by Chapter</h1>
  <p class="subtitle">
    Slice a long PDF into one file per chapter — by regex match on page text,
    or by the document's top-level outline.
  </p>
</header>

<section class="panel">
  {#if !input}
    <button class="dropzone" onclick={pickInput}>
      <span class="dz-icon">+</span>
      <span class="dz-title">Choose a PDF</span>
      <span class="dz-hint">A book, a report, anything multi-chapter.</span>
    </button>
  {:else}
    <div class="file-card">
      <div>
        <div class="file-name">{basename(input)}</div>
        <div class="file-meta">
          {#if pageCount !== null}{pageCount} page{pageCount === 1 ? "" : "s"}{:else}…{/if}
        </div>
      </div>
      <button class="ghost" onclick={pickInput}>Change</button>
    </div>

    <div class="tabs">
      <button class:tab-active={mode === "regex"} onclick={() => switchMode("regex")}>
        By regex
      </button>
      <button class:tab-active={mode === "outline"} onclick={() => switchMode("outline")}>
        By outline
      </button>
    </div>

    {#if mode === "regex"}
      <label class="field">
        <span class="field-label">Pattern (regex)</span>
        <input
          type="text"
          placeholder="^Chapter\s+\d+"
          bind:value={pattern}
          spellcheck="false"
          autocapitalize="off"
          autocorrect="off"
        />
        <span class="field-hint">
          Rust regex syntax. Slab finds every page whose extracted text matches,
          then each match starts a new chunk.
        </span>
      </label>
      <div class="presets">
        {#each presets as p}
          <button class="chip" onclick={() => usePreset(p.pattern)}>{p.label}</button>
        {/each}
      </div>
    {:else}
      <div class="hint-block">
        Splits at every top-level outline entry (chapter bookmarks). No
        regex needed — Slab just walks the existing TOC. Falls back to
        regex mode if the PDF has no outline.
      </div>
    {/if}

    <label class="field">
      <span class="field-label">Output folder</span>
      <div class="row">
        <input type="text" readonly value={outDir ?? ""} placeholder="Choose a folder…" />
        <button onclick={pickOutDir}>Browse</button>
      </div>
    </label>

    <div class="actions">
      <button
        class="ghost"
        onclick={runPreview}
        disabled={status.kind === "working" || previewBusy}
      >
        {previewBusy ? "Scanning…" : "Preview"}
      </button>
      <button
        class="primary"
        onclick={runSplit}
        disabled={status.kind === "working"}
      >
        {status.kind === "working" && !previewBusy ? "Splitting…" : "Split into chapters"}
      </button>
    </div>
  {/if}

  {#if status.kind === "ok"}
    <div class="status ok">✓ {status.msg}</div>
  {:else if status.kind === "err"}
    <div class="status err">✕ {status.msg}</div>
  {/if}

  {#if preview && preview.length > 0}
    <details class="preview-list" open>
      <summary>Chapter starts ({preview.length} match{preview.length === 1 ? "" : "es"})</summary>
      <div class="page-chips">
        {#if preview[0] !== 1}
          <span class="page-chip page-chip-implicit" title="Pages 1 to {preview[0] - 1} bundled as the cover/preface chunk.">
            p1 (cover)
          </span>
        {/if}
        {#each preview as p}
          <span class="page-chip" title="Page {p}">p{p}</span>
        {/each}
      </div>
    </details>
  {/if}

  {#if outputs.length > 0}
    <details class="output-list" open>
      <summary>{outputs.length} file{outputs.length === 1 ? "" : "s"} written</summary>
      <ul>
        {#each outputs as o}
          <li>{basename(o)}</li>
        {/each}
      </ul>
    </details>
  {/if}
</section>

<style>
  .presets {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: -8px;
    margin-bottom: 4px;
  }
  .chip {
    background: var(--bg-3);
    color: var(--text-2);
    border: 1px solid var(--border);
    padding: 4px 10px;
    border-radius: 999px;
    font-size: 11px;
  }
  .chip:hover {
    color: var(--text);
    border-color: var(--accent);
  }
  .hint-block {
    background: var(--bg-3);
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    padding: 10px 12px;
    font-size: 12px;
    color: var(--text-2);
    line-height: 1.5;
  }
  .page-chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    padding-top: 8px;
  }
  .page-chip {
    display: inline-block;
    background: var(--bg-3);
    color: var(--text);
    border: 1px solid var(--border);
    padding: 3px 8px;
    border-radius: 4px;
    font-size: 11px;
    font-family: var(--mono, monospace);
  }
  .page-chip-implicit {
    color: var(--text-3);
    font-style: italic;
  }
</style>
