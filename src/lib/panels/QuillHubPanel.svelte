<script lang="ts">
  /**
   * v3.28.0 "Quill Hub" — one panel, four sub-tabs.
   *
   *   Detect (v3.27.0)   → propose form fields on a flat PDF
   *   Design (v3.26.0)   → draw / edit fields manually
   *   Fill   (v3.9.0)    → inspect + fill an AcroForm
   *   Batch  (v3.25.0)   → mail-merge a CSV onto a template
   *
   * Why this exists: until v3.28.0 these four Acrobat-killer features
   * lived as four scattered panels with four shortcuts the user had to
   * memorise. The Hub turns them into one product surface — a single
   * "Forms" tab in the sidebar with a tabbed shell, a shared "current
   * PDF" awareness via `$lib/quill`, and a live "Next: …" CTA that
   * always points the user at the next sensible step.
   *
   * The sub-tab panels are still the existing standalone components —
   * each keeps its own file-picker for now, but they $effect-sync their
   * current input into the shared store, which feeds the chip in the
   * header and the next-step CTA in the footer.
   */
  import { onMount } from "svelte";
  import { quill, setActiveTab, resetQuill, type QuillTab } from "$lib/quill";
  import { basename } from "$lib/types";

  import FormsPanel from "$lib/panels/FormsPanel.svelte";
  import QuillAutodetectPanel from "$lib/panels/QuillAutodetectPanel.svelte";
  import QuillDesignerPanel from "$lib/panels/QuillDesignerPanel.svelte";
  import QuillBatchPanel from "$lib/panels/QuillBatchPanel.svelte";

  const TABS: { id: QuillTab; label: string; sub: string }[] = [
    { id: "detect", label: "Detect", sub: "Find fillable spots on a flat PDF" },
    { id: "design", label: "Design", sub: "Draw fields by hand" },
    { id: "fill", label: "Fill", sub: "Type values into a form" },
    { id: "batch", label: "Batch", sub: "Merge a CSV across many copies" },
  ];

  let state = $derived($quill);

  function go(t: QuillTab) {
    setActiveTab(t);
  }

  function chip(): string {
    if (!state.input) return "no file yet";
    if (state.formsReport?.fields?.some((f) => f.value))
      return "filled, ready to batch";
    if (state.formsReport?.has_acroform)
      return `${state.formsReport.fields.length} field${state.formsReport.fields.length === 1 ? "" : "s"} ready`;
    if (state.detection && state.detection.candidates.length > 0)
      return `${state.detection.candidates.length} candidate${state.detection.candidates.length === 1 ? "" : "s"}`;
    return "flat PDF";
  }

  onMount(() => {
    // Deep-link: `?quill=detect|design|fill|batch` jumps straight to a tab.
    if (typeof window === "undefined") return;
    const params = new URLSearchParams(window.location.search);
    const q = params.get("quill") as QuillTab | null;
    if (q && TABS.some((t) => t.id === q)) setActiveTab(q);
  });

  const NEXT_LABEL: Record<QuillTab, string> = {
    detect: "Detect fields",
    design: "Design fields",
    fill: "Fill the form",
    batch: "Batch with CSV",
  };
</script>

