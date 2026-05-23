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
  import { vimEnabled } from "$lib/vim/mode";
  import { LOCALES, locale, setLocale, t, tStore, type LocaleId } from "$lib/i18n";
  import { bootKeymap, keymapView, prettyBindingFor } from "$lib/keymap";
  import { onMount } from "svelte";

  // Local mirror of the current locale so the segmented control re-renders.
  let currentLocale = $state<LocaleId>("en");
  $effect(() => {
    const unsub = locale.subscribe((v) => (currentLocale = v));
    return unsub;
  });
  function chooseLocale(id: LocaleId) {
    setLocale(id);
    const label = LOCALES.find((l) => l.id === id)?.label ?? id;
    // Use t() (one-shot, reads get(locale)) so the toast is already
    // in the newly-selected language.
    notify.success(t("toast.localeChanged", { label }));
  }

  // Local mirror of the vim-enabled store so the segmented control reflects it.
  let vimOn = $state(false);
  $effect(() => {
    const unsub = vimEnabled.subscribe((v) => (vimOn = v));
    return unsub;
  });
  function setVimEnabled(on: boolean) {
    vimEnabled.set(on);
    notify.success(on ? t("vim.enabled") : t("vim.disabled"), {
      detail: on ? t("vim.enabled.detail") : undefined,
    });
  }

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

  // Boot the keymap so the Theater section can render up-to-date
  // shortcut labels (including any user overrides) the moment the
  // panel mounts. Subscribe to the view so we re-render after every
  // rebind without manually wiring a derived store.
  onMount(() => {
    void bootKeymap();
  });
  $effect(() => {
    // touch the store so Svelte tracks it; the prettyBindingFor calls
    // in the template read the cache, so any rebind triggers a redraw.
    const _ = $keymapView.actions.length;
    void _;
  });

  const THEME_OPTIONS: { id: ThemeMode; label: string; hint: string }[] = [
    { id: "auto", label: "Auto", hint: "Match system appearance" },
    { id: "light", label: "Light", hint: "Always light" },
    { id: "dark", label: "Dark", hint: "Always dark" },
    { id: "white", label: "White", hint: "Pure white, max contrast" },
  ];

  const DENSITY_OPTIONS: { id: Density; label: string; hint: string }[] = [
    { id: "comfortable", label: "Comfortable", hint: "Roomy spacing (default)" },
    { id: "compact", label: "Compact", hint: "Tighter UI, more on screen" },
  ];
</script>

