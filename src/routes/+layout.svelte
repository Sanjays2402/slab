<script lang="ts">
  import "../app.css";
  import { onMount } from "svelte";
  import { bootTheme, uiConfig } from "$lib/theme";
  import { bootKeymap } from "$lib/keymap";
  import { bootI18n } from "$lib/i18n";
  import ToastStack from "$lib/ToastStack.svelte";
  import OnboardingTour from "$lib/OnboardingTour.svelte";

  let { children } = $props();

  let tourOpen = $state(false);

  // Boot the v1.0.0 "Glass" theme system as early as possible.
  // Theme/accent/density are persisted to `~/.slab/config.toml`
  // (Tauri) or `localStorage` (browser dev). `bootTheme` is
  // idempotent and applies attributes synchronously after the
  // initial async read.
  onMount(() => {
    // Apply persisted locale + <html lang>/<dir> before anything else
    // so screen readers and `:dir(rtl)` CSS see the right values on
    // first paint. Synchronous — i18n state is purely client-side.
    bootI18n();
    void bootTheme().then(() => {
      // After the initial config load, decide whether to show the
      // onboarding tour. First-launch only — once dismissed it stays
      // dismissed unless the user re-triggers from the command palette.
      const cfg = get(uiConfig);
      if (!cfg.onboarded) {
        // Give the rest of the UI ~400ms to mount + the theme to settle
        // so the tour pops over a fully-rendered window, not a flash of
        // white.
        setTimeout(() => {
          tourOpen = true;
        }, 400);
      }
    });
    // Load the user's keymap as early as possible — global key handlers
    // mount in onMount on +page.svelte and consult `matches()`, so this
    // must complete before the first keystroke for the customised
    // bindings to take effect. Idempotent + silent on failure.
    void bootKeymap();

    // Listen for the global custom event so Settings + Command Palette
    // can re-open the tour without importing this layout.
    const handler = () => {
      tourOpen = true;
    };
    window.addEventListener("slab:show-onboarding", handler);
    return () => window.removeEventListener("slab:show-onboarding", handler);
  });

  // Local helper to read the latest store value synchronously.
  // Avoids importing svelte/store's `get` at the top — keeps the bundle
  // graph lean since this layout is on every route.
  function get<T>(store: { subscribe: (fn: (v: T) => void) => () => void }): T {
    let val!: T;
    const unsub = store.subscribe((v) => (val = v));
    unsub();
    return val;
  }
</script>

<div class="app">
  {@render children()}
</div>

<ToastStack />
<OnboardingTour bind:open={tourOpen} onClose={() => (tourOpen = false)} />

<style>
  .app {
    width: 100vw;
    height: 100vh;
    display: flex;
    overflow: hidden;
  }
</style>
