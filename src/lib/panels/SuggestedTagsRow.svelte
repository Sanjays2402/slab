<!--
  Suggested Tags row (v3.39.0 "Atlas Tag-Suggest").

  For a single untagged (or lightly-tagged) document, fetches up to 5
  locally-computed tag suggestions and renders them as a pastel chip row.
  Click ✓ to accept (the tag is created/attached and the chip slides out);
  click ✗ to dismiss (never suggested again for this doc).

  Mounts inside LibraryPanel beneath each document card's existing tag row.
  Renders nothing when the engine returns no suggestions, so it's graceful
  for docs with nothing plausible to suggest.

  Wow moment: open a library full of untagged PDFs and every card grows a
  row of plausible ✨ pastel tag chips, clustered by hash-color, with zero
  configuration.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import {
    tagSuggestionsForDoc,
    acceptTagSuggestion,
    dismissTagSuggestion,
    type TagSuggestion,
    type TagRecord,
  } from "$lib/library";

  type Props = {
    /** Document to suggest tags for. */
    docId: number;
    /** Called with the created/attached tag after an accept, so the parent
        can patch the doc's chip row optimistically. */
    onAccepted?: (tag: TagRecord) => void;
  };
  const { docId, onAccepted }: Props = $props();

  let suggestions = $state<TagSuggestion[]>([]);
  let loading = $state(true);
  let busy = $state<string | null>(null);
  let leaving = $state<Set<string>>(new Set());

  async function load() {
    loading = true;
    try {
      suggestions = await tagSuggestionsForDoc(docId);
    } catch (e) {
      // Best-effort: never crash the library if suggestions fail.
      console.error("[Atlas Tag-Suggest]", e);
      suggestions = [];
    } finally {
      loading = false;
    }
  }

  onMount(load);

  function slideOutThenDrop(tagName: string) {
    leaving = new Set([...leaving, tagName]);
    setTimeout(() => {
      suggestions = suggestions.filter((x) => x.tag_name !== tagName);
      const next = new Set(leaving);
      next.delete(tagName);
      leaving = next;
    }, 200);
  }

  async function accept(s: TagSuggestion) {
    busy = s.tag_name;
    try {
      const tag = await acceptTagSuggestion(docId, s.tag_name);
      onAccepted?.(tag);
      slideOutThenDrop(s.tag_name);
    } catch (e) {
      console.error("[Atlas Tag-Suggest] accept", e);
    } finally {
      busy = null;
    }
  }

  async function dismiss(s: TagSuggestion) {
    busy = s.tag_name;
    try {
      await dismissTagSuggestion(docId, s.tag_name);
      slideOutThenDrop(s.tag_name);
    } catch (e) {
      console.error("[Atlas Tag-Suggest] dismiss", e);
    } finally {
      busy = null;
    }
  }

  // Source -> monochrome glyph (matches Slab's no-color-emoji chrome rule):
  //   vocabulary -> diamond, co-occurrence -> link, domain -> hash.
  function sourceIcon(src: string): string {
    if (src === "cooccurrence") return "\u26AD"; // ⚭ marriage/link symbol
    if (src === "domain") return "\u2317"; // ⌗ viewdata square (hash-like)
    return "\u2666"; // ♦ black diamond (vocabulary match)
  }

  function sourceTitle(src: string): string {
    if (src === "cooccurrence") return "Frequently used alongside this doc's tags";
    if (src === "domain") return "Matched a known document type";
    return "Matches one of your existing tags";
  }

  // Deterministic pastel from the tag name (mirrors the Rust pastel_for so
  // the chip color previews what the saved tag will look like).
  function pastelFor(name: string): string {
    let h = 0x811c9dc5;
    for (let i = 0; i < name.length; i++) {
      h ^= name.charCodeAt(i);
      h = Math.imul(h, 0x01000193) >>> 0;
    }
    return `hsl(${h % 360}, 60%, 80%)`;
  }
</script>

{#if loading || suggestions.length > 0}
  <div class="suggested-tags-row">
    <span class="label">✨ Suggested</span>
    {#if loading}
      <span class="skeleton"></span>
    {:else}
      {#each suggestions as s (s.tag_name)}
        <span
          class="sugg-chip"
          class:leaving={leaving.has(s.tag_name)}
          style:--chip-accent={pastelFor(s.tag_name)}
        >
          <span class="icon" title={sourceTitle(s.source)}>{sourceIcon(s.source)}</span>
          <span class="name">{s.tag_name}</span>
          <button
            class="act accept"
            title="Add this tag"
            disabled={busy === s.tag_name}
            onclick={() => accept(s)}>✓</button>
          <button
            class="act dismiss"
            title="Never suggest this for this document"
            disabled={busy === s.tag_name}
            onclick={() => dismiss(s)}>✗</button>
        </span>
      {/each}
    {/if}
  </div>
{/if}

<style>
  .suggested-tags-row {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
    margin-top: 6px;
    padding: 5px 7px;
    border-radius: 9px;
    background: color-mix(in oklab, var(--surface-2, #1b1b1f) 70%, transparent);
    border: 1px solid color-mix(in oklab, var(--accent, #7c3aed) 18%, transparent);
  }

  .label {
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.02em;
    color: var(--text-2, #a8a8b3);
    user-select: none;
  }

  .sugg-chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 3px 2px 8px;
    border-radius: 999px;
    font-size: 12px;
    line-height: 1.4;
    background: var(--surface-3, #26262c);
    border-left: 3px solid var(--chip-accent, var(--accent, #7c3aed));
    transform-origin: left center;
    transition:
      transform 180ms cubic-bezier(0.22, 1, 0.36, 1),
      opacity 180ms ease;
  }

  .sugg-chip.leaving {
    transform: scale(0.7) translateX(-6px);
    opacity: 0;
  }

  .icon {
    opacity: 0.75;
    font-size: 11px;
  }

  .name {
    color: var(--text-1, #ededf2);
    white-space: nowrap;
  }

  .act {
    border: none;
    background: transparent;
    cursor: pointer;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    font-size: 11px;
    line-height: 1;
    color: var(--text-2, #a8a8b3);
    transition:
      background 120ms ease,
      color 120ms ease;
  }

  .act:hover:not(:disabled) {
    background: color-mix(in oklab, var(--accent, #7c3aed) 24%, transparent);
    color: var(--text-1, #ededf2);
  }

  .act.dismiss:hover:not(:disabled) {
    background: color-mix(in oklab, #ff6464 30%, transparent);
  }

  .act:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .skeleton {
    width: 92px;
    height: 18px;
    border-radius: 999px;
    background: linear-gradient(
      90deg,
      var(--surface-3, #26262c) 0%,
      color-mix(in oklab, var(--accent, #7c3aed) 16%, transparent) 50%,
      var(--surface-3, #26262c) 100%
    );
    background-size: 200% 100%;
    animation: shimmer 1.4s ease-in-out infinite;
  }

  @keyframes shimmer {
    0% {
      background-position: 200% 0;
    }
    100% {
      background-position: -200% 0;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .sugg-chip,
    .skeleton {
      animation: none !important;
      transition: none !important;
    }
  }
</style>
