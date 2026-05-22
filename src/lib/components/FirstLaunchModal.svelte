<script lang="ts">
  // First-launch self-install modal (issue #25, v2.0.3).
  //
  // Shown at most once per machine, the first time Slab is opened from
  // outside its canonical install location (e.g. Downloads on macOS,
  // /tmp on Linux, %USERPROFILE%\Downloads on Windows).
  //
  // Two CTAs:
  //   - Install Slab    → relocates to ~/Applications/Slab.app /
  //                        %LOCALAPPDATA%\Programs\Slab\Slab.exe /
  //                        ~/.local/bin/slab, registers default
  //                        handler, never asks again.
  //   - Run from here   → records the decision and never prompts again.
  //
  // No admin, no UAC, no sudo. The 420ms gold-accent settling animation
  // on the success state is the WOW moment.

  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import type { CmdResult } from "$lib/types";

  type Probe = {
    should_prompt: boolean;
    decision: "pending" | "run_from_here" | "installed";
    looks_temporary: boolean;
    canonical_install_dir: string | null;
  };

  type Props = {
    /** Called once the modal is dismissed (install done, skipped, or never shown). */
    onDismiss: () => void;
  };

  let { onDismiss }: Props = $props();

  type Phase = "probing" | "idle" | "installing" | "done" | "error";

  let phase = $state<Phase>("probing");
  let probe = $state<Probe | null>(null);
  let installedPath = $state<string | null>(null);
  let errorMsg = $state<string | null>(null);
  let settled = $state(false); // triggers gold-halo settling animation

  onMount(async () => {
    try {
      const res = await invoke<CmdResult<Probe>>("slab_first_launch_probe");
      if (res.kind === "ok") {
        probe = res.value;
        if (!res.value.should_prompt) {
          onDismiss();
          return;
        }
        phase = "idle";
      } else {
        // Probe failure → don't block app launch.
        console.warn("first_launch probe failed:", res.message);
        onDismiss();
      }
    } catch (e) {
      console.warn("first_launch probe threw:", e);
      onDismiss();
    }
  });

  async function install() {
    phase = "installing";
    errorMsg = null;
    try {
      const res = await invoke<CmdResult<string>>("slab_first_launch_install");
      if (res.kind === "ok") {
        installedPath = res.value;
        phase = "done";
        // Trigger the gold-accent settling animation after one frame so
        // CSS transitions actually fire.
        requestAnimationFrame(() => {
          settled = true;
        });
        // Auto-dismiss after the user has had a beat to read the
        // success state. 1800ms = 420ms settle + 1380ms read.
        setTimeout(() => onDismiss(), 1800);
      } else {
        errorMsg = res.message;
        phase = "error";
      }
    } catch (e) {
      errorMsg = String(e);
      phase = "error";
    }
  }

  async function runFromHere() {
    try {
      await invoke<CmdResult<null>>("slab_first_launch_skip");
    } catch (e) {
      console.warn("first_launch skip failed:", e);
    }
    onDismiss();
  }
</script>

