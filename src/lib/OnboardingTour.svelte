<!--
  OnboardingTour — v1.0.0 "Glass" Slice 6.

  Five-step first-launch walkthrough. Mounted in +layout.svelte and shown
  automatically when `uiConfig.onboarded === false`. The user can:
    - Click "Next" / "Back" to step through
    - Click "Skip" or press Esc to dismiss
    - Click "Got it" on the final step

  All three "finish" paths set `uiConfig.onboarded = true` and persist, so
  the tour never reappears unless the user re-triggers it from the
  Command Palette ("Show onboarding tour").

  Visual: dim full-page backdrop + centered card. No DOM spotlighting —
  on first launch, the user hasn't loaded a doc yet, so there's nothing
  meaningful to highlight in place. The card itself names each feature.
-->
<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { setUiConfig } from "$lib/theme";

  type Props = {
    open: boolean;
    onClose: () => void;
  };

  let { open = $bindable(false), onClose }: Props = $props();

  type Step = {
    icon: string;
    title: string;
    body: string;
    hint?: string;
  };

  const steps: Step[] = [
    {
      icon: "🍰",
      title: "Welcome to Slab",
      body:
        "Slab is an Adobe-free PDF tool that runs entirely on your Mac. Nothing leaves your machine — every action, every AI call, every search.",
      hint: "Take 30 seconds for the tour.",
    },
    {
      icon: "▥",
      title: "Open anything",
      body:
        "Drag a PDF, Office doc, EPUB, or webpage onto the Reader. Slab converts and renders it natively. Your last 12 files stay in the Recent tray — pin the ones you want to keep around.",
      hint: "Tip: ⌘O opens the file picker.",
    },
    {
      icon: "✦",
      title: "Beacon AI — local first",
      body:
        "Chat with any open document. Summaries, semantic search, PII redaction, and translation — all running through your local Ollama server by default. Configure providers in Settings.",
      hint: "Try: Beacon AI → 'Summarize this page'.",
    },
    {
      icon: "⌘",
      title: "Command Palette",
      body:
        "Press ⌘K (or Ctrl+K) anywhere in Slab to jump to a panel, switch theme, open a recent file, or run any command. Your most-used commands float to the top.",
      hint: "Press ? for the full keyboard shortcuts overlay.",
    },
    {
      icon: "⚙",
      title: "Make it yours",
      body:
        "5 accent colors, light/dark/auto theme, comfortable or compact density — all in Settings. Every preference syncs to ~/.slab/config.toml so you can hand-edit too.",
      hint: "That's it. Press 'Got it' to dive in.",
    },
  ];

  let stepIdx = $state(0);

  $effect(() => {
    if (open) stepIdx = 0;
  });

  async function finish() {
    // Persist regardless of which path the user took.
    try {
      await setUiConfig({ onboarded: true });
    } catch {
      // Even if persistence fails, close the tour — don't trap the user.
    }
    onClose();
  }

  function next() {
    if (stepIdx < steps.length - 1) stepIdx += 1;
    else void finish();
  }

  function back() {
    if (stepIdx > 0) stepIdx -= 1;
  }

  function onKey(e: KeyboardEvent) {
    if (!open) return;
    if (e.key === "Escape") {
      e.preventDefault();
      void finish();
    } else if (e.key === "ArrowRight" || e.key === "Enter") {
      e.preventDefault();
      next();
    } else if (e.key === "ArrowLeft") {
      e.preventDefault();
      back();
    }
  }

  onMount(() => {
    window.addEventListener("keydown", onKey);
  });
  onDestroy(() => {
    window.removeEventListener("keydown", onKey);
  });

  let step = $derived(steps[stepIdx]);
</script>

