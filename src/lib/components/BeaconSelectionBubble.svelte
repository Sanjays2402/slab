<script lang="ts">
  // Beacon Selection Bubble — floats above any text selection in the PDF
  // reader. Five quick LLM actions: Translate / Explain / Define / Rewrite /
  // Summarize. Renders the answer in-place inside the bubble, with a
  // "Copy" button.
  //
  // Mount: included once inside ReaderPanel. We attach listeners to the
  // passed-in `host` element (the `pdfjs-container` div) — `mouseup` to
  // capture selections, `keydown(Esc)` to dismiss. The host owns the PDF
  // viewer, so any selection inside the text layer ends up on `window`.
  //
  // Why this lives in its own component:
  //   1. Keeps ReaderPanel.svelte under 2000 lines.
  //   2. Lets BeaconChatPanel (later) reuse the same UI for selection-driven
  //      "ask about this passage" calls if we ever want that.
  //
  // Backend contract: `slab_beacon_selection_action(text, action, target_lang?)`
  // → `{ content, model, action, input_chars, output_chars }`.

  import { invoke } from "@tauri-apps/api/core";
  import { onDestroy, onMount } from "svelte";
  import type { CmdResult } from "$lib/types";

  type SelectionAction =
    | "translate"
    | "explain"
    | "define"
    | "rewrite"
    | "summarize";

  type SelectionActionReply = {
    content: string;
    model: string;
    action: SelectionAction;
    input_chars: number;
    output_chars: number;
  };

  type Props = {
    /** The PDF.js container. Selection events bubble up to here. */
    host: HTMLElement | null | undefined;
  };

  let { host }: Props = $props();

  // ─── Bubble state ─────────────────────────────────────────────────────
  let visible = $state(false);
  let x = $state(0);
  let y = $state(0);
  /** The captured snippet. Frozen at selection time so the bubble survives
   *  the user clicking inside it (which clears `window.getSelection()`). */
  let snippet = $state("");
  let snippetPreview = $derived(
    snippet.length > 140 ? snippet.slice(0, 140) + "…" : snippet,
  );

  // ─── Result state ─────────────────────────────────────────────────────
  let activeAction = $state<SelectionAction | null>(null);
  let resultText = $state("");
  let resultModel = $state("");
  let busy = $state(false);
  let errorMsg = $state("");
  /** Translate is the only action that takes a target language. We default
   *  to English, but expose a tiny inline picker the first time the user
   *  hits Translate so polyglot Slab users (Sanjay's stated audience) can
   *  retarget without diving into settings. */
  let targetLang = $state("English");
  let showLangPicker = $state(false);

  const ACTIONS: { id: SelectionAction; label: string; emoji: string; hint: string }[] = [
    { id: "translate", label: "Translate", emoji: "✦", hint: "Translate to another language" },
    { id: "explain",   label: "Explain",   emoji: "?",  hint: "Plain-English explanation" },
    { id: "define",    label: "Define",    emoji: "📖", hint: "Dictionary-style definition" },
    { id: "rewrite",   label: "Rewrite",   emoji: "✎",  hint: "Clearer phrasing, same meaning" },
    { id: "summarize", label: "Summarize", emoji: "≡",  hint: "One-sentence TL;DR" },
  ];

  const LANGUAGES = [
    "English", "Spanish", "French", "German", "Italian",
    "Portuguese", "Dutch", "Japanese", "Korean", "Chinese (Simplified)",
    "Hindi", "Arabic", "Russian", "Turkish", "Polish",
  ];

  // ─── Listeners ────────────────────────────────────────────────────────
  /** Track whether the mouse is currently down inside the host. We only
   *  pop the bubble on the mouseup that ENDS a selection drag — random
   *  clicks elsewhere shouldn't trigger us. */
  let dragging = false;

  function isInsideHost(node: Node | null): boolean {
    if (!host || !node) return false;
    // Selection's anchor/focus nodes are text nodes; we walk up to the
    // nearest Element ancestor and check.
    const el = node.nodeType === 1
      ? (node as Element)
      : node.parentElement;
    return !!(el && host.contains(el));
  }

  function onMouseDown(e: MouseEvent) {
    // Ignore drags that originate inside the bubble itself — we don't want
    // clicking a button to dismiss + reopen the bubble.
    const target = e.target as Element | null;
    if (target?.closest(".beacon-sel-bubble")) return;
    dragging = true;
  }

  function onMouseUp(e: MouseEvent) {
    const target = e.target as Element | null;
    if (target?.closest(".beacon-sel-bubble")) {
      // Clicks INSIDE the bubble don't trigger re-capture.
      return;
    }
    const wasDragging = dragging;
    dragging = false;
    if (!wasDragging) {
      // Plain click (no drag) — hide bubble and bail.
      hide();
      return;
    }
    captureSelection(e);
  }

  function captureSelection(e: MouseEvent) {
    const sel = window.getSelection();
    if (!sel || sel.isCollapsed) {
      hide();
      return;
    }
    const text = sel.toString().trim();
    if (text.length < 2) {
      hide();
      return;
    }
    // Must be inside the host. We check the anchor; if the user selected
    // OUT of the host into something else, ignore.
    if (!isInsideHost(sel.anchorNode) || !isInsideHost(sel.focusNode)) {
      hide();
      return;
    }
    // Position the bubble just above the selection bounding box. We use
    // the last range's bounding rect because getSelection().getRangeAt(0)
    // is the only Range a PDF.js text-layer selection produces.
    try {
      const range = sel.getRangeAt(0);
      const rect = range.getBoundingClientRect();
      // Anchor the bubble to its top-left so its inline width can be
      // determined by CSS without us having to measure it.
      x = rect.left + window.scrollX;
      y = rect.top + window.scrollY - 8;  // 8px gap above the selection
    } catch {
      // Fall back to mouse coords if the range went weird.
      x = e.pageX;
      y = e.pageY - 8;
    }
    snippet = text;
    resultText = "";
    activeAction = null;
    errorMsg = "";
    showLangPicker = false;
    visible = true;
  }

  function hide() {
    visible = false;
    snippet = "";
    activeAction = null;
    resultText = "";
    errorMsg = "";
    showLangPicker = false;
  }

  function onKeyDown(e: KeyboardEvent) {
    if (e.key === "Escape" && visible) {
      hide();
    }
  }

  // ─── Action invocation ────────────────────────────────────────────────
  async function runAction(action: SelectionAction) {
    if (!snippet || busy) return;
    activeAction = action;
    busy = true;
    errorMsg = "";
    resultText = "";
    resultModel = "";
    try {
      const args: { text: string; action: SelectionAction; targetLang?: string } = {
        text: snippet,
        action,
      };
      if (action === "translate") {
        args.targetLang = targetLang;
      }
      const res = (await invoke("slab_beacon_selection_action", args)) as CmdResult<SelectionActionReply>;
      if (res.kind === "ok") {
        resultText = res.value.content;
        resultModel = res.value.model;
      } else {
        errorMsg = friendlyError(res.message);
      }
    } catch (e) {
      errorMsg = friendlyError(String(e));
    } finally {
      busy = false;
    }
  }

  function friendlyError(raw: string): string {
    if (/provider unavailable|connect|timeout|connection refused/i.test(raw)) {
      return "AI provider isn't reachable. Start Ollama or switch provider in Beacon settings.";
    }
    if (/rate limited|429/i.test(raw)) {
      return "AI provider is rate-limited. Wait a moment and try again.";
    }
    if (/too big|max/i.test(raw)) {
      return raw; // already user-friendly from the backend
    }
    return raw;
  }

  async function copyResult() {
    if (!resultText) return;
    try {
      await navigator.clipboard.writeText(resultText);
    } catch {
      // Clipboard may be unavailable in some Tauri contexts — silently noop.
    }
  }

  // ─── Effects ──────────────────────────────────────────────────────────
  onMount(() => {
    document.addEventListener("keydown", onKeyDown);
  });

  onDestroy(() => {
    document.removeEventListener("keydown", onKeyDown);
    if (host) {
      host.removeEventListener("mousedown", onMouseDown);
      host.removeEventListener("mouseup", onMouseUp);
    }
  });

  // Rebind host listeners when the host element changes (rare — happens on
  // initial mount once ReaderPanel's bind:this resolves).
  $effect(() => {
    if (!host) return;
    host.addEventListener("mousedown", onMouseDown);
    host.addEventListener("mouseup", onMouseUp);
    return () => {
      host.removeEventListener("mousedown", onMouseDown);
      host.removeEventListener("mouseup", onMouseUp);
    };
  });
