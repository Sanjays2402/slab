<script lang="ts">
  // PluginConsentModal — v2.0.0 "Workshop" Slice 5.
  //
  // First-enable consent UI for v2.0.0 runtime plugins. The plugin's
  // manifest declares the *upper bounds* of every capability axis it
  // wants (fs/net/ui/beacon + the two allow-lists). The user gets to
  // pick the actual grant, bounded by those declared maxima — they
  // can dial down, never up. Approving wires the chosen grants to
  // disk via `setPluginGrants` and lets the enable flow continue.
  // Denying writes deny-all grants (so we don't ask again until the
  // user explicitly resets) and aborts the enable.
  //
  // Pure presentational + state-management component: takes
  // `manifest`, `initial` (current grants if any), and approve/deny
  // callbacks. No knowledge of the Tauri layer.
  //
  // Visual contract mirrors `UninstallConfirmModal.svelte` and
  // `DecryptModal.svelte` — centered modal, blurred backdrop, Escape
  // dismisses (as Deny), focus traps the primary action.

  import { tStore, t } from "$lib/i18n";
  import type {
    Manifest,
    ManifestCapabilities,
    PluginGrants,
  } from "$lib/plugins";

  type Props = {
    /** Manifest of the plugin being enabled. */
    manifest: Manifest;
    /**
     * Existing grants when the user is *re-reviewing* permissions
     * (post-enable, via "Review permissions"). For the first-enable
     * flow this should be `null` and the component pre-fills with
     * the manifest's declared bounds (the maximally-useful default).
     */
    initial: PluginGrants | null;
    /** User approved — modal passes back the final grant set. */
    onApprove: (grants: PluginGrants) => void;
    /** User denied — close + abort the enable flow. */
    onDeny: () => void;
  };

  let { manifest, initial, onApprove, onDeny }: Props = $props();

  // The declared upper bounds. For declarative-only plugins
  // (no [runtime] section) capabilities are all-none and we'll
  // short-circuit to "no permissions needed".
  let declared = $derived<ManifestCapabilities>(
    manifest.runtime?.capabilities ?? {
      fs: "none",
      net: "none",
      ui: "none",
      beacon: "none",
      net_allow_hosts: [],
      fs_allow_paths: [],
    },
  );

  // Whether the plugin needs *any* permissions at all. When false,
  // the modal short-circuits to a "nothing to grant" view with a
  // single Approve button. Mirrors the Rust enforce() short-circuit.
  let hasRuntime = $derived(manifest.runtime !== null);
  let needsAnyPermission = $derived(
    hasRuntime &&
      (declared.fs !== "none" ||
        declared.net !== "none" ||
        declared.ui !== "none" ||
        declared.beacon !== "none"),
  );

  // Working copy of the grants. Initialized to either the existing
  // user decision (re-review path) or to the declared bounds
  // (first-enable path — give the plugin everything it asked for
  // so it works out of the box; the user can dial down if they want).
  let fs = $state<PluginGrants["fs"]>(initial?.fs ?? declared.fs);
  let net = $state<PluginGrants["net"]>(initial?.net ?? declared.net);
  let ui = $state<PluginGrants["ui"]>(initial?.ui ?? declared.ui);
  let beacon = $state<PluginGrants["beacon"]>(initial?.beacon ?? declared.beacon);
  let netAllowHosts = $state<string[]>(
    initial?.net_allow_hosts ?? [...declared.net_allow_hosts],
  );
  let fsAllowPaths = $state<string[]>(
    initial?.fs_allow_paths ?? [...declared.fs_allow_paths],
  );

  let approveBtn: HTMLButtonElement | undefined = $state();

  // Focus the primary action on mount. Unlike the uninstall modal
  // this is non-destructive, so focusing Approve is the natural
  // "you're done reading, hit Enter" affordance.
  $effect(() => {
    approveBtn?.focus();
  });

  // ---- Allowed-value computation per axis ------------------------
  //
  // The user can pick any value at or below the declared bound. We
  // enumerate the lattice for each axis and filter to the prefix
  // up to (and including) the declared max. Order matters — these
  // arrays are written low-to-high, which is also how they render.

  const FS_ORDER = ["none", "read", "read-write"] as const;
  const NET_ORDER = ["none", "specific", "any"] as const;
  const UI_ORDER = ["none", "panel", "tool", "both"] as const;
  const BEACON_ORDER = ["none", "tool-provider", "ai-provider", "both"] as const;

  function allowedValues<T extends readonly string[]>(order: T, max: T[number]): T[number][] {
    const idx = order.indexOf(max);
    if (idx < 0) return [order[0]];
    return order.slice(0, idx + 1) as unknown as T[number][];
  }

  let allowedFs = $derived(allowedValues(FS_ORDER, declared.fs));
  let allowedNet = $derived(allowedValues(NET_ORDER, declared.net));
  let allowedUi = $derived(allowedValues(UI_ORDER, declared.ui));
  let allowedBeacon = $derived(allowedValues(BEACON_ORDER, declared.beacon));

  // When the user picks "none" for fs/net, the corresponding
  // allow-list is meaningless — collapse it from the UI so we don't
  // ask them to manage hosts/paths that won't be used.
  let showHostsList = $derived(net !== "none" && declared.net_allow_hosts.length > 0);
  let showPathsList = $derived(fs !== "none" && declared.fs_allow_paths.length > 0);

  function approve() {
    // When the user picks "none" for an axis, scrub the allow-list
    // for that axis to deny-all on disk. Keeps the grants file
    // tidy and the enforce() path correct (no stale hosts/paths
    // matter when the gate is fully closed).
    const grants: PluginGrants = {
      fs,
      net,
      ui,
      beacon,
      net_allow_hosts: net === "none" ? [] : netAllowHosts,
      fs_allow_paths: fs === "none" ? [] : fsAllowPaths,
    };
    onApprove(grants);
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      onDeny();
    }
  }

  function valueLabel(v: string): string {
    return t(`plugins.consent.value.${v}` as const);
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div
  class="backdrop"
  role="dialog"
  aria-modal="true"
  aria-labelledby="consent-title"
  onclick={(e) => {
    if (e.target === e.currentTarget) onDeny();
  }}
  onkeydown={(e) => {
    if (e.key === "Escape") onDeny();
  }}
  tabindex="-1"
>
  <div class="modal" role="document">
    <header>
      <div class="lock-icon" aria-hidden="true">🔐</div>
      <div>
        <h1 id="consent-title">
          {t("plugins.consent.title", { name: manifest.name })}
        </h1>
        <p class="subtitle">{$tStore("plugins.consent.subtitle")}</p>
      </div>
    </header>

    {#if !needsAnyPermission}
      <div class="no-runtime">
        <p>{$tStore("plugins.consent.noRuntime")}</p>
      </div>
    {:else}
      <div class="caps">
        <!-- FS -->
        {#if declared.fs !== "none"}
          <div class="cap-row">
            <div class="cap-head">
              <h2>{$tStore("plugins.consent.permission.fs")}</h2>
              <p class="cap-desc">{$tStore("plugins.consent.permission.fs.desc")}</p>
            </div>
            <div class="cap-control">
              <div class="seg" role="radiogroup" aria-label="fs">
                {#each allowedFs as v}
                  <button
                    type="button"
                    role="radio"
                    aria-checked={fs === v}
                    class:tab-active={fs === v}
                    onclick={() => (fs = v)}>{valueLabel(v)}</button
                  >
                {/each}
              </div>
              <p class="declared-hint">
                {t("plugins.consent.declaredHint", { bound: valueLabel(declared.fs) })}
              </p>
            </div>
          </div>
          {#if showPathsList}
            <div class="allow-list">
              <dt>{$tStore("plugins.consent.allowPaths.label")}</dt>
              <dd>
                {#if fsAllowPaths.length === 0}
                  <span class="muted">{$tStore("plugins.consent.allowList.empty")}</span>
                {:else}
                  <ul>
                    {#each fsAllowPaths as p}
                      <li class="mono">{p}</li>
                    {/each}
                  </ul>
                {/if}
              </dd>
            </div>
          {/if}
        {/if}

        <!-- NET -->
        {#if declared.net !== "none"}
          <div class="cap-row">
            <div class="cap-head">
              <h2>{$tStore("plugins.consent.permission.net")}</h2>
              <p class="cap-desc">{$tStore("plugins.consent.permission.net.desc")}</p>
            </div>
            <div class="cap-control">
              <div class="seg" role="radiogroup" aria-label="net">
                {#each allowedNet as v}
                  <button
                    type="button"
                    role="radio"
                    aria-checked={net === v}
                    class:tab-active={net === v}
                    onclick={() => (net = v)}>{valueLabel(v)}</button
                  >
                {/each}
              </div>
              <p class="declared-hint">
                {t("plugins.consent.declaredHint", { bound: valueLabel(declared.net) })}
              </p>
            </div>
          </div>
          {#if showHostsList}
            <div class="allow-list">
              <dt>{$tStore("plugins.consent.allowHosts.label")}</dt>
              <dd>
                {#if netAllowHosts.length === 0}
                  <span class="muted">{$tStore("plugins.consent.allowList.empty")}</span>
                {:else}
                  <ul>
                    {#each netAllowHosts as h}
                      <li class="mono">{h}</li>
                    {/each}
                  </ul>
                {/if}
              </dd>
            </div>
          {/if}
        {/if}

        <!-- UI -->
        {#if declared.ui !== "none"}
          <div class="cap-row">
            <div class="cap-head">
              <h2>{$tStore("plugins.consent.permission.ui")}</h2>
              <p class="cap-desc">{$tStore("plugins.consent.permission.ui.desc")}</p>
            </div>
            <div class="cap-control">
              <div class="seg" role="radiogroup" aria-label="ui">
                {#each allowedUi as v}
                  <button
                    type="button"
                    role="radio"
                    aria-checked={ui === v}
                    class:tab-active={ui === v}
                    onclick={() => (ui = v)}>{valueLabel(v)}</button
                  >
                {/each}
              </div>
              <p class="declared-hint">
                {t("plugins.consent.declaredHint", { bound: valueLabel(declared.ui) })}
              </p>
            </div>
          </div>
        {/if}

        <!-- BEACON -->
        {#if declared.beacon !== "none"}
          <div class="cap-row">
            <div class="cap-head">
              <h2>{$tStore("plugins.consent.permission.beacon")}</h2>
              <p class="cap-desc">{$tStore("plugins.consent.permission.beacon.desc")}</p>
            </div>
            <div class="cap-control">
              <div class="seg" role="radiogroup" aria-label="beacon">
                {#each allowedBeacon as v}
                  <button
                    type="button"
                    role="radio"
                    aria-checked={beacon === v}
                    class:tab-active={beacon === v}
                    onclick={() => (beacon = v)}>{valueLabel(v)}</button
                  >
                {/each}
              </div>
              <p class="declared-hint">
                {t("plugins.consent.declaredHint", { bound: valueLabel(declared.beacon) })}
              </p>
            </div>
          </div>
        {/if}
      </div>
    {/if}

    <div class="actions">
      <button type="button" class="ghost" onclick={onDeny}>
        {$tStore("plugins.consent.deny")}
      </button>
      <button bind:this={approveBtn} type="button" class="primary" onclick={approve}>
        {$tStore("plugins.consent.approve")}
      </button>
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 120;
    background: rgba(0, 0, 0, 0.55);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 24px;
    backdrop-filter: blur(2px);
  }
  .modal {
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: var(--r-md, 10px);
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.4);
    max-width: 560px;
    width: 100%;
    max-height: 88vh;
    overflow-y: auto;
    padding: 20px 22px 18px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  header {
    display: flex;
    gap: 12px;
    align-items: flex-start;
  }
  .lock-icon {
    font-size: 26px;
    line-height: 1;
    margin-top: 2px;
  }
  h1 {
    font-size: 16px;
    margin: 0;
    font-weight: 600;
  }
  .subtitle {
    font-size: 13px;
    color: var(--text-3);
    margin: 4px 0 0;
    line-height: 1.4;
  }
  .no-runtime {
    padding: 14px 16px;
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: var(--r-sm, 6px);
    font-size: 13px;
    color: var(--text-2);
  }
  .caps {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .cap-row {
    display: grid;
    grid-template-columns: 1fr;
    gap: 10px;
    padding: 14px 14px 12px;
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: var(--r-sm, 6px);
  }
  .cap-head h2 {
    font-size: 14px;
    margin: 0;
    font-weight: 600;
  }
  .cap-desc {
    font-size: 12px;
    color: var(--text-3);
    margin: 3px 0 0;
    line-height: 1.4;
  }
  .cap-control {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .seg {
    display: inline-flex;
    border: 1px solid var(--border);
    border-radius: var(--r-sm, 6px);
    overflow: hidden;
    align-self: flex-start;
  }
  .seg button {
    font: inherit;
    font-size: 12px;
    padding: 6px 10px;
    background: transparent;
    color: var(--text-2);
    border: 0;
    border-right: 1px solid var(--border);
    cursor: pointer;
  }
  .seg button:last-child {
    border-right: 0;
  }
  .seg button:hover:not(.tab-active) {
    background: var(--bg-3, var(--bg-1));
  }
  .seg button.tab-active {
    background: var(--accent, #3a8bff);
    color: var(--accent-fg, white);
    font-weight: 600;
  }
  .declared-hint {
    font-size: 11px;
    color: var(--text-3);
    margin: 0;
  }
  .allow-list {
    margin: -6px 0 0 12px;
    padding: 8px 12px;
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: var(--r-sm, 6px);
    font-size: 12px;
  }
  .allow-list dt {
    color: var(--text-3);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    font-size: 10px;
    margin-bottom: 4px;
  }
  .allow-list dd {
    margin: 0;
  }
  .allow-list ul {
    margin: 0;
    padding-left: 16px;
    list-style: disc;
    color: var(--text-2);
  }
  .mono {
    font-family: var(--font-mono, ui-monospace, SFMono-Regular, monospace);
  }
  .muted {
    color: var(--text-3);
    font-style: italic;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 4px;
  }
  button.primary,
  button.ghost {
    font: inherit;
    padding: 8px 14px;
    border-radius: var(--r-sm, 6px);
    cursor: pointer;
    border: 1px solid var(--border);
  }
  button.primary {
    background: var(--accent, #3a8bff);
    color: var(--accent-fg, white);
    border-color: var(--accent, #3a8bff);
    font-weight: 600;
  }
  button.ghost {
    background: transparent;
    color: var(--text-2);
  }
  button.ghost:hover {
    background: var(--bg-2);
  }
</style>