{#if phase !== "probing" && probe?.should_prompt}
  <div class="overlay" role="dialog" aria-modal="true" aria-labelledby="fl-title">
    <div class="card" class:settled>
      <div class="icon-ring" class:done={phase === "done"}>
        <svg viewBox="0 0 64 64" aria-hidden="true">
          <defs>
            <linearGradient id="slab-icon-grad" x1="0" y1="0" x2="1" y2="1">
              <stop offset="0%" stop-color="var(--accent, #e7b349)" />
              <stop offset="100%" stop-color="var(--accent-deep, #b67e1f)" />
            </linearGradient>
          </defs>
          {#if phase === "done"}
            <path
              d="M16 33 l11 11 l21 -23"
              fill="none"
              stroke="url(#slab-icon-grad)"
              stroke-width="5"
              stroke-linecap="round"
              stroke-linejoin="round"
            />
          {:else}
            <rect
              x="14"
              y="10"
              width="36"
              height="44"
              rx="3"
              fill="none"
              stroke="url(#slab-icon-grad)"
              stroke-width="3"
            />
            <line x1="22" y1="22" x2="42" y2="22" stroke="url(#slab-icon-grad)" stroke-width="2.5" stroke-linecap="round" />
            <line x1="22" y1="32" x2="42" y2="32" stroke="url(#slab-icon-grad)" stroke-width="2.5" stroke-linecap="round" />
            <line x1="22" y1="42" x2="34" y2="42" stroke="url(#slab-icon-grad)" stroke-width="2.5" stroke-linecap="round" />
          {/if}
        </svg>
      </div>

      <h2 id="fl-title">
        {#if phase === "done"}
          You're all set.
        {:else if phase === "installing"}
          Installing Slab…
        {:else if phase === "error"}
          Install couldn't finish.
        {:else if probe?.looks_temporary}
          Looks like you're running Slab from a temporary spot.
        {:else}
          Install Slab to your apps folder?
        {/if}
      </h2>

      <p class="body">
        {#if phase === "done"}
          Slab now lives at <code>{installedPath ?? "your apps folder"}</code>.
          Open it from your launcher next time — no need to find this file again.
        {:else if phase === "installing"}
          Copying Slab into place and registering it with your system.
          No admin password needed.
        {:else if phase === "error"}
          {errorMsg ?? "Something went wrong."} You can still keep using Slab from
          where it is — just hit "Run from here" below.
        {:else}
          We'll copy Slab into <code>{probe?.canonical_install_dir ?? "your apps folder"}</code>
          so you can launch it from Spotlight, the Start Menu, or your app launcher.
          <strong>No admin password.</strong> No background processes. Reversible.
        {/if}
      </p>

      {#if phase === "idle" || phase === "error"}
        <div class="actions">
          <button type="button" class="ghost" onclick={runFromHere}>
            Run from here
          </button>
          <button type="button" class="primary" onclick={install}>
            Install Slab
          </button>
        </div>
      {:else if phase === "installing"}
        <div class="progress" aria-busy="true">
          <span class="dot" style="--i:0"></span>
          <span class="dot" style="--i:1"></span>
          <span class="dot" style="--i:2"></span>
        </div>
      {/if}

      <p class="footnote">
        Slab is free, open-source, and never sends your files to a server.
      </p>
    </div>
  </div>
{/if}

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: color-mix(in oklab, var(--bg, #1c1f22) 78%, transparent);
    backdrop-filter: blur(18px) saturate(140%);
    display: grid;
    place-items: center;
    z-index: 10000;
    animation: fade-in 220ms ease both;
  }

  @keyframes fade-in {
    from { opacity: 0; }
    to { opacity: 1; }
  }

  .card {
    width: min(440px, 92vw);
    background: var(--surface, #23262a);
    border: 1px solid color-mix(in oklab, var(--accent, #e7b349) 22%, transparent);
    border-radius: 16px;
    padding: 32px 28px 22px;
    box-shadow:
      0 30px 80px -20px rgba(0, 0, 0, 0.55),
      0 0 0 1px color-mix(in oklab, var(--accent, #e7b349) 12%, transparent);
    text-align: center;
    color: var(--fg, #e9ebef);
    transition: transform 420ms cubic-bezier(0.34, 1.56, 0.64, 1),
                box-shadow 420ms ease;
  }
  .card.settled {
    transform: scale(1.0);
    box-shadow:
      0 30px 80px -20px rgba(0, 0, 0, 0.55),
      0 0 0 2px color-mix(in oklab, var(--accent, #e7b349) 70%, transparent),
      0 0 60px -10px color-mix(in oklab, var(--accent, #e7b349) 60%, transparent);
  }

  .icon-ring {
    width: 72px;
    height: 72px;
    margin: 0 auto 18px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    background: color-mix(in oklab, var(--accent, #e7b349) 12%, transparent);
    transition: background 420ms ease;
  }
  .icon-ring.done {
    background: color-mix(in oklab, var(--accent, #e7b349) 28%, transparent);
    animation: settle 420ms cubic-bezier(0.34, 1.56, 0.64, 1) both;
  }
  .icon-ring svg {
    width: 44px;
    height: 44px;
  }

  @keyframes settle {
    0%   { transform: scale(0.6); opacity: 0; }
    60%  { transform: scale(1.08); opacity: 1; }
    100% { transform: scale(1.0);  opacity: 1; }
  }

  h2 {
    margin: 0 0 10px;
    font-size: 19px;
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  .body {
    margin: 0 0 22px;
    font-size: 14px;
    line-height: 1.55;
    color: color-mix(in oklab, var(--fg, #e9ebef) 78%, transparent);
  }
  .body code {
    background: color-mix(in oklab, var(--accent, #e7b349) 14%, transparent);
    color: var(--accent, #e7b349);
    padding: 1px 6px;
    border-radius: 4px;
    font-size: 12.5px;
    font-family: ui-monospace, "SF Mono", Menlo, monospace;
  }
  .body strong {
    color: var(--accent, #e7b349);
    font-weight: 600;
  }

  .actions {
    display: flex;
    gap: 10px;
    justify-content: center;
    margin-bottom: 18px;
  }

  button {
    font: inherit;
    border-radius: 8px;
    padding: 10px 18px;
    cursor: pointer;
    border: 1px solid transparent;
    transition: background 160ms ease, border-color 160ms ease,
                transform 80ms ease;
  }
  button:active { transform: scale(0.97); }

  .ghost {
    background: transparent;
    color: color-mix(in oklab, var(--fg, #e9ebef) 70%, transparent);
    border-color: color-mix(in oklab, var(--fg, #e9ebef) 20%, transparent);
  }
  .ghost:hover {
    border-color: color-mix(in oklab, var(--fg, #e9ebef) 40%, transparent);
    color: var(--fg, #e9ebef);
  }

  .primary {
    background: linear-gradient(135deg, var(--accent, #e7b349), var(--accent-deep, #b67e1f));
    color: #1a1308;
    font-weight: 600;
    box-shadow: 0 6px 20px -8px color-mix(in oklab, var(--accent, #e7b349) 70%, transparent);
  }
  .primary:hover {
    transform: translateY(-1px);
  }

  .progress {
    display: flex;
    gap: 6px;
    justify-content: center;
    margin-bottom: 18px;
  }
  .progress .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--accent, #e7b349);
    animation: bounce 1.1s ease-in-out infinite both;
    animation-delay: calc(var(--i) * 0.15s);
  }
  @keyframes bounce {
    0%, 80%, 100% { transform: scale(0.6); opacity: 0.5; }
    40%           { transform: scale(1.0); opacity: 1; }
  }

  .footnote {
    margin: 0;
    font-size: 11.5px;
    color: color-mix(in oklab, var(--fg, #e9ebef) 50%, transparent);
  }

  @media (prefers-reduced-motion: reduce) {
    .overlay, .icon-ring.done, .card, .progress .dot {
      animation: none !important;
      transition: none !important;
    }
  }
</style>