<section class="quill-hub" data-testid="quill-hub">
  <header class="hub-head">
    <div class="hub-title">
      <span class="icon" aria-hidden="true">✎</span>
      <h2>Forms</h2>
      <span class="chip" data-testid="quill-chip">{chip()}</span>
      {#if state.input}
        <span class="file" title={state.input}>{basename(state.input)}</span>
      {/if}
    </div>
    <div class="hub-actions">
      <button
        class="ghost"
        onclick={() => resetQuill()}
        disabled={!state.input}
        title="Clear the shared file and reports"
      >
        Reset
      </button>
    </div>
  </header>

  <nav class="hub-tabs" aria-label="Forms workspace">
    {#each TABS as t}
      <button
        type="button"
        class="tab"
        class:active={state.activeTab === t.id}
        class:suggested={state.suggestedNextTab === t.id &&
          state.activeTab !== t.id}
        onclick={() => go(t.id)}
        aria-current={state.activeTab === t.id ? "page" : undefined}
        data-testid={`quill-tab-${t.id}`}
      >
        <span class="tab-label">{t.label}</span>
        <span class="tab-sub">{t.sub}</span>
      </button>
    {/each}
  </nav>

  <div class="hub-body">
    {#if state.activeTab === "detect"}
      <QuillAutodetectPanel />
    {:else if state.activeTab === "design"}
      <QuillDesignerPanel />
    {:else if state.activeTab === "fill"}
      <FormsPanel />
    {:else if state.activeTab === "batch"}
      <QuillBatchPanel />
    {/if}
  </div>

  {#if state.suggestedNextTab !== state.activeTab && state.input}
    <footer class="hub-cta" data-testid="quill-cta">
      <span class="cta-hint">Next:</span>
      <button class="primary" onclick={() => go(state.suggestedNextTab)}>
        {NEXT_LABEL[state.suggestedNextTab]} →
      </button>
      <span class="cta-sub">
        {TABS.find((t) => t.id === state.suggestedNextTab)?.sub}
      </span>
    </footer>
  {/if}
</section>

<style>
  .quill-hub {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--glass-bg, rgba(255, 255, 255, 0.02));
  }
  .hub-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 16px 24px 12px;
    border-bottom: 1px solid var(--glass-border, rgba(255, 255, 255, 0.08));
  }
  .hub-title {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
  }
  .hub-title h2 {
    margin: 0;
    font-size: 18px;
    font-weight: 600;
  }
  .icon {
    font-size: 18px;
    opacity: 0.8;
  }
  .chip {
    font-size: 11px;
    padding: 3px 8px;
    border-radius: 999px;
    background: var(--accent-soft, rgba(120, 160, 255, 0.16));
    color: var(--accent-strong, #88b0ff);
    white-space: nowrap;
  }
  .file {
    font-size: 12px;
    opacity: 0.6;
    font-family: ui-monospace, monospace;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 280px;
  }
  .hub-tabs {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 0;
    border-bottom: 1px solid var(--glass-border, rgba(255, 255, 255, 0.08));
  }
  .tab {
    padding: 14px 16px;
    background: transparent;
    border: none;
    border-bottom: 2px solid transparent;
    text-align: left;
    cursor: pointer;
    transition: background 120ms ease, border-color 120ms ease;
    color: inherit;
  }
  .tab:hover {
    background: var(--glass-hover, rgba(255, 255, 255, 0.03));
  }
  .tab.active {
    border-bottom-color: var(--accent-strong, #88b0ff);
  }
  .tab.suggested {
    box-shadow: inset 0 -2px 0 var(--accent-soft, rgba(120, 160, 255, 0.5));
  }
  .tab-label {
    display: block;
    font-weight: 600;
    font-size: 13px;
  }
  .tab-sub {
    display: block;
    font-size: 11px;
    opacity: 0.55;
    margin-top: 2px;
  }
  .hub-body {
    flex: 1;
    overflow: auto;
    min-height: 0;
  }
  .hub-cta {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 24px;
    border-top: 1px solid var(--glass-border, rgba(255, 255, 255, 0.08));
    background: var(--glass-strong, rgba(0, 0, 0, 0.2));
  }
  .cta-hint {
    font-size: 12px;
    opacity: 0.6;
  }
  .cta-sub {
    font-size: 12px;
    opacity: 0.7;
  }
  .primary {
    padding: 6px 14px;
    border-radius: 8px;
    border: 1px solid var(--accent-strong, #88b0ff);
    background: var(--accent-soft, rgba(120, 160, 255, 0.18));
    color: var(--accent-strong, #88b0ff);
    font-weight: 600;
    cursor: pointer;
  }
  .ghost {
    padding: 6px 12px;
    border-radius: 6px;
    border: 1px solid var(--glass-border, rgba(255, 255, 255, 0.1));
    background: transparent;
    color: inherit;
    cursor: pointer;
  }
  .ghost:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
</style>
