<script lang="ts">
  // Settings panel — v1.0.0 "Glass" Slice 1.
  //
  // The first user-facing customisation surface in Slab. Owns theme,
  // accent colour, and density. Backend persistence lives in
  // `slab_ui_config_write` (writes `~/.slab/config.toml [ui]`).
  //
  // Design intent (Linear / Raycast quality bar):
  //   - Each setting is a "row" with a label + description on the left
  //     and the chooser (segmented or swatch grid) on the right.
  //   - Changes apply *live* via the `theme` store — no Apply button.
  //     Linear/Arc do this; it's instantly testable. Writes back to
  //     disk happen on every change but the apply is instant.
  //   - Reset button at the bottom for "I broke something" recovery.

  import { uiConfig, setUiConfig, ACCENT_COLORS } from "$lib/theme";
  import type { ThemeMode, AccentColor, Density } from "$lib/theme";
  import { notify } from "$lib/notify";

  // Mirror the store into local state so Svelte 5 reactivity picks up
  // changes from `bootTheme` racing the panel mount.
  let cfg = $state({ theme: "auto" as ThemeMode, accent: "orange" as AccentColor, density: "comfortable" as Density });
  let saving = $state<"idle" | "saving" | "saved" | "error">("idle");
  let savedTimer: ReturnType<typeof setTimeout> | null = null;
  let lastError = $state<string | null>(null);

  // Subscribe once on mount. (Svelte stores still work in $effect.)
  $effect(() => {
    const unsub = uiConfig.subscribe((v) => {
      cfg = { ...v };
    });
    return unsub;
  });

  async function update(patch: { theme?: ThemeMode; accent?: AccentColor; density?: Density }) {
    saving = "saving";
    lastError = null;
    try {
      await setUiConfig(patch);
      saving = "saved";
      if (savedTimer) clearTimeout(savedTimer);
      savedTimer = setTimeout(() => (saving = "idle"), 1400);
    } catch (e) {
      saving = "error";
      lastError = e instanceof Error ? e.message : String(e);
      notify.error("Couldn't save settings", { detail: lastError });
    }
  }

  async function reset() {
    await update({ theme: "auto", accent: "orange", density: "comfortable" });
    if (saving === "saved") notify.success("Settings reset to defaults");
  }

  const THEME_OPTIONS: { id: ThemeMode; label: string; hint: string }[] = [
    { id: "auto", label: "Auto", hint: "Match system appearance" },
    { id: "light", label: "Light", hint: "Always light" },
    { id: "dark", label: "Dark", hint: "Always dark" },
  ];

  const DENSITY_OPTIONS: { id: Density; label: string; hint: string }[] = [
    { id: "comfortable", label: "Comfortable", hint: "Roomy spacing (default)" },
    { id: "compact", label: "Compact", hint: "Tighter UI, more on screen" },
  ];
</script>