<section class="panel settings-panel">
  <div class="content-header">
    <h1>{$tStore("settings.title")}</h1>
    <p class="subtitle">{$tStore("settings.subtitle")}</p>
  </div>

  <!-- Language (Glass II Slice 5) -->
  <div class="row">
    <div class="row-info">
      <h2>{$tStore("settings.language.title")}</h2>
      <p class="row-desc">{$tStore("settings.language.desc")}</p>
    </div>
    <div class="row-control">
      <div class="seg" role="radiogroup" aria-label={$tStore("settings.language.title")}>
        {#each LOCALES as opt (opt.id)}
          <button
            type="button"
            role="radio"
            aria-checked={currentLocale === opt.id}
            class:tab-active={currentLocale === opt.id}
            lang={opt.id}
            onclick={() => chooseLocale(opt.id)}
          >
            {opt.label}
          </button>
        {/each}
      </div>
    </div>
  </div>

  <!-- Theme -->
  <div class="row">
    <div class="row-info">
      <h2>{$tStore("settings.theme.title")}</h2>
      <p class="row-desc">{$tStore("settings.theme.desc")}</p>
    </div>
    <div class="row-control">
      <div class="seg" role="radiogroup" aria-label={$tStore("settings.theme.title")}>
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
      <h2>{$tStore("settings.accent.title")}</h2>
      <p class="row-desc">{$tStore("settings.accent.desc")}</p>
    </div>
    <div class="row-control">
      <div class="swatches" role="radiogroup" aria-label={$tStore("settings.accent.title")}>
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
      <h2>{$tStore("settings.density.title")}</h2>
      <p class="row-desc">{$tStore("settings.density.desc")}</p>
    </div>
    <div class="row-control">
      <div class="seg" role="radiogroup" aria-label={$tStore("settings.density.title")}>
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

  <!-- Vim bindings (Glass II Slice 1) -->
  <div class="row">
    <div class="row-info">
      <h2>{$tStore("settings.vim.title")}</h2>
      <p class="row-desc">{$tStore("settings.vim.desc")}</p>
    </div>
    <div class="row-control">
      <div class="seg" role="radiogroup" aria-label={$tStore("settings.vim.title")}>
        <button
          type="button"
          role="radio"
          aria-checked={!vimOn}
          class:tab-active={!vimOn}
          onclick={() => setVimEnabled(false)}
        >{$tStore("settings.toggle.off")}</button>
        <button
          type="button"
          role="radio"
          aria-checked={vimOn}
          class:tab-active={vimOn}
          onclick={() => setVimEnabled(true)}
        >{$tStore("settings.toggle.on")}</button>
      </div>
    </div>
  </div>

  <!-- Help & onboarding (Glass Slice 6) -->
  <div class="row">
    <div class="row-info">
      <h2>{$tStore("settings.onboarding.title")}</h2>
      <p class="row-desc">{$tStore("settings.onboarding.desc")}</p>
    </div>
    <div class="row-control">
      <button
        type="button"
        class="ghost"
        onclick={() => window.dispatchEvent(new CustomEvent("slab:show-onboarding"))}
      >
        {$tStore("settings.onboarding.cta")}
      </button>
    </div>
  </div>

  <!-- Atlas v2.2.0 — Search section. Pure information panel for the
       MVP: the actual knobs (max results, snippet length) live in
       ~/.slab/config.toml under [library.search] and are read by the
       backend on each call; the panel exposes them as a clear
       description so curious users can hand-edit. -->
  <div class="row">
    <div class="row-info">
      <h2>{$tStore("settings.search.title")}</h2>
      <p class="row-desc">{$tStore("settings.search.desc")}</p>
      <ul class="hint-list">
        <li><kbd>⇧⌘F</kbd> {$tStore("settings.search.hint.open")}</li>
        <li><kbd>⌘K</kbd> {$tStore("settings.search.hint.palette")}</li>
        <li>{$tStore("settings.search.hint.privacy")}</li>
      </ul>
    </div>
    <div class="row-control">
      <button
        type="button"
        class="ghost"
        onclick={() => window.dispatchEvent(new CustomEvent("slab:focus-library-search"))}
      >
        {$tStore("settings.search.cta")}
      </button>
    </div>
  </div>

  <!-- Theater v2.3.0 — Slice 7 polish. Surface the presenter-mode
       shortcuts so first-time users discover them without hunting
       through Keymap. Knobs (default ink colour, second-display
       preference) live in `~/.slab/config.toml` for v2.3.1; the panel
       here is an information surface + a CTA that opens the panel. -->
  <div class="row">
    <div class="row-info">
      <h2>{$tStore("settings.theater.title")}</h2>
      <p class="row-desc">{$tStore("settings.theater.desc")}</p>
      <ul class="hint-list">
        <li><kbd>{prettyBindingFor("theater.start")}</kbd> {$tStore("settings.theater.hint.start")}</li>
        <li><kbd>{prettyBindingFor("theater.next")}</kbd> {$tStore("settings.theater.hint.next")}</li>
        <li><kbd>{prettyBindingFor("theater.prev")}</kbd> {$tStore("settings.theater.hint.prev")}</li>
        <li><kbd>{prettyBindingFor("theater.blackout")}</kbd> {$tStore("settings.theater.hint.blackout")}</li>
        <li><kbd>{prettyBindingFor("theater.ink")}</kbd> {$tStore("settings.theater.hint.ink")}</li>
        <li><kbd>{prettyBindingFor("theater.exit")}</kbd> {$tStore("settings.theater.hint.exit")}</li>
        <li class="muted">{$tStore("settings.theater.hint.customize")}</li>
      </ul>
    </div>
    <div class="row-control">
      <button
        type="button"
        class="ghost"
        onclick={() => window.dispatchEvent(new CustomEvent("slab:focus-theater"))}
      >
        {$tStore("settings.theater.cta")}
      </button>
    </div>
  </div>

  <!-- Reset + status -->
  <div class="footer-row">
    <button class="ghost" onclick={reset} type="button">{$tStore("settings.reset")}</button>
    {#if saving === "saved"}
      <span class="status ok">{$tStore("settings.status.saved")}</span>
    {:else if saving === "saving"}
      <span class="status saving">{$tStore("settings.status.saving")}</span>
    {:else if saving === "error"}
      <span class="status err">{$tStore("settings.status.error", { detail: lastError ?? "unknown error" })}</span>
    {/if}
  </div>

  <p class="config-path">
    {$tStore("settings.config.storedAt")} <code>~/.slab/config.toml</code> · <code>[ui]</code>
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
  /* Atlas v2.2.0 — bulleted hint rows inside the Search section. */
  .hint-list {
    margin: 8px 0 0;
    padding: 0;
    list-style: none;
    color: var(--text-2);
    font-size: 12px;
    line-height: 1.7;
  }
  .hint-list li {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 2px 0;
  }
  .hint-list kbd {
    display: inline-block;
    padding: 1px 6px;
    border-radius: 4px;
    background: var(--bg-3);
    color: var(--text);
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 11px;
    border: 1px solid var(--border);
    min-width: 32px;
    text-align: center;
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
