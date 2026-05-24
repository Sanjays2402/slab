<script lang="ts">
  // v3.24.0 "Stack Pro" — three-way PDF compare.
  //
  // Pick a common-ancestor `base` PDF plus two divergent revisions (`mine`,
  // `theirs`). The backend runs two 2-way diffs against the base and merges
  // them into a `ThreeWayDiff` where every base line is classified
  // Unchanged / MineOnly / TheirsOnly / BothAgree / Conflict.
  //
  // Litera Compare charges $400/seat/yr for this exact feature. Adobe
  // Acrobat doesn't ship 3-way at all. We do, free + offline.

  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { idle, basename, type CmdResult, type Status } from "$lib/types";

  // --- Backend DTOs (mirror src-tauri/src/pdf/diff3.rs) ---
  type ThreeWayKind =
    | "unchanged"
    | "mineonly"
    | "theirsonly"
    | "bothagree"
    | "conflict";
  type ThreeWayLine = {
    kind: ThreeWayKind;
    base_line: number | null;
    mine_line: number | null;
    theirs_line: number | null;
    base_text: string;
    mine_text: string | null;
    theirs_text: string | null;
  };
  type ThreeWaySummary = {
    unchanged: number;
    mine_only: number;
    theirs_only: number;
    both_agree: number;
    conflicts: number;
  };
  type ThreeWayPage = {
    page: number;
    lines: ThreeWayLine[];
    summary: ThreeWaySummary;
  };
  type ThreeWayDiff = {
    base_path: string;
    mine_path: string;
    theirs_path: string;
    pages: ThreeWayPage[];
    total: ThreeWaySummary;
  };

  // serde produces "mineonly"/"theirsonly"/"bothagree" lowercased (rename_all
  // = "lowercase"); we normalise the type alias to match.

  let basePath = $state<string | null>(null);
  let minePath = $state<string | null>(null);
  let theirsPath = $state<string | null>(null);
  let status = $state<Status>(idle);
  let diff = $state<ThreeWayDiff | null>(null);
  let filter = $state<"all" | "conflicts">("all");
  // Track user picks for conflict resolution (purely UI state for the
  // upcoming export step — Task 5).
  let resolutions = $state<Record<string, "mine" | "theirs">>({});

  async function pickPdf(): Promise<string | null> {
    const picked = await open({
      multiple: false,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    return typeof picked === "string" ? picked : null;
  }

  async function pickBase() {
    const p = await pickPdf();
    if (p) {
      basePath = p;
      diff = null;
      resolutions = {};
      status = idle;
    }
  }
  async function pickMine() {
    const p = await pickPdf();
    if (p) {
      minePath = p;
      diff = null;
      resolutions = {};
      status = idle;
    }
  }
  async function pickTheirs() {
    const p = await pickPdf();
    if (p) {
      theirsPath = p;
      diff = null;
      resolutions = {};
      status = idle;
    }
  }

  function clearAll() {
    basePath = null;
    minePath = null;
    theirsPath = null;
    diff = null;
    resolutions = {};
    status = idle;
  }

  async function runCompare() {
    if (!basePath || !minePath || !theirsPath) {
      status = {
        kind: "err",
        msg: "Pick a base PDF, a 'mine' PDF, and a 'theirs' PDF first.",
      };
      return;
    }
    status = { kind: "working", msg: "Three-way comparing…" };
    diff = null;
    resolutions = {};
    try {
      const res = await invoke<CmdResult<ThreeWayDiff>>("slab_diff3_pdfs", {
        base: basePath,
        mine: minePath,
        theirs: theirsPath,
      });
      if (res.kind === "ok") {
        diff = res.value;
        const t = res.value.total;
        const parts: string[] = [];
        if (t.conflicts > 0) parts.push(`⚠ ${t.conflicts} conflict${t.conflicts === 1 ? "" : "s"}`);
        if (t.mine_only > 0) parts.push(`◆ ${t.mine_only} mine-only`);
        if (t.theirs_only > 0) parts.push(`◇ ${t.theirs_only} theirs-only`);
        if (t.both_agree > 0) parts.push(`✓ ${t.both_agree} agree`);
        const summary =
          parts.length === 0
            ? `${t.unchanged} unchanged · no divergences`
            : `${parts.join(" · ")} · ${t.unchanged} unchanged`;
        status = {
          kind: "ok",
          msg: `${res.value.pages.length} page(s) · ${summary}`,
        };
      } else {
        status = { kind: "err", msg: res.message };
      }
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }

  function visiblePages(d: ThreeWayDiff): ThreeWayPage[] {
    if (filter === "all") return d.pages;
    return d.pages.filter((p) => p.summary.conflicts > 0);
  }

  function visibleLines(p: ThreeWayPage): ThreeWayLine[] {
    if (filter === "all") return p.lines;
    return p.lines.filter((l) => l.kind === "conflict");
  }

  function kindLabel(k: ThreeWayKind): string {
    switch (k) {
      case "unchanged":
        return "=";
      case "mineonly":
        return "M";
      case "theirsonly":
        return "T";
      case "bothagree":
        return "✓";
      case "conflict":
        return "⚠";
    }
  }

  function resolutionKey(pageIdx: number, lineIdx: number): string {
    return `${pageIdx}:${lineIdx}`;
  }

  function chooseMine(pageIdx: number, lineIdx: number) {
    resolutions = { ...resolutions, [resolutionKey(pageIdx, lineIdx)]: "mine" };
  }
  function chooseTheirs(pageIdx: number, lineIdx: number) {
    resolutions = {
      ...resolutions,
      [resolutionKey(pageIdx, lineIdx)]: "theirs",
    };
  }

  let conflictCount = $derived(diff ? diff.total.conflicts : 0);
  let resolvedCount = $derived(Object.keys(resolutions).length);
</script>

<section class="diff3-panel">
  <header>
    <h2>Three-way Compare <span class="badge">Stack Pro</span></h2>
    <p class="hint">
      Drop a common ancestor PDF plus two divergent revisions. Slab classifies
      every base line as unchanged, mine-only, theirs-only, agreement, or
      conflict — the feature Litera Compare charges $400/seat/yr for.
    </p>
  </header>

  <div class="pickers">
    <div class="picker">
      <label>Base (common ancestor)</label>
      <div class="row">
        <button class="ghost" onclick={pickBase}>
          {basePath ? basename(basePath) : "Pick base PDF…"}
        </button>
      </div>
    </div>
    <div class="picker">
      <label>Mine</label>
      <div class="row">
        <button class="ghost" onclick={pickMine}>
          {minePath ? basename(minePath) : "Pick mine PDF…"}
        </button>
      </div>
    </div>
    <div class="picker">
      <label>Theirs</label>
      <div class="row">
        <button class="ghost" onclick={pickTheirs}>
          {theirsPath ? basename(theirsPath) : "Pick theirs PDF…"}
        </button>
      </div>
    </div>
  </div>

  <div class="actions">
    <button
      class="primary"
      onclick={runCompare}
      disabled={!basePath || !minePath || !theirsPath || status.kind === "working"}
    >
      {status.kind === "working" ? "Comparing…" : "Run three-way compare"}
    </button>
    <button class="ghost" onclick={clearAll} disabled={status.kind === "working"}>
      Clear
    </button>
    {#if diff}
      <label class="filter">
        <input
          type="radio"
          name="diff3-filter"
          value="all"
          checked={filter === "all"}
          onchange={() => (filter = "all")}
        />
        All lines
      </label>
      <label class="filter">
        <input
          type="radio"
          name="diff3-filter"
          value="conflicts"
          checked={filter === "conflicts"}
          onchange={() => (filter = "conflicts")}
        />
        Conflicts only
      </label>
    {/if}
  </div>

  {#if status.kind === "err"}
    <p class="err">{status.msg}</p>
  {:else if status.kind === "ok" || status.kind === "working"}
    <p class="status">{status.msg}</p>
  {/if}

  {#if diff}
    {#if conflictCount > 0}
      <p class="resolve-bar">
        <strong>{resolvedCount}</strong> of <strong>{conflictCount}</strong> conflict{conflictCount === 1 ? "" : "s"}
        resolved. (Choose Mine or Theirs per row; export will land in a follow-up.)
      </p>
    {/if}

    {#each visiblePages(diff) as page, pageIdx (page.page)}
      <details open class="page">
        <summary>
          <span class="page-label">Page {page.page}</span>
          <span class="page-summary">
            {#if page.summary.conflicts > 0}
              <span class="chip chip-conflict">⚠ {page.summary.conflicts} conflict{page.summary.conflicts === 1 ? "" : "s"}</span>
            {/if}
            {#if page.summary.mine_only > 0}
              <span class="chip chip-mine">◆ {page.summary.mine_only} mine</span>
            {/if}
            {#if page.summary.theirs_only > 0}
              <span class="chip chip-theirs">◇ {page.summary.theirs_only} theirs</span>
            {/if}
            {#if page.summary.both_agree > 0}
              <span class="chip chip-agree">✓ {page.summary.both_agree} agree</span>
            {/if}
            {#if page.summary.unchanged > 0}
              <span class="chip chip-equal">= {page.summary.unchanged}</span>
            {/if}
          </span>
        </summary>

        <div class="grid-head">
          <div></div>
          <div class="col-h">Base</div>
          <div class="col-h">Mine</div>
          <div class="col-h">Theirs</div>
        </div>

        {#each visibleLines(page) as line, lineIdx (`${pageIdx}-${line.base_line}-${lineIdx}`)}
          <div class="grid-row row-{line.kind}">
            <div class="kind-cell" title={line.kind}>{kindLabel(line.kind)}</div>
            <div class="cell base">{line.base_text || "—"}</div>
            <div class="cell mine">{line.mine_text ?? "—"}</div>
            <div class="cell theirs">{line.theirs_text ?? "—"}</div>
            {#if line.kind === "conflict"}
              <div class="resolve">
                <button
                  class="chip-btn"
                  class:active={resolutions[resolutionKey(pageIdx, lineIdx)] === "mine"}
                  onclick={() => chooseMine(pageIdx, lineIdx)}
                  title="Resolve in favour of Mine"
                >
                  Keep Mine
                </button>
                <button
                  class="chip-btn"
                  class:active={resolutions[resolutionKey(pageIdx, lineIdx)] === "theirs"}
                  onclick={() => chooseTheirs(pageIdx, lineIdx)}
                  title="Resolve in favour of Theirs"
                >
                  Keep Theirs
                </button>
              </div>
            {/if}
          </div>
        {/each}
      </details>
    {/each}
  {/if}
</section>

<style>
  .diff3-panel {
    padding: 20px 24px;
    max-width: 1400px;
  }
  header h2 {
    margin: 0 0 6px;
    font-size: 18px;
    display: flex;
    align-items: baseline;
    gap: 10px;
  }
  .badge {
    font-size: 11px;
    padding: 2px 8px;
    border-radius: 99px;
    background: linear-gradient(180deg, rgba(120, 80, 220, 0.18), rgba(80, 60, 200, 0.12));
    color: #b39df0;
    border: 1px solid rgba(140, 100, 240, 0.35);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  .hint {
    color: var(--text-2);
    font-size: 13px;
    margin: 0 0 18px;
    max-width: 720px;
  }
  .pickers {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 12px;
    margin-bottom: 14px;
  }
  .picker label {
    display: block;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-2);
    margin-bottom: 6px;
  }
  .picker .row button {
    width: 100%;
    text-align: left;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 14px;
    flex-wrap: wrap;
  }
  .actions .filter {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 12px;
    color: var(--text-2);
    margin-left: 8px;
  }
  button.primary {
    padding: 7px 14px;
    border-radius: 8px;
    background: var(--accent);
    color: white;
    border: 0;
    font-weight: 600;
    cursor: pointer;
  }
  button.primary:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }
  button.ghost {
    padding: 7px 12px;
    border-radius: 8px;
    background: var(--bg-3, rgba(255, 255, 255, 0.05));
    color: var(--text);
    border: 1px solid var(--border);
    cursor: pointer;
  }
  .err {
    color: var(--err, #ff7676);
    font-size: 13px;
  }
  .status {
    color: var(--text-2);
    font-size: 13px;
    margin: 8px 0 14px;
  }
  .resolve-bar {
    font-size: 13px;
    padding: 8px 12px;
    border-radius: 8px;
    background: rgba(255, 200, 80, 0.08);
    border: 1px solid rgba(255, 200, 80, 0.3);
    color: var(--text);
    margin: 6px 0 14px;
  }
  details.page {
    border: 1px solid var(--border);
    border-radius: 10px;
    margin-bottom: 12px;
    background: var(--bg-2);
    overflow: hidden;
  }
  details.page summary {
    cursor: pointer;
    padding: 10px 14px;
    display: flex;
    align-items: center;
    gap: 12px;
    list-style: none;
  }
  details.page summary::-webkit-details-marker { display: none; }
  .page-label { font-weight: 600; }
  .page-summary { display: flex; gap: 6px; flex-wrap: wrap; }
  .chip {
    font-size: 11px;
    padding: 2px 8px;
    border-radius: 99px;
    background: var(--bg-3, rgba(255, 255, 255, 0.06));
    border: 1px solid var(--border);
  }
  .chip-conflict { background: rgba(255, 100, 100, 0.16); border-color: rgba(255, 100, 100, 0.4); color: #ffb0b0; }
  .chip-mine { background: rgba(110, 180, 255, 0.14); border-color: rgba(110, 180, 255, 0.4); color: #cfe2ff; }
  .chip-theirs { background: rgba(180, 130, 255, 0.14); border-color: rgba(180, 130, 255, 0.4); color: #e2cfff; }
  .chip-agree { background: rgba(120, 220, 140, 0.14); border-color: rgba(120, 220, 140, 0.4); color: #c4f5cf; }
  .chip-equal { color: var(--text-2); }

  .grid-head, .grid-row {
    display: grid;
    grid-template-columns: 36px 1fr 1fr 1fr;
    gap: 0;
    border-top: 1px solid var(--border);
    font-size: 12.5px;
    font-family: ui-monospace, "SF Mono", Menlo, monospace;
  }
  .grid-head .col-h {
    padding: 6px 10px;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-2);
    border-left: 1px solid var(--border);
  }
  .grid-row .kind-cell {
    text-align: center;
    padding: 6px 0;
    color: var(--text-2);
  }
  .grid-row .cell {
    padding: 6px 10px;
    border-left: 1px solid var(--border);
    white-space: pre-wrap;
    word-break: break-word;
  }
  .row-conflict { background: rgba(255, 100, 100, 0.08); }
  .row-conflict .kind-cell { color: #ff8080; font-weight: 700; }
  .row-mineonly { background: rgba(110, 180, 255, 0.06); }
  .row-theirsonly { background: rgba(180, 130, 255, 0.06); }
  .row-bothagree { background: rgba(120, 220, 140, 0.05); }
  .row-unchanged { color: var(--text-2); }

  .resolve {
    grid-column: 2 / -1;
    display: flex;
    gap: 6px;
    padding: 0 10px 8px;
    border-left: 1px solid var(--border);
  }
  .chip-btn {
    font-size: 11px;
    padding: 3px 9px;
    border-radius: 99px;
    border: 1px solid var(--border);
    background: transparent;
    color: var(--text-2);
    cursor: pointer;
  }
  .chip-btn.active {
    background: var(--accent);
    color: white;
    border-color: var(--accent);
  }
</style>