<section class="panel settings-panel">
  <div class="content-header">
    <h1>Settings</h1>
    <p class="subtitle">Make Slab look like home. Changes apply instantly.</p>
  </div>

  <!-- Theme -->
  <div class="row">
    <div class="row-info">
      <h2>Theme</h2>
      <p class="row-desc">Choose Slab's appearance. Auto follows your operating system's day/night setting.</p>
    </div>
    <div class="row-control">
      <div class="seg" role="radiogroup" aria-label="Theme">
        {#each THEME_OPTIONS as opt (opt.id)}
          <button
            type="button"
            role="radio"
            aria-checked={cfg.theme === opt.id}
            class:tab-active={cfg.theme === opt.id}
            title={opt.hint}
            onclick={() => update({ theme: opt.id })}
          >
            {opt.label}
          </button>
        {/each}
      </div>
    </div>
  </div>

  <!-- Accent -->
  <div class="row">
    <div class="row-info">
      <h2>Accent</h2>
      <p class="row-desc">Highlight colour for buttons, links, and the current-page indicator.</p>
    </div>
    <div class="row-control">
      <div class="swatches" role="radiogroup" aria-label="Accent colour">
        {#each ACCENT_COLORS as swatch (swatch.id)}
          <button
            type="button"
            role="radio"
            aria-checked={cfg.accent === swatch.id}
            aria-label={swatch.label}
            class="swatch"
            class:swatch-active={cfg.accent === swatch.id}
            style="--swatch: {swatch.hex}"
            title={swatch.label}
            onclick={() => update({ accent: swatch.id })}
          >
            <span class="swatch-dot"></span>
            <span class="swatch-label">{swatch.label}</span>
          </button>
        {/each}
      </div>
    </div>
  </div>

  <!-- Density -->
  <div class="row">
    <div class="row-info">
      <h2>Density</h2>
      <p class="row-desc">Spacing throughout the shell. Pick Compact on small screens or when you want more on the page.</p>
    </div>
    <div class="row-control">
      <div class="seg" role="radiogroup" aria-label="Density">
        {#each DENSITY_OPTIONS as opt (opt.id)}
          <button
            type="button"
            role="radio"
            aria-checked={cfg.density === opt.id}
            class:tab-active={cfg.density === opt.id}
            title={opt.hint}
            onclick={() => update({ density: opt.id })}
          >
            {opt.label}
          </button>
        {/each}
      </div>
    </div>
  </div>

  <!-- Reset + status -->
  <div class="footer-row">
    <button class="ghost" onclick={reset} type="button">Reset to defaults</button>
    {#if saving === "saved"}
      <span class="status ok">Saved ✓</span>
    {:else if saving === "saving"}
      <span class="status saving">Saving…</span>
    {:else if saving === "error"}
      <span class="status err">Couldn't save: {lastError ?? "unknown error"}</span>
    {/if}
  </div>

  <p class="config-path">
    Stored at <code>~/.slab/config.toml</code> under <code>[ui]</code>. Hand-editable.
  </p>
</section>

<style>
  .settings-panel {
    max-width: 760px;
    padding: 32px 36px 48px;
    overflow-y: auto;
  }

  .row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 24px;
    align-items: flex-start;
    padding: 18px 0;
    border-bottom: 1px solid var(--border);
  }

  .row-info h2 {
    margin: 0 0 4px;
    font-size: 14px;
    font-weight: 600;
    color: var(--text);
  }
  .row-desc {
    margin: 0;
    color: var(--text-2);
    font-size: 12px;
    line-height: 1.5;
    max-width: 460px;
  }

  .row-control {
    display: flex;
    align-items: center;
  }

  /* Swatch grid (accent picker) */
  .swatches {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
    justify-content: flex-end;
  }
  .swatch {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    background: var(--bg-2);
    border: 1px solid var(--border);
    color: var(--text-2);
    padding: 6px 10px 6px 8px;
    border-radius: var(--r-md);
    font-size: 12px;
    cursor: pointer;
    transition: border-color 0.12s, background 0.12s;
  }
  .swatch:hover {
    border-color: var(--border-strong);
  }
  .swatch-active {
    border-color: var(--swatch);
    color: var(--text);
    background: color-mix(in oklab, var(--swatch) 12%, var(--bg-2));
  }
  .swatch-dot {
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: var(--swatch);
    box-shadow: 0 0 0 1px rgba(0, 0, 0, 0.15) inset;
  }

  .footer-row {
    display: flex;
    align-items: center;
    gap: 14px;
    margin-top: 16px;
  }

  .status {
    font-size: 12px;
    border: 1px solid var(--border);
    padding: 6px 10px;
    border-radius: 6px;
  }
  .status.ok {
    background: rgba(94, 226, 165, 0.1);
    border-color: rgba(94, 226, 165, 0.35);
    color: var(--success);
  }
  .status.saving {
    color: var(--text-2);
  }
  .status.err {
    background: rgba(255, 90, 90, 0.1);
    border-color: rgba(255, 90, 90, 0.35);
    color: var(--danger);
  }

  .config-path {
    margin-top: 28px;
    color: var(--text-3);
    font-size: 11px;
  }
  .config-path code {
    background: var(--bg-3);
    border: 1px solid var(--border);
    padding: 1px 5px;
    border-radius: 4px;
    font-family: var(--font-mono);
    font-size: 11px;
  }
</style>
