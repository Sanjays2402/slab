<script lang="ts">
  import "../app.css";
  import { onMount } from "svelte";
  import { bootTheme } from "$lib/theme";
  import ToastStack from "$lib/ToastStack.svelte";

  let { children } = $props();

  // Boot the v1.0.0 "Glass" theme system as early as possible.
  // Theme/accent/density are persisted to `~/.slab/config.toml`
  // (Tauri) or `localStorage` (browser dev). `bootTheme` is
  // idempotent and applies attributes synchronously after the
  // initial async read.
  onMount(() => {
    void bootTheme();
  });
</script>

<div class="app">
  {@render children()}
</div>

<ToastStack />

<style>
  .app {
    width: 100vw;
    height: 100vh;
    display: flex;
    overflow: hidden;
  }
</style>
