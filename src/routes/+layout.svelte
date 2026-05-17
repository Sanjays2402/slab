<script lang="ts">
  import "../app.css";
  import { onMount } from "svelte";
  import { bootTheme, uiConfig } from "$lib/theme";
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