{#if open}
  <div class="onb-backdrop" role="dialog" aria-modal="true" aria-labelledby="onb-title">
    <div class="onb-card">
      <div class="onb-progress" aria-hidden="true">
        {#each steps as _, i}
          <span class="onb-dot" class:active={i === stepIdx} class:done={i < stepIdx}></span>
        {/each}
      </div>
      <div class="onb-icon">{step.icon}</div>
      <h1 id="onb-title" class="onb-title">{step.title}</h1>
      <p class="onb-body">{step.body}</p>
      {#if step.hint}
        <p class="onb-hint">{step.hint}</p>
      {/if}
      <div class="onb-actions">
        <button class="onb-skip" onclick={finish}>Skip</button>
        <div class="onb-nav">
          <button class="onb-btn ghost" onclick={back} disabled={stepIdx === 0}>Back</button>
          <button class="onb-btn primary" onclick={next}>
            {stepIdx === steps.length - 1 ? "Got it" : "Next"}
          </button>
        </div>
      </div>
      <div class="onb-meta">Step {stepIdx + 1} of {steps.length} · Esc to dismiss</div>
    </div>
  </div>
{/if}

<style>
  .onb-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
    backdrop-filter: blur(8px);
    -webkit-backdrop-filter: blur(8px);
    z-index: 9000;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 24px;
    animation: onb-fade 180ms ease-out;
  }
  @keyframes onb-fade {
    from { opacity: 0; }
    to { opacity: 1; }
  }
  .onb-card {
    width: min(520px, 100%);
    background: var(--bg);
    color: var(--text);
    border: 1px solid var(--border);
    border-radius: 16px;
    padding: 32px 32px 24px;
    box-shadow: 0 32px 80px -20px rgba(0, 0, 0, 0.5);
    display: flex;
    flex-direction: column;
    gap: 14px;
    text-align: center;
    animation: onb-pop 220ms cubic-bezier(0.2, 0.9, 0.3, 1.15);
  }
  @keyframes onb-pop {
    from { transform: translateY(12px) scale(0.97); opacity: 0; }
    to { transform: translateY(0) scale(1); opacity: 1; }
  }
  .onb-progress {
    display: flex;
    justify-content: center;
    gap: 8px;
    margin-bottom: 4px;
  }
  .onb-dot {
    width: 8px;
    height: 8px;
    border-radius: 999px;
    background: var(--border);
    transition: background 160ms, transform 160ms;
  }
  .onb-dot.active {
    background: var(--accent);
    transform: scale(1.3);
  }
  .onb-dot.done {
    background: color-mix(in srgb, var(--accent) 60%, var(--border));
  }
  .onb-icon {
    font-size: 44px;
    line-height: 1;
    margin: 4px 0 6px;
  }
  .onb-title {
    font-size: 22px;
    font-weight: 600;
    margin: 0;
    color: var(--text);
  }
  .onb-body {
    font-size: 14px;
    line-height: 1.55;
    margin: 0;
    color: var(--text-2);
  }
  .onb-hint {
    font-size: 12px;
    color: var(--text-3);
    margin: -2px 0 8px;
    font-style: italic;
  }
  .onb-actions {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-top: 12px;
    gap: 8px;
  }
  .onb-nav {
    display: flex;
    gap: 8px;
  }
  .onb-btn {
    padding: 8px 18px;
    border-radius: 8px;
    font-size: 13px;
    font-weight: 500;
    border: 1px solid var(--border);
    background: var(--bg-2);
    color: var(--text);
    cursor: pointer;
    transition: border-color 120ms, background 120ms, color 120ms, transform 80ms;
  }
  .onb-btn:hover:not(:disabled) {
    border-color: var(--accent);
  }
  .onb-btn:active:not(:disabled) {
    transform: translateY(1px);
  }
  .onb-btn:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
  .onb-btn.primary {
    background: var(--accent);
    border-color: var(--accent);
    color: white;
  }
  .onb-btn.primary:hover {
    filter: brightness(1.08);
  }
  .onb-skip {
    background: transparent;
    border: 0;
    color: var(--text-3);
    font-size: 12px;
    cursor: pointer;
    padding: 6px 8px;
  }
  .onb-skip:hover {
    color: var(--text);
    text-decoration: underline;
  }
  .onb-meta {
    font-size: 11px;
    color: var(--text-3);
    margin-top: 4px;
  }
</style>
