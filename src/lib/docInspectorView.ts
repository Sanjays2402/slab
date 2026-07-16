// Pure view-model helpers for the document inspector's inline title and tag
// editor. The Svelte component owns IPC and notifications; this module owns
// normalization, fuzzy tag filtering, and immutable optimistic snapshots.

import {
  scorePaletteField,
  splitHighlight,
  type HighlightSegment,
} from "./paletteSearch";
import type { DocumentRecord, TagRecord } from "./library";

export interface InspectorTagOption {
  readonly tag: TagRecord;
  readonly score: number;
  readonly segments: HighlightSegment[];
}

export interface InspectorTitleMutation {
  readonly before: DocumentRecord;
  readonly optimistic: DocumentRecord;
  readonly title: string | null;
}

export interface InspectorTagMutation {
  readonly before: DocumentRecord;
  readonly optimistic: DocumentRecord;
  readonly tag: TagRecord;
  readonly attached: boolean;
}

/** Collapse whitespace and trim a tag-filter query. */
export function normalizeInspectorTagQuery(
  query: string | null | undefined,
): string {
  if (typeof query !== "string") return "";
  return query.replace(/\s+/g, " ").trim();
}

/** Normalize a title draft to the backend's trim-or-null contract. */
export function normalizeInspectorTitle(
  title: string | null | undefined,
): string | null {
  if (typeof title !== "string") return null;
  const trimmed = title.trim();
  return trimmed.length > 0 ? trimmed : null;
}

/**
 * Return unattached tags ranked with the same prefix / substring / subsequence
 * scorer used by the command palette. Duplicate tag ids are ignored.
 */
export function filterInspectorTagOptions(
  allTags: readonly TagRecord[],
  attachedTags: readonly TagRecord[],
  query: string | null | undefined,
): InspectorTagOption[] {
  const normalized = normalizeInspectorTagQuery(query);
  const attachedIds = new Set(attachedTags.map((tag) => tag.id));
  const seen = new Set<number>();
  const options: InspectorTagOption[] = [];

  for (const tag of allTags) {
    if (!tag || seen.has(tag.id) || attachedIds.has(tag.id)) continue;
    seen.add(tag.id);
    const match = scorePaletteField(normalized, tag.name);
    if (match.score <= 0) continue;
    options.push({
      tag,
      score: match.score,
      segments: splitHighlight(tag.name, match.ranges),
    });
  }

  return options.sort(
    (a, b) =>
      b.score - a.score ||
      a.tag.name.localeCompare(b.tag.name, undefined, {
        sensitivity: "base",
        numeric: true,
      }) ||
      a.tag.id - b.tag.id,
  );
}

/** Toggle one tag without mutating the document's current tag array. */
export function toggleInspectorTag(
  attachedTags: readonly TagRecord[],
  tag: TagRecord,
): TagRecord[] {
  const unique: TagRecord[] = [];
  const seen = new Set<number>();
  for (const current of attachedTags) {
    if (!current || seen.has(current.id)) continue;
    seen.add(current.id);
    unique.push(current);
  }

  if (seen.has(tag.id)) {
    return unique.filter((current) => current.id !== tag.id);
  }
  return [...unique, tag];
}

/** Build the before/optimistic pair used to publish and roll back a title edit. */
export function planInspectorTitleMutation(
  doc: DocumentRecord,
  draft: string,
): InspectorTitleMutation | null {
  const title = normalizeInspectorTitle(draft);
  if (title === normalizeInspectorTitle(doc.title)) return null;
  return {
    before: doc,
    optimistic: { ...doc, title },
    title,
  };
}

/** Build the immutable optimistic snapshot for one tag add/remove. */
export function planInspectorTagMutation(
  doc: DocumentRecord,
  tag: TagRecord,
): InspectorTagMutation {
  const wasAttached = doc.tags.some((current) => current.id === tag.id);
  return planInspectorTagAssignment(doc, tag, !wasAttached);
}

/** Build an optimistic snapshot that sets one tag link to a known state. */
export function planInspectorTagAssignment(
  doc: DocumentRecord,
  tag: TagRecord,
  attached: boolean,
): InspectorTagMutation {
  const uniqueTags = toggleInspectorTag(doc.tags, tag);
  const wasAttached = doc.tags.some((current) => current.id === tag.id);
  const tags = attached === wasAttached ? [...doc.tags] : uniqueTags;
  return {
    before: doc,
    optimistic: { ...doc, tags },
    tag,
    attached,
  };
}

/**
 * Undo only the target link against the latest document snapshot. This keeps
 * unrelated tags that may have arrived from another window while IPC was in
 * flight instead of restoring a stale whole-array snapshot.
 */
export function rollbackInspectorTagMutation(
  current: DocumentRecord,
  mutation: InspectorTagMutation,
): DocumentRecord {
  if (current.id !== mutation.before.id) return current;
  const tag = mutation.tag;
  const currentlyAttached = current.tags.some((item) => item.id === tag.id);
  if (currentlyAttached !== mutation.attached) return current;
  const wasAttached = mutation.before.tags.some((item) => item.id === tag.id);
  return planInspectorTagAssignment(current, tag, wasAttached).optimistic;
}
