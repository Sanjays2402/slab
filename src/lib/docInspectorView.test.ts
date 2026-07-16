// Pure-helper tests for the document inspector title/tag editor.
// Run with: tsx src/lib/docInspectorView.test.ts

import {
  filterInspectorTagOptions,
  normalizeInspectorTagQuery,
  normalizeInspectorTitle,
  planInspectorTagAssignment,
  planInspectorTagMutation,
  planInspectorTitleMutation,
  rollbackInspectorTagMutation,
  toggleInspectorTag,
} from "./docInspectorView";
import type { DocumentRecord, TagRecord } from "./library";

let passed = 0;
let failed = 0;
function expect(condition: boolean, label: string): void {
  if (condition) {
    passed++;
  } else {
    failed++;
    // eslint-disable-next-line no-console
    console.error(`FAIL: ${label}`);
  }
}

function tag(id: number, name: string): TagRecord {
  return { id, name, color: null, description: null };
}

function document(tags: TagRecord[] = [], id = 7): DocumentRecord {
  return {
    id,
    folder_id: 2,
    path: "/docs/scan.pdf",
    title: "Quarterly scan",
    hash: "abc",
    size_bytes: 123,
    mtime_ns: 456,
    pages: 3,
    added_at: 10,
    last_seen_at: 20,
    ocr_state: "text_native",
    ocr_output_path: null,
    ocr_error: null,
    notes: null,
    starred: false,
    tags,
  };
}

const finance = tag(1, "Finance");
const final = tag(2, "Final");
const followUp = tag(3, "Follow up");
const legal = tag(4, "Legal");

// Query/title normalization.
expect(normalizeInspectorTagQuery("  fin   q1 ") === "fin q1", "query collapses whitespace");
expect(normalizeInspectorTagQuery(null) === "", "null query becomes empty");
expect(normalizeInspectorTitle("  Q1 report  ") === "Q1 report", "title trims");
expect(normalizeInspectorTitle("   ") === null, "blank title clears override");
expect(normalizeInspectorTitle(undefined) === null, "missing title becomes null");

// Search excludes attached tags, ranks fuzzy matches, highlights, and de-dupes.
{
  const all = [legal, followUp, finance, final, { ...finance }];
  const unfiltered = filterInspectorTagOptions(all, [legal], "");
  expect(
    unfiltered.map((option) => option.tag.name).join(",") === "Final,Finance,Follow up",
    "empty query returns unattached tags alphabetically",
  );
  const matches = filterInspectorTagOptions(all, [], "fin");
  expect(matches.length === 2, "query finds prefix matches");
  expect(matches[0].segments.some((segment) => segment.hit), "match includes highlight segments");
  expect(
    matches[0].segments.map((segment) => segment.text).join("") === matches[0].tag.name,
    "highlight segments reconstruct the tag name",
  );
  expect(filterInspectorTagOptions(all, [], "flup")[0]?.tag.id === followUp.id, "fuzzy subsequence matches");
  expect(filterInspectorTagOptions(all, [], "zzz").length === 0, "non-match returns no options");
}

// Toggle is immutable and removes duplicate ids while changing membership.
{
  const original = [finance, { ...finance }, legal];
  const removed = toggleInspectorTag(original, finance);
  expect(removed.map((item) => item.id).join(",") === "4", "toggle removes an attached tag");
  expect(original.length === 3, "toggle does not mutate the source");
  const added = toggleInspectorTag([finance], final);
  expect(added.map((item) => item.id).join(",") === "1,2", "toggle appends a new tag");
}

// Mutation plans retain an exact rollback snapshot and publish a fresh document.
{
  const before = document([finance]);
  expect(planInspectorTitleMutation(before, "Quarterly scan") === null, "unchanged title is a no-op");
  const titlePlan = planInspectorTitleMutation(before, "  Board packet ");
  expect(titlePlan?.before === before, "title plan retains rollback snapshot");
  expect(titlePlan?.optimistic !== before, "title plan creates a fresh optimistic row");
  expect(titlePlan?.optimistic.title === "Board packet", "title plan normalizes optimistic value");
  expect(before.title === "Quarterly scan", "title plan leaves source untouched");
  expect(planInspectorTitleMutation(before, " ")?.title === null, "title plan supports clearing override");

  const addPlan = planInspectorTagMutation(before, final);
  expect(addPlan.before === before, "tag plan retains rollback snapshot");
  expect(addPlan.attached === true, "tag plan reports add");
  expect(addPlan.tag.id === final.id, "tag plan retains the target tag");
  expect(addPlan.optimistic.tags.length === 2, "tag plan publishes optimistic tags");
  expect(before.tags.length === 1, "tag plan leaves source tags untouched");

  const removePlan = planInspectorTagMutation(before, finance);
  expect(removePlan.attached === false, "tag plan reports removal");
  expect(
    planInspectorTagAssignment(before, finance, true).optimistic.tags.length === 1,
    "tag assignment does not toggle a link already in the requested state",
  );
  expect(
    planInspectorTagAssignment(before, final, false).optimistic.tags.length === 1,
    "tag assignment leaves an absent link detached",
  );

  const afterExternalAdd = document([finance, final, legal]);
  const rolledBackAdd = rollbackInspectorTagMutation(afterExternalAdd, addPlan);
  expect(
    rolledBackAdd.tags.map((item) => item.id).join(",") === "1,4",
    "add rollback removes only its target and preserves external tags",
  );

  const afterExternalRemove = document([legal]);
  const rolledBackRemove = rollbackInspectorTagMutation(afterExternalRemove, removePlan);
  expect(
    rolledBackRemove.tags.map((item) => item.id).join(",") === "4,1",
    "remove rollback restores only its target and preserves external tags",
  );
  expect(
    rollbackInspectorTagMutation(document(), addPlan).tags.length === 0,
    "add rollback preserves a newer detach",
  );
  expect(
    rollbackInspectorTagMutation(document([finance, legal]), removePlan).tags
      .map((item) => item.id)
      .join(",") === "1,4",
    "remove rollback preserves a newer attach",
  );
  expect(
    rollbackInspectorTagMutation(document([], 8), addPlan).id === 8,
    "rollback ignores a different document",
  );
}

// eslint-disable-next-line no-console
console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) process.exitCode = 1;
