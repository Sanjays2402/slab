<script lang="ts">
  // Beacon Glossary panel — builds a definitions list for jargon,
  // acronyms, italicised terms, and "defined on first use" phrases.
  // Workflow:
  //
  //   1. User picks (or inherits via `slab:open-recent`) a PDF.
  //   2. Auto-attempt cache load on path change. Cache hit → render
  //      instantly. Cache miss → user clicks "Build glossary".
  //   3. "Build glossary" calls `slab_beacon_build_glossary`. Result is
  //      auto-cached server-side. UI then re-renders.
  //   4. "Rebuild" → `slab_beacon_clear_glossary_cache` then a fresh
  //      build (forces the LLM pass to run again).
  //   5. Each entry has: kind badge, term + page chip, definition body,
  //      tiny "source as seen" snippet under the definition, and a
  //      "copy" button (copies "term — definition").
  //
  // Design parity with BeaconCitationsPanel: same friendly-error map,
  // same Status type, same goto-page event for the page chip.

  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { onMount } from "svelte";
  import { basename, idle, type CmdResult, type Status } from "$lib/types";

  type CandidateKind =
    | "Acronym"
    | "DefinedOnFirstUse"
    | "Italicised"
    | "CapitalisedPhrase";

  type GlossaryEntry = {
    term: string;
    definition: string;
    page: number;
    confidence: number;
    kind: CandidateKind;
    source_snippet: string;
  };

  type GlossarySummary = {
    candidates_total: number;
    accepted: number;
    rejected: number;
    kept_acronyms: number;
    kept_defined_first_use: number;
    kept_italicised: number;
    kept_capitalised_phrase: number;
  };

  type GlossaryReport = {
    entries: GlossaryEntry[];
    summary: GlossarySummary;
    model: string;
  };

  let pdfPath = $state<string | null>(null);
  let report = $state<GlossaryReport | null>(null);
  let filter = $state<"all" | CandidateKind>("all");
  let query = $state("");
  let includeLlm = $state(true);
  let status = $state<Status>(idle);
  let copiedTerm = $state<string | null>(null);

  onMount(() => {
    const onOpenRecent = async (e: Event) => {
      const d = (e as CustomEvent).detail as { path: string } | undefined;
      if (d?.path) {
        pdfPath = d.path;
        report = null;
        status = idle;
        await tryLoadCache();
      }
    };
    window.addEventListener("slab:open-recent", onOpenRecent);
    return () => window.removeEventListener("slab:open-recent", onOpenRecent);
  });

  async function pickPdf() {
    const picked = await open({
      multiple: false,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof picked !== "string") return;
    pdfPath = picked;
    report = null;
    status = idle;
    await tryLoadCache();
  }

  async function tryLoadCache() {
    if (!pdfPath) return;
    try {
      const res = await invoke<CmdResult<GlossaryReport | null>>(
        "slab_beacon_load_glossary_cache",
        { pdfPath },
      );
      if (res.kind === "ok" && res.value) {
        report = res.value;
        status = {
          kind: "ok",
          msg: `Loaded ${res.value.entries.length} terms from cache.`,
        };
      }
    } catch {
      // Best-effort cache load; silent on failure.
    }
  }

  async function build() {
    if (!pdfPath) {
      status = { kind: "err", msg: "Pick a PDF first." };
      return;
    }
    status = { kind: "working", msg: "Building glossary…" };
    try {
      const res = await invoke<CmdResult<GlossaryReport>>(
        "slab_beacon_build_glossary",
        {
          pdfPath,
          opts: {
            include_llm_pass: includeLlm,
            max_context_chars: 32000,
            max_candidates: 200,
          },
        },
      );
      if (res.kind === "ok") {
        report = res.value;
        status = {
          kind: "ok",
          msg: `Built ${report.entries.length} terms (${report.summary.candidates_total} scanned).`,
        };
      } else {
        status = { kind: "err", msg: friendly(res.message) };
      }
    } catch (e) {
      status = { kind: "err", msg: friendly(String(e)) };
    }
  }

  async function rebuild() {
    if (!pdfPath) return;
    try {
      await invoke<CmdResult<void>>("slab_beacon_clear_glossary_cache", {
        pdfPath,
      });
    } catch {
      // Continue even if clear fails; build will overwrite.
    }
    report = null;
    await build();
  }

  function friendly(raw: string): string {
    const m = raw.toLowerCase();
    if (m.includes("provider unavailable") || m.includes("connect")) {
      return "Beacon provider not reachable. Start Ollama or pick a different provider in Settings.";
    }
    if (m.includes("rate limited") || m.includes("429")) {
      return "Beacon rate-limited the request. Try again in a moment.";
    }
    return raw;
  }

  function gotoPage(page: number) {
    if (!pdfPath) return;
    window.dispatchEvent(
      new CustomEvent("slab:beacon-goto-page", {
        detail: { path: pdfPath, page },
      }),
    );
  }

  async function copyEntry(e: GlossaryEntry) {
    try {
      await navigator.clipboard.writeText(`${e.term} — ${e.definition}`);
      copiedTerm = e.term;
      setTimeout(() => {
        if (copiedTerm === e.term) copiedTerm = null;
      }, 1500);
    } catch {
      // Clipboard may be unavailable (e.g. test env); silent fail.
    }
  }

  function kindLabel(k: CandidateKind): string {
    switch (k) {
      case "Acronym":
        return "ACR";
      case "DefinedOnFirstUse":
        return "DEF";
      case "Italicised":
        return "ITAL";
      case "CapitalisedPhrase":
        return "CAP";
    }
  }

  const filtered = $derived.by(() => {
    if (!report) return [] as GlossaryEntry[];
    const q = query.trim().toLowerCase();
    return report.entries.filter((e) => {
      if (filter !== "all" && e.kind !== filter) return false;
      if (!q) return true;
      return (
        e.term.toLowerCase().includes(q) ||
        e.definition.toLowerCase().includes(q)
      );
    });
  });
</script>

<section class="panel glossary">
  <header>
    <h2>📖 Glossary</h2>
    <p class="hint">
      Auto-extract jargon, acronyms, and italicised terms with plain-English definitions.
    </p>
  </header>

  <div class="pdf-row">
    <button onclick={pickPdf}>{pdfPath ? basename(pdfPath) : "Pick PDF…"}</button>
    <label class="llm-toggle">
      <input type="checkbox" bind:checked={includeLlm} />
      LLM definitions
    </label>
    <button
      class="primary"
      onclick={build}
      disabled={!pdfPath || status.kind === "working"}
    >
      {status.kind === "working" ? "Building…" : report ? "Rebuild" : "Build glossary"}
    </button>
    {#if report}
      <button class="secondary" onclick={rebuild} disabled={status.kind === "working"}>
        ↻ Force rebuild
      </button>
    {/if}
  </div>

  {#if status.kind === "err"}
    <p class="err">{status.msg}</p>
  {/if}

  {#if report}
    <p class="summary">
      <strong>{report.summary.accepted}</strong> kept ·
      <span class="rejected">{report.summary.rejected} dropped</span> ·
      <span class="scanned">{report.summary.candidates_total} scanned</span>
      {#if report.model}<span class="model">via {report.model}</span>{/if}
    </p>

    <div class="filter-row">
      <input
        type="search"
        placeholder="Filter terms or definitions…"
        bind:value={query}
        class="search"
      />
      <div class="chips">
        <button
          class="chip-filter"
          class:active={filter === "all"}
          onclick={() => (filter = "all")}
        >
          all ({report.entries.length})
        </button>
        <button
          class="chip-filter"
          class:active={filter === "Acronym"}
          onclick={() => (filter = "Acronym")}
        >
          ACR ({report.summary.kept_acronyms})
        </button>
        <button
          class="chip-filter"
          class:active={filter === "DefinedOnFirstUse"}
          onclick={() => (filter = "DefinedOnFirstUse")}
        >
          DEF ({report.summary.kept_defined_first_use})
        </button>
        <button
          class="chip-filter"
          class:active={filter === "Italicised"}
          onclick={() => (filter = "Italicised")}
        >
          ITAL ({report.summary.kept_italicised})
        </button>
        <button
          class="chip-filter"
          class:active={filter === "CapitalisedPhrase"}
          onclick={() => (filter = "CapitalisedPhrase")}
        >
          CAP ({report.summary.kept_capitalised_phrase})
        </button>
      </div>
    </div>

    {#if filtered.length === 0}
      <p class="empty">
        {report.entries.length === 0
          ? "No terms extracted. Try a different PDF or toggle off LLM."
          : "No matches for current filter."}
      </p>
    {:else}
      <ul class="entries">
        {#each filtered as e (`${e.term}-${e.page}`)}
          <li class="entry">
            <div class="entry-head">
              <span class="kind kind-{e.kind.toLowerCase()}">{kindLabel(e.kind)}</span>
              <span class="term">{e.term}</span>
              <button class="page-chip" onclick={() => gotoPage(e.page)} title="Jump to page">
                p.{e.page}
              </button>
              <button class="copy" onclick={() => copyEntry(e)} title="Copy term + definition">
                {copiedTerm === e.term ? "✓" : "⧉"}
              </button>
            </div>
            {#if e.definition}
              <p class="definition">{e.definition}</p>
            {:else}
              <p class="definition empty-def">— no definition extracted —</p>
            {/if}
            {#if e.source_snippet}
              <p class="snippet" title="As it appears in the document">
                {e.source_snippet}
              </p>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  {/if}
</section>

<style>
  .panel.glossary {
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    overflow: auto;
  }
  header h2 {
    margin: 0;
    font-size: 18px;
  }
  .hint {
    color: var(--text-2, #888);
    font-size: 13px;
    margin: 4px 0 0;
  }
  .pdf-row {
    display: flex;
    gap: 8px;
    align-items: center;
    flex-wrap: wrap;
  }
  .llm-toggle {
    font-size: 13px;
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .primary {
    font-weight: 600;
  }
  .secondary {
    font-size: 12px;
    opacity: 0.85;
  }
  .err {
    color: var(--err, #c33);
    font-size: 13px;
    margin: 0;
  }
  .summary {
    font-size: 13px;
    color: var(--text-2, #666);
    margin: 0;
  }
  .summary .rejected {
    color: var(--warn, #c80);
  }
  .summary .scanned {
    opacity: 0.75;
  }
  .summary .model {
    opacity: 0.6;
    margin-left: 6px;
  }
  .filter-row {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .search {
    padding: 6px 10px;
    background: var(--hover, #1a1a1a);
    border: 1px solid var(--border, #2a2a2a);
    border-radius: 6px;
    color: inherit;
    font-size: 13px;
    width: 100%;
    box-sizing: border-box;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .chip-filter {
    font-size: 11px;
    padding: 3px 8px;
    background: transparent;
    border: 1px solid var(--border, #2a2a2a);
    border-radius: 4px;
    color: var(--text-2, #888);
    cursor: pointer;
  }
  .chip-filter:hover {
    background: var(--hover, #1a1a1a);
    color: inherit;
  }
  .chip-filter.active {
    background: var(--accent, #4af);
    border-color: var(--accent, #4af);
    color: white;
  }
  .empty {
    color: var(--text-2, #888);
    font-size: 13px;
    margin-top: 12px;
  }
  ul.entries {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .entry {
    padding: 10px;
    background: transparent;
    border: 1px solid var(--border, #2a2a2a);
    border-radius: 6px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .entry:hover {
    background: var(--hover, #1a1a1a);
  }
  .entry-head {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .kind {
    font-size: 10px;
    font-weight: 700;
    padding: 2px 6px;
    border-radius: 3px;
    background: var(--hover, #222);
    color: var(--text-2, #aaa);
    letter-spacing: 0.5px;
    flex-shrink: 0;
  }
  .kind-acronym {
    background: rgba(74, 170, 255, 0.15);
    color: #4af;
  }
  .kind-definedonfirstuse {
    background: rgba(34, 180, 100, 0.15);
    color: #2a8;
  }
  .kind-italicised {
    background: rgba(200, 130, 0, 0.15);
    color: #c80;
  }
  .kind-capitalisedphrase {
    background: rgba(160, 100, 200, 0.15);
    color: #a6c;
  }
  .term {
    font-weight: 600;
    font-size: 14px;
    flex: 1;
  }
  .page-chip {
    font-size: 11px;
    padding: 2px 8px;
    background: var(--hover, #222);
    border: none;
    border-radius: 4px;
    cursor: pointer;
    color: inherit;
  }
  .page-chip:hover {
    background: var(--accent, #4af);
    color: white;
  }
  .copy {
    font-size: 13px;
    padding: 2px 6px;
    background: transparent;
    border: 1px solid var(--border, #2a2a2a);
    border-radius: 4px;
    cursor: pointer;
    color: var(--text-2, #888);
  }
  .copy:hover {
    color: inherit;
    background: var(--hover, #222);
  }
  .definition {
    font-size: 13px;
    margin: 0;
    line-height: 1.4;
  }
  .definition.empty-def {
    color: var(--text-2, #888);
    font-style: italic;
  }
  .snippet {
    font-size: 11px;
    color: var(--text-2, #888);
    background: var(--hover, #1a1a1a);
    padding: 4px 8px;
    border-radius: 4px;
    margin: 4px 0 0;
    font-family: monospace;
    line-height: 1.3;
    border-left: 2px solid var(--border, #2a2a2a);
  }
</style>
