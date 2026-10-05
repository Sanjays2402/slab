<script lang="ts">
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import { basename } from "$lib/types";

  let { paths }: { paths: string[] } = $props();
  let error = $state("");
  let revealing = $state(false);

  async function reveal(path: string) {
    if (revealing) return;
    revealing = true;
    error = "";
    try { await revealItemInDir(path); }
    catch (e) { error = `Could not show the saved file: ${String(e)}`; }
    finally { revealing = false; }
  }
</script>

{#if paths.length > 0}
  <div class="saved-files">
    <div class="saved-heading">
      <span role="status">✓ {paths.length} PDF{paths.length === 1 ? "" : "s"} saved</span>
      <button onclick={() => reveal(paths[0])} disabled={revealing}>Show in folder</button>
    </div>
    <details>
      <summary>View saved files</summary>
      <ul>
        {#each paths as path, i (`${path}-${i}`)}
          <li><button class="ghost" title={path} aria-label={`Show ${basename(path)} in folder`} onclick={() => reveal(path)} disabled={revealing}>{basename(path)}</button></li>
        {/each}
      </ul>
    </details>
    {#if error}<p class="reveal-error" role="alert">{error}</p>{/if}
  </div>
{/if}

<style>
  .saved-files { padding: 14px; border: 1px solid color-mix(in srgb, var(--success) 40%, var(--border)); border-radius: var(--r-md); background: color-mix(in srgb, var(--success) 5%, var(--bg-2)); }
  .saved-heading { display: flex; align-items: center; justify-content: space-between; flex-wrap: wrap; gap: 12px; }
  .saved-heading span { color: var(--text); font-weight: 600; }
  details { margin-top: 10px; }
  summary { cursor: pointer; color: var(--text-2); font-size: 12px; }
  ul { padding: 0; margin: 8px 0 0; list-style: none; }
  li button { text-align: left; max-width: 100%; overflow-wrap: anywhere; }
  .reveal-error { color: var(--text); border-left: 3px solid var(--danger); padding-left: 12px; overflow-wrap: anywhere; }
</style>
