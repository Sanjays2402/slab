<script lang="ts">
  // Beacon Citations panel — scans the current PDF for inline citations,
  // extracts a structured References table, and links inline mentions to
  // their bibliography entries. Workflow:
  //
  //   1. User picks (or inherits via `slab:open-recent`) a PDF.
  //   2. Click "Scan citations" → `slab_beacon_find_citations`.
  //   3. Render references list with mention-count badges. Expanding a
  //      reference shows the inline mentions, each as a chip that fires
  //      `slab:beacon-goto-page` on click.
  //   4. Footer shows totals: "N references · M mentions · K orphans".
  //
  // Design notes:
  // - Same friendly-error mapping as BeaconChatPanel / BeaconSearchPanel.
  // - Offline toggle: "Skip LLM (regex inline only)" disables the
  //   references extraction so users without Ollama still get value.

  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { onMount } from "svelte";
  import { basename, idle, type CmdResult, type Status } from "$lib/types";

  type InlineCite = {
    page: number;
    text: string;
    key: string;
    authors_hint: string;
    year_hint: string;
  };
  type Reference = {
    key: string;
    authors: string;
    year: string;
    title: string;
    page_in_doc: number;
  };
  type CitationSummary = {
    inline_total: number;
    references_total: number;
    linked: number;
    orphans: number;
  };
  type CitationReport = {
    inline: InlineCite[];
    references: Reference[];
    summary: CitationSummary;
    model: string;
  };

  let pdfPath = $state<string | null>(null);
  let report = $state<CitationReport | null>(null);
  let includeLlm = $state(true);
  let expanded = $state<Set<string>>(new Set());
  let status = $state<Status>(idle);

  onMount(() => {
    const onOpenRecent = (e: Event) => {
      const d = (e as CustomEvent).detail as { path: string } | undefined;
      if (d?.path) {
        pdfPath = d.path;
        report = null;
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
  }

  async function scan() {
    if (!pdfPath) {
      status = { kind: "err", msg: "Pick a PDF first." };
      return;
    }
    status = { kind: "working", msg: "Scanning citations…" };
    try {
      const res = await invoke<CmdResult<CitationReport>>(
        "slab_beacon_find_citations",
        {
          pdfPath,
          opts: { include_llm_pass: includeLlm, max_context_chars: 40000 },
        },
      );
      if (res.kind === "ok") {
        report = res.value;
        status = {
          kind: "ok",
          msg: `Found ${report.summary.inline_total} cites, ${report.summary.references_total} refs.`,
        };
      } else {
        status = { kind: "err", msg: friendly(res.message) };
      }
    } catch (e) {
      status = { kind: "err", msg: friendly(String(e)) };
    }
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

  function toggle(key: string) {
    if (expanded.has(key)) {
      expanded.delete(key);
    } else {
      expanded.add(key);
    }
    expanded = new Set(expanded);
  }

  function mentionsOf(key: string): InlineCite[] {
    if (!report) return [];
    return report.inline.filter((c) => c.key === key);
  }

  function orphanCites(): InlineCite[] {
    if (!report) return [];
    const refs = report.references;
    return report.inline.filter((c) => !refs.some((r) => r.key === c.key));
  }

  function gotoPage(page: number) {
    if (!pdfPath) return;
    window.dispatchEvent(
      new CustomEvent("slab:beacon-goto-page", {
        detail: { path: pdfPath, page },
      }),
    );
  }
</script>

<section class="panel citations">
  <header>
    <h2>📑 Citations</h2>
    <p class="hint">Find inline citations and link them to the bibliography.</p>
  </header>

  <div class="pdf-row">
    <button onclick={pickPdf}>{pdfPath ? basename(pdfPath) : "Pick PDF…"}</button>
    <label class="llm-toggle">
      <input type="checkbox" bind:checked={includeLlm} />
      Extract bibliography (LLM)
    </label>
    <button class="primary" onclick={scan} disabled={!pdfPath || status.kind === "working"}>
      {status.kind === "working" ? "Scanning…" : "Scan citations"}
    </button>
  </div>

  {#if status.kind === "err"}
    <p class="err">{status.msg}</p>
  {/if}

  {#if report}
    <p class="summary">
      <strong>{report.summary.references_total}</strong> references ·
      <strong>{report.summary.inline_total}</strong> inline ·
      <span class="linked">{report.summary.linked} linked</span> ·
      <span class="orphans">{report.summary.orphans} orphans</span>
      {#if report.model}<span class="model">via {report.model}</span>{/if}
    </p>

    {#if report.references.length === 0 && report.inline.length === 0}
      <p class="empty">No citations or references detected.</p>
    {/if}

    {#if report.references.length > 0}
      <ul class="refs">
        {#each report.references as r (r.key)}
          {@const mentions = mentionsOf(r.key)}
          <li>
            <div class="ref-row" role="button" tabindex="0"
              onclick={() => toggle(r.key)}
              onkeydown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); toggle(r.key); } }}
              aria-expanded={expanded.has(r.key)}>
              <span class="badge">{mentions.length}</span>
              <span class="ref-text">
                <strong>{r.authors || "Unknown"}</strong>
                {#if r.year}<span class="year">({r.year})</span>{/if}
                <span class="title">{r.title}</span>
              </span>
              <button
                class="jump"
                onclick={(e) => { e.stopPropagation(); gotoPage(r.page_in_doc); }}
                title="Jump to bibliography page">
                p.{r.page_in_doc} →
              </button>
            </div>
            {#if expanded.has(r.key)}
              <ul class="mentions">
                {#each mentions as m (`${m.page}-${m.text}`)}
                  <li>
                    <button class="chip" onclick={() => gotoPage(m.page)}>
                      <code>{m.text}</code> · p.{m.page}
                    </button>
                  </li>
                {/each}
                {#if mentions.length === 0}
                  <li class="no-mentions">no inline mentions found</li>
                {/if}
              </ul>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}

    {#if report.summary.orphans > 0}
      {@const orphans = orphanCites()}
      <details class="orphans-block">
        <summary>{report.summary.orphans} orphan inline cite(s) — no matching reference</summary>
        <ul class="mentions">
          {#each orphans as c (`${c.page}-${c.text}`)}
            <li>
              <button class="chip orphan" onclick={() => gotoPage(c.page)}>
                <code>{c.text}</code> · p.{c.page}
              </button>
            </li>
          {/each}
        </ul>
      </details>
    {/if}
  {/if}
</section>

<style>
  .panel.citations {
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
  .summary .linked { color: var(--ok, #2a8); }
  .summary .orphans { color: var(--warn, #c80); }
  .summary .model { opacity: 0.6; margin-left: 6px; }
  .empty {
    color: var(--text-2, #888);
    font-size: 13px;
    margin-top: 12px;
  }
  ul.refs {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .ref-row {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px;
    background: transparent;
    border: 1px solid var(--border, #2a2a2a);
    border-radius: 6px;
    text-align: left;
    cursor: pointer;
  }
  .ref-row:hover { background: var(--hover, #1a1a1a); }
  .badge {
    background: var(--accent, #4af);
    color: white;
    border-radius: 9999px;
    font-size: 11px;
    padding: 2px 8px;
    flex-shrink: 0;
    font-weight: 600;
  }
  .ref-text {
    flex: 1;
    font-size: 13px;
  }
  .ref-text .year {
    color: var(--text-2, #888);
    margin-left: 4px;
  }
  .ref-text .title {
    display: block;
    color: var(--text-2, #aaa);
    font-size: 12px;
    margin-top: 2px;
  }
  .jump {
    font-size: 11px;
    padding: 2px 8px;
    background: var(--hover, #222);
    border: none;
    border-radius: 4px;
    cursor: pointer;
    color: inherit;
  }
  .jump:hover { background: var(--accent, #4af); color: white; }
  ul.mentions {
    list-style: none;
    margin: 6px 0 6px 32px;
    padding: 0;
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .chip {
    background: var(--hover, #222);
    border: 1px solid var(--border, #2a2a2a);
    border-radius: 4px;
    padding: 4px 8px;
    cursor: pointer;
    font-size: 12px;
    color: inherit;
  }
  .chip:hover { background: var(--accent, #4af); color: white; }
  .chip.orphan { border-color: var(--warn, #c80); }
  .no-mentions {
    font-size: 12px;
    color: var(--text-2, #888);
    padding: 4px 0;
  }
  .orphans-block summary {
    cursor: pointer;
    font-size: 13px;
    color: var(--warn, #c80);
  }
</style>