</script>

{#if visible}
  <div
    class="beacon-sel-bubble"
    role="dialog"
    aria-label="Beacon selection actions"
    style="left: {x}px; top: {y}px;"
  >
    <header class="bubble-head">
      <span class="bubble-title">✦ Beacon</span>
      <span class="bubble-snippet" title={snippet}>"{snippetPreview}"</span>
      <button class="bubble-close" onclick={hide} title="Dismiss (Esc)" aria-label="Dismiss">×</button>
    </header>

    {#if activeAction === null}
      <!-- Action grid -->
      <div class="action-grid" role="toolbar" aria-label="Selection actions">
        {#each ACTIONS as a (a.id)}
          <button
            class="action-btn"
            onclick={() => {
              if (a.id === "translate") {
                showLangPicker = true;
              } else {
                runAction(a.id);
              }
            }}
            title={a.hint}
          >
            <span class="action-emoji" aria-hidden="true">{a.emoji}</span>
            <span class="action-label">{a.label}</span>
          </button>
        {/each}
      </div>

      {#if showLangPicker}
        <div class="lang-row">
          <label class="lang-label" for="beacon-sel-lang">Translate to</label>
          <select
            id="beacon-sel-lang"
            class="lang-select"
            bind:value={targetLang}
          >
            {#each LANGUAGES as lang (lang)}
              <option value={lang}>{lang}</option>
            {/each}
          </select>
          <button class="lang-go" onclick={() => runAction("translate")}>Translate →</button>
        </div>
      {/if}
    {:else}
      <!-- Result view -->
      <div class="result-row">
        <span class="result-action-pill">
          {ACTIONS.find((a) => a.id === activeAction)?.emoji}
          {ACTIONS.find((a) => a.id === activeAction)?.label}
          {#if activeAction === "translate"}
            <span class="result-lang">→ {targetLang}</span>
          {/if}
        </span>
        <button
          class="result-back"
          onclick={() => { activeAction = null; resultText = ""; errorMsg = ""; }}
          title="Pick a different action"
        >
          ← Back
        </button>
      </div>

      {#if busy}
        <div class="result-body busy">
          <span class="spinner" aria-hidden="true"></span>
          <span>Asking {resultModel || "the model"}…</span>
        </div>
      {:else if errorMsg}
        <div class="result-body err" role="alert">
          {errorMsg}
        </div>
      {:else if resultText}
        <div class="result-body">
          <p class="result-text">{resultText}</p>
          <div class="result-foot">
            <span class="result-model" title="Model used">{resultModel}</span>
            <button class="result-copy" onclick={copyResult} title="Copy to clipboard">Copy</button>
          </div>
        </div>
      {/if}
    {/if}
  </div>
{/if}

<style>
  .beacon-sel-bubble {
    position: absolute;
    /* Translate up so the bubble's BOTTOM sits at y (above the selection). */
    transform: translateY(-100%);
    z-index: 9999;
    min-width: 280px;
    max-width: 420px;
    background: rgba(20, 22, 28, 0.96);
    backdrop-filter: blur(18px) saturate(140%);
    -webkit-backdrop-filter: blur(18px) saturate(140%);
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 12px;
    box-shadow:
      0 10px 30px -8px rgba(0, 0, 0, 0.7),
      0 2px 4px rgba(0, 0, 0, 0.4);
    color: #e7e9ee;
    font-family: -apple-system, BlinkMacSystemFont, "SF Pro Text", system-ui, sans-serif;
    font-size: 13px;
    user-select: none;
    animation: bubble-in 90ms ease-out;
  }

  @keyframes bubble-in {
    from {
      opacity: 0;
      transform: translateY(calc(-100% + 4px));
    }
    to {
      opacity: 1;
      transform: translateY(-100%);
    }
  }

  .bubble-head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 8px 6px 10px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.07);
  }

  .bubble-title {
    font-weight: 600;
    color: #ffd166;
    flex-shrink: 0;
    font-size: 12px;
    letter-spacing: 0.02em;
  }

  .bubble-snippet {
    flex: 1 1 auto;
    color: rgba(231, 233, 238, 0.6);
    font-style: italic;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 11.5px;
  }

  .bubble-close {
    flex-shrink: 0;
    width: 22px;
    height: 22px;
    line-height: 18px;
    text-align: center;
    background: transparent;
    border: 0;
    color: rgba(255, 255, 255, 0.5);
    border-radius: 5px;
    cursor: pointer;
    font-size: 16px;
    padding: 0;
  }

  .bubble-close:hover {
    background: rgba(255, 255, 255, 0.07);
    color: #fff;
  }

  .action-grid {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    padding: 8px;
  }

  .action-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
    background: rgba(255, 255, 255, 0.04);
    border: 1px solid rgba(255, 255, 255, 0.07);
    border-radius: 7px;
    color: #e7e9ee;
    font: inherit;
    font-size: 12.5px;
    cursor: pointer;
    transition: background 0.08s ease, border-color 0.08s ease;
  }

  .action-btn:hover {
    background: rgba(255, 209, 102, 0.12);
    border-color: rgba(255, 209, 102, 0.35);
  }

  .action-emoji {
    font-size: 13px;
    color: #ffd166;
  }

  .action-label {
    font-weight: 500;
  }

  .lang-row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 8px 10px 8px;
    border-top: 1px solid rgba(255, 255, 255, 0.05);
  }

  .lang-label {
    font-size: 11.5px;
    color: rgba(231, 233, 238, 0.7);
    flex-shrink: 0;
  }

  .lang-select {
    flex: 1 1 auto;
    background: rgba(255, 255, 255, 0.05);
    border: 1px solid rgba(255, 255, 255, 0.1);
    color: #e7e9ee;
    border-radius: 6px;
    padding: 4px 6px;
    font: inherit;
    font-size: 12px;
  }

  .lang-go {
    background: #ffd166;
    color: #1a1a1a;
    border: 0;
    border-radius: 6px;
    padding: 4px 10px;
    font: inherit;
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
  }

  .lang-go:hover {
    background: #ffe19a;
  }

  .result-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 8px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.05);
  }

  .result-action-pill {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 3px 8px;
    background: rgba(255, 209, 102, 0.12);
    border: 1px solid rgba(255, 209, 102, 0.3);
    border-radius: 5px;
    font-size: 11.5px;
    color: #ffd166;
  }

  .result-lang {
    color: rgba(255, 209, 102, 0.7);
    font-weight: 400;
  }

  .result-back {
    background: transparent;
    border: 0;
    color: rgba(231, 233, 238, 0.6);
    font: inherit;
    font-size: 11.5px;
    cursor: pointer;
    padding: 2px 6px;
    border-radius: 5px;
  }

  .result-back:hover {
    background: rgba(255, 255, 255, 0.05);
    color: #fff;
  }

  .result-body {
    padding: 8px 10px 10px;
  }

  .result-body.busy {
    display: flex;
    align-items: center;
    gap: 8px;
    color: rgba(231, 233, 238, 0.7);
    font-size: 12.5px;
  }

  .result-body.err {
    color: #ff8b8b;
    font-size: 12.5px;
    line-height: 1.4;
  }

  .result-text {
    margin: 0 0 8px 0;
    line-height: 1.5;
    color: #e7e9ee;
    font-size: 13px;
    /* User CAN select the result text — they often want to copy a slice. */
    user-select: text;
    white-space: pre-wrap;
  }

  .result-foot {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
    font-size: 11px;
  }

  .result-model {
    color: rgba(231, 233, 238, 0.45);
    font-family: ui-monospace, "SF Mono", Menlo, monospace;
  }

  .result-copy {
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.1);
    color: #e7e9ee;
    border-radius: 5px;
    padding: 3px 10px;
    font: inherit;
    font-size: 11.5px;
    cursor: pointer;
  }

  .result-copy:hover {
    background: rgba(255, 255, 255, 0.1);
  }

  .spinner {
    width: 12px;
    height: 12px;
    border: 2px solid rgba(255, 209, 102, 0.25);
    border-top-color: #ffd166;
    border-radius: 50%;
    animation: spin 0.65s linear infinite;
    flex-shrink: 0;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }
</style>
