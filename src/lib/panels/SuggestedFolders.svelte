<!--
  Suggested Folders panel (v3.38.0 "Atlas Suggest").

  Reads the local heuristic suggestion engine (which clusters the
  user's recent library searches) and renders up to 3 candidate
  Smart Folders the user can Add or Dismiss in one click.

  Mounts inside SmartFoldersHubPanel above the search input.

  Wow moment: a sparkle conic-gradient border that slowly rotates,
  emphasizing that this is the *personalized* / magic section.
  On first paint the cards fly up in a brief stagger (round 58), so the
  strip settles in rather than snapping in fully formed; the stagger
  collapses to a no-op under prefers-reduced-motion.
  Renders nothing when the user has fewer than 10 logged searches
  (the engine returns []), so it's graceful for fresh installs.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { quintOut } from "svelte/easing";
  import {
    librarySuggestionsList,
    librarySuggestionsDismiss,
    librarySuggestionsAccept,
    librarySearchLogCount,
    type FolderSuggestion,
  } from "$lib/library";

  type Props = {
    /** Called after a suggestion is accepted, so the parent can refresh
        the main folder list. */
    onAccepted?: () => void;
  };
  const { onAccepted }: Props = $props();

  let suggestions = $state<FolderSuggestion[]>([]);
  let logCount = $state(0);
  let loading = $state(true);
  let busyHash = $state<string | null>(null);
  let dismissedAnim = $state<Set<string>>(new Set());

  // Respect the OS reduced-motion setting: skip the entrance stagger entirely
  // for users who asked for less movement (the strip just appears). Read once
  // at mount via the standard media query.
  let reduceMotion = $state(false);

  /** Per-card entrance: a brief upward fly with a stagger so the suggested
      strip settles in on first paint instead of snapping in fully formed.
      Index-keyed delay; collapses to a no-op when reduced-motion is set. */
  function cardIn(_node: Element, { index }: { index: number }) {
    if (reduceMotion) return { duration: 0 };
    return {
      delay: Math.min(index, 3) * 70,
      duration: 320,
      easing: quintOut,
      css: (t: number, u: number) => `opacity: ${t}; transform: translateY(${u * 8}px);`,
    };
  }

  async function refresh() {
    loading = true;
    try {
      const [list, count] = await Promise.all([
        librarySuggestionsList(),
        librarySearchLogCount(),
      ]);
      suggestions = list;
      logCount = count;
    } catch (e) {
      // Best-effort: never crash the hub if suggestions fail.
      console.error("[Atlas Suggest]", e);
      suggestions = [];
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    // Snapshot the OS reduced-motion preference so the entrance can no-op for
    // users who asked for less movement.
    if (typeof window !== "undefined" && window.matchMedia) {
      reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    }
    refresh();
  });

  async function accept(s: FolderSuggestion) {
    busyHash = s.cluster_hash;
    try {
      await librarySuggestionsAccept(s);
      // Slide-out animation then drop from the list.
      dismissedAnim = new Set([...dismissedAnim, s.cluster_hash]);
      setTimeout(() => {
        suggestions = suggestions.filter(
          (x) => x.cluster_hash !== s.cluster_hash,
        );
      }, 260);
      onAccepted?.();
    } catch (e) {
      console.error("[Atlas Suggest accept]", e);
    } finally {
      busyHash = null;
    }
  }

  async function dismiss(s: FolderSuggestion) {
    busyHash = s.cluster_hash;
    try {
      dismissedAnim = new Set([...dismissedAnim, s.cluster_hash]);
      await librarySuggestionsDismiss(s.cluster_hash);
      setTimeout(() => {
        suggestions = suggestions.filter(
          (x) => x.cluster_hash !== s.cluster_hash,
        );
      }, 260);
    } catch (e) {
      console.error("[Atlas Suggest dismiss]", e);
    } finally {
      busyHash = null;
    }
  }
</script>

{#if !loading && suggestions.length > 0}
  <section class="suggest-wrap" aria-label="Suggested Smart Folders">
    <header class="suggest-head">
      <span class="sparkle" aria-hidden="true">✨</span>
      <span class="title">Suggested for you</span>
      <span class="sub">based on your recent searches</span>
    </header>
    <ul class="suggest-list">
      {#each suggestions as s, i (s.cluster_hash)}
        <li
          class="suggest-card"
          class:dismissing={dismissedAnim.has(s.cluster_hash)}
          style="--card-color: {s.color};"
          in:cardIn={{ index: i }}
        >
          <div class="sparkle-border" aria-hidden="true"></div>
          <div class="card-inner">
            <span class="card-icon" aria-hidden="true">{s.icon}</span>
            <div class="card-body">
              <div class="card-name">{s.name}</div>
              <div class="card-reason">{s.reason}</div>
            </div>
            <div class="card-actions">
              <button
                class="add-btn"
                onclick={() => accept(s)}
                disabled={busyHash === s.cluster_hash}
                title="Save as a personal Smart Folder"
              >
                {busyHash === s.cluster_hash ? "…" : "+ Add"}
              </button>
              <button
                class="dismiss-btn"
                onclick={() => dismiss(s)}
                disabled={busyHash === s.cluster_hash}
                aria-label="Dismiss suggestion"
                title="Dismiss — don't suggest this again"
              >
                ✕
              </button>
            </div>
          </div>
        </li>
      {/each}
    </ul>
  </section>
{/if}

<style>
  .suggest-wrap {
    padding: 14px 18px 8px;
    border-bottom: 1px solid color-mix(in srgb, currentColor 10%, transparent);
  }
  .suggest-head {
    display: flex;
    align-items: baseline;
    gap: 8px;
    margin-bottom: 10px;
    font-size: 13px;
  }
  .sparkle {
    font-size: 16px;
    animation: sparkle-pulse 2.4s ease-in-out infinite;
  }
  .title {
    font-weight: 600;
    color: color-mix(in srgb, currentColor 92%, transparent);
  }
  .sub {
    color: color-mix(in srgb, currentColor 55%, transparent);
    font-size: 12px;
  }

  .suggest-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 10px;
    grid-template-columns: repeat(auto-fit, minmax(280px, 1fr));
  }

  .suggest-card {
    position: relative;
    border-radius: 12px;
    padding: 2px; /* room for the sparkle border */
    overflow: hidden;
    transition:
      opacity 240ms ease,
      transform 240ms ease;
  }
  .suggest-card.dismissing {
    opacity: 0;
    transform: scale(0.94) translateY(-4px);
  }

  /* Rotating conic-gradient border — the wow moment. */
  .sparkle-border {
    position: absolute;
    inset: 0;
    border-radius: 12px;
    background: conic-gradient(
      from var(--angle, 0deg),
      var(--card-color),
      transparent 35%,
      var(--card-color) 60%,
      transparent 95%,
      var(--card-color)
    );
    opacity: 0.85;
    animation: sparkle-spin 6s linear infinite;
    /* Allow it to be masked into a ring by the inner card. */
    z-index: 0;
  }

  .card-inner {
    position: relative;
    z-index: 1;
    background: color-mix(in srgb, Canvas 92%, transparent);
    border-radius: 10px;
    padding: 12px 14px;
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: 12px;
    backdrop-filter: blur(8px);
    -webkit-backdrop-filter: blur(8px);
  }

  .card-icon {
    font-size: 26px;
    line-height: 1;
    width: 36px;
    height: 36px;
    display: grid;
    place-items: center;
    border-radius: 9px;
    background: color-mix(in srgb, var(--card-color) 18%, transparent);
  }
  .card-name {
    font-size: 14px;
    font-weight: 600;
    color: color-mix(in srgb, currentColor 94%, transparent);
  }
  .card-reason {
    font-size: 12px;
    margin-top: 2px;
    color: color-mix(in srgb, currentColor 60%, transparent);
  }
  .card-actions {
    display: flex;
    gap: 6px;
  }
  .add-btn,
  .dismiss-btn {
    border: 1px solid color-mix(in srgb, currentColor 14%, transparent);
    background: color-mix(in srgb, var(--card-color) 12%, transparent);
    color: inherit;
    border-radius: 8px;
    padding: 6px 10px;
    font-size: 12px;
    cursor: pointer;
    transition: background 120ms ease, transform 120ms ease;
  }
  .add-btn {
    font-weight: 600;
    background: var(--card-color);
    color: white;
    border-color: transparent;
  }
  .add-btn:hover:not(:disabled) {
    transform: translateY(-1px);
    filter: brightness(1.07);
  }
  .add-btn:disabled,
  .dismiss-btn:disabled {
    opacity: 0.55;
    cursor: progress;
  }
  .dismiss-btn:hover:not(:disabled) {
    background: color-mix(in srgb, currentColor 10%, transparent);
  }

  @keyframes sparkle-spin {
    from { --angle: 0deg; }
    to   { --angle: 360deg; }
  }
  @property --angle {
    syntax: "<angle>";
    initial-value: 0deg;
    inherits: false;
  }

  @keyframes sparkle-pulse {
    0%, 100% { transform: scale(1); opacity: 0.85; }
    50%      { transform: scale(1.18); opacity: 1; }
  }

  @media (prefers-reduced-motion: reduce) {
    .sparkle-border,
    .sparkle { animation: none; }
  }
</style>
