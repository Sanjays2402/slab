// Pure-helper tests for the "?" shortcuts-overlay core.
//
// Style matches paletteSearch.test.ts / toastStack.test.ts — no test
// runner, just an inline `expect` so the contract reads at a glance.
//
// Run with:
//   tsx src/lib/shortcutsOverlay.test.ts

import {
  buildShortcutGroups,
  shortcutGroupRank,
  countShortcutRows,
  filterShortcutGroups,
  flattenShortcutRows,
  canonicalizeBinding,
  detectShortcutConflicts,
  conflictingActionIds,
  formatShortcutsText,
  SHORTCUT_GROUP_ORDER,
  type ShortcutActionLike,
  type ShortcutInfoSpec,
  type ShortcutRow,
} from "./shortcutsOverlay";

let passed = 0;
let failed = 0;

function expect(cond: boolean, label: string): void {
  if (!cond) {
    failed++;
    // eslint-disable-next-line no-console
    console.error("FAIL:", label);
    if (typeof process !== "undefined") process.exitCode = 1;
  } else {
    passed++;
    // eslint-disable-next-line no-console
    console.log("ok:", label);
  }
}

const act = (
  id: string,
  group: string,
  binding: string,
  extra: Partial<ShortcutActionLike> = {},
): ShortcutActionLike => ({ id, label: id, group, binding, ...extra });

// --- shortcutGroupRank ----------------------------------------------
{
  expect(shortcutGroupRank("Global") === 0, "rank: Global is first");
  expect(shortcutGroupRank("Tabs") === 1, "rank: Tabs is second");
  expect(
    shortcutGroupRank("Home") === SHORTCUT_GROUP_ORDER.length - 1,
    "rank: Home is last known",
  );
  expect(
    shortcutGroupRank("Zzz Unknown") === SHORTCUT_GROUP_ORDER.length,
    "rank: unknown group sorts after all known",
  );
  // Order array has no dupes.
  expect(
    new Set(SHORTCUT_GROUP_ORDER).size === SHORTCUT_GROUP_ORDER.length,
    "rank: group order has no duplicates",
  );
}

// --- buildShortcutGroups: basic grouping + order --------------------
{
  const groups = buildShortcutGroups([
    act("home.open", "Home", "Mod+0"),
    act("palette.open", "Global", "Mod+K"),
    act("tabs.new", "Tabs", "Mod+T"),
  ]);
  expect(groups.length === 3, "build: three groups");
  expect(
    groups.map((g) => g.title).join(",") === "Global,Tabs,Home",
    "build: groups in curated order (Global,Tabs,Home) regardless of input order",
  );
  expect(groups[0].rows.length === 1 && groups[0].rows[0].actionId === "palette.open", "build: Global row");
  expect(groups[0].rows[0].binding === "Mod+K", "build: binding carried through");
  expect(groups[0].rows[0].staticKeys.length === 0, "build: bindable row has no static keys");
}

// --- buildShortcutGroups: rows within a group keep keymap order ------
{
  const groups = buildShortcutGroups([
    act("tabs.goto3", "Tabs", "Mod+3"),
    act("tabs.goto1", "Tabs", "Mod+1"),
    act("tabs.goto2", "Tabs", "Mod+2"),
  ]);
  expect(groups.length === 1, "order: single Tabs group");
  expect(
    groups[0].rows.map((r) => r.actionId).join(",") === "tabs.goto3,tabs.goto1,tabs.goto2",
    "order: rows preserve the input keymap order (no re-sort within a group)",
  );
}

// --- buildShortcutGroups: info hints append after bindable rows ------
{
  const infos: ShortcutInfoSpec[] = [
    { group: "Global", label: "Close current overlay", keys: ["Esc"] },
    { group: "Theater", label: "Whiteboard", keys: ["W"] },
  ];
  const groups = buildShortcutGroups([act("palette.open", "Global", "Mod+K")], infos);
  const global = groups.find((g) => g.title === "Global")!;
  expect(global.rows.length === 2, "info: Global has bindable + info row");
  expect(global.rows[0].actionId === "palette.open", "info: bindable row first");
  expect(
    global.rows[1].actionId === null && global.rows[1].label === "Close current overlay",
    "info: info row second with null actionId",
  );
  expect(
    global.rows[1].staticKeys.join("") === "Esc",
    "info: info row carries static keys",
  );
  // A Theater group materializes purely from an info hint (no bindable).
  const theater = groups.find((g) => g.title === "Theater");
  expect(!!theater && theater.rows.length === 1, "info: info-only group materializes");
}

// --- buildShortcutGroups: override flag ------------------------------
{
  const groups = buildShortcutGroups([
    act("a.x", "Global", "Mod+J", { is_override: true }),
    act("b.y", "Global", "Mod+L", { is_override: false }),
    // is_override absent -> derive from default mismatch.
    act("c.z", "Global", "Mod+P", { default_binding: "Mod+Q" }),
    act("d.w", "Global", "Mod+R", { default_binding: "Mod+R" }),
  ]);
  const rows = groups[0].rows;
  expect(rows[0].isOverride === true, "override: explicit true");
  expect(rows[1].isOverride === false, "override: explicit false");
  expect(rows[2].isOverride === true, "override: derived true when binding != default");
  expect(rows[3].isOverride === false, "override: derived false when binding == default");
}

// --- buildShortcutGroups: unknown group sorts last alphabetically ----
{
  const groups = buildShortcutGroups([
    act("z.a", "Zebra", "Z"),
    act("a.a", "Apple", "A"),
    act("g.a", "Global", "Mod+K"),
  ]);
  expect(
    groups.map((g) => g.title).join(",") === "Global,Apple,Zebra",
    "unknown: known group first, then unknown groups alphabetically",
  );
}

// --- buildShortcutGroups: defensive inputs ---------------------------
{
  // @ts-expect-error null tolerance
  expect(buildShortcutGroups(null).length === 0, "defensive: null actions -> empty");
  expect(buildShortcutGroups([]).length === 0, "defensive: empty actions -> empty");
  // Garbage rows skipped, good rows kept.
  const groups = buildShortcutGroups([
    // @ts-expect-error garbage
    null,
    // @ts-expect-error missing group
    { id: "x", label: "x", binding: "X" },
    act("ok.id", "Global", "Mod+K"),
  ]);
  expect(groups.length === 1 && groups[0].rows.length === 1, "defensive: garbage rows skipped");
  // Empty groups never appear (info with empty array still creates a row).
  const onlyInfo = buildShortcutGroups([], [{ group: "Reading", label: "Scroll", keys: [] }]);
  expect(onlyInfo.length === 1 && onlyInfo[0].rows[0].staticKeys.length === 0, "defensive: empty keys ok");
  // Does not mutate inputs.
  const input = [act("a.a", "Global", "Mod+K")];
  const snapshot = JSON.stringify(input);
  buildShortcutGroups(input);
  expect(JSON.stringify(input) === snapshot, "defensive: input not mutated");
}

// --- countShortcutRows ----------------------------------------------
{
  const groups = buildShortcutGroups(
    [act("a.a", "Global", "Mod+K"), act("b.b", "Tabs", "Mod+T")],
    [{ group: "Global", label: "Esc", keys: ["Esc"] }],
  );
  expect(countShortcutRows(groups) === 3, "count: 2 bindable + 1 info = 3");
  expect(countShortcutRows([]) === 0, "count: empty -> 0");
  // @ts-expect-error null tolerance
  expect(countShortcutRows(null) === 0, "count: null -> 0");
}

// --- filterShortcutGroups: empty query -> unfiltered ----------------
{
  const groups = buildShortcutGroups(
    [act("palette.open", "Global", "Mod+K"), act("tabs.new", "Tabs", "Mod+T")],
    [{ group: "Global", label: "Close", keys: ["Esc"] }],
  );
  const all = filterShortcutGroups(groups, "");
  expect(countShortcutRows(all) === 3, "filter: empty query keeps every row");
  expect(all[0].rows[0].titleRanges.length === 0, "filter: empty query -> no ranges");
  const blank = filterShortcutGroups(groups, "   ");
  expect(countShortcutRows(blank) === 3, "filter: whitespace query treated as empty");
}

// --- filterShortcutGroups: label match with highlight ranges --------
{
  const groups = buildShortcutGroups([
    { id: "a.x", label: "Open command palette", group: "Global", binding: "Mod+K" },
    { id: "b.y", label: "Close current tab", group: "Tabs", binding: "Mod+W" },
  ]);
  const res = filterShortcutGroups(groups, "palette");
  expect(res.length === 1 && res[0].title === "Global", "filter: label match keeps only Global");
  expect(res[0].rows.length === 1, "filter: only the matching row");
  const r = res[0].rows[0];
  expect(r.titleRanges.length > 0, "filter: label match yields highlight ranges");
  const hit = r.titleRanges.map((x) => r.label.slice(x.start, x.end)).join("");
  expect(hit.toLowerCase() === "palette", "filter: ranges cover the matched substring");
}

// --- filterShortcutGroups: group-title match surfaces whole section -
{
  const groups = buildShortcutGroups([
    act("t.next", "Theater", "PageDown"),
    act("t.prev", "Theater", "PageUp"),
    act("g.k", "Global", "Mod+K"),
  ]);
  const res = filterShortcutGroups(groups, "theater");
  expect(res.length === 1 && res[0].title === "Theater", "filter: group-title hit keeps Theater");
  expect(res[0].rows.length === 2, "filter: group-title hit surfaces ALL section rows");
  expect(
    res[0].rows.every((r) => r.titleRanges.length === 0),
    "filter: group-title hit leaves rows un-highlighted (heading matched, not labels)",
  );
}

// --- filterShortcutGroups: key-text match (no visible label mark) ----
{
  const groups = buildShortcutGroups(
    [act("g.k", "Global", "Mod+Shift+K")],
    [{ group: "Global", label: "Close current overlay", keys: ["Esc"] }],
  );
  // "shift" matches the canonical binding text, not the label.
  const byBinding = filterShortcutGroups(groups, "shift");
  expect(
    byBinding.length === 1 && byBinding[0].rows.length === 1,
    "filter: query matches a row by its canonical binding text",
  );
  expect(
    byBinding[0].rows[0].titleRanges.length === 0,
    "filter: key-text match leaves label un-highlighted",
  );
  // "esc" matches the info row's static key text.
  const byStatic = filterShortcutGroups(groups, "esc");
  expect(
    byStatic.some((g) => g.rows.some((r) => r.actionId === null)),
    "filter: info row found by its static key text",
  );
}

// --- filterShortcutGroups: no match -> empty -------------------------
{
  const groups = buildShortcutGroups([act("g.k", "Global", "Mod+K")]);
  const res = filterShortcutGroups(groups, "zzzznotathing");
  expect(res.length === 0, "filter: no match -> empty array");
}

// --- filterShortcutGroups: defensive + purity ------------------------
{
  // @ts-expect-error null tolerance
  expect(filterShortcutGroups(null, "x").length === 0, "filter: null groups -> empty");
  const groups = buildShortcutGroups([act("g.k", "Global", "Mod+K")]);
  const snapshot = JSON.stringify(groups);
  filterShortcutGroups(groups, "palette");
  expect(JSON.stringify(groups) === snapshot, "filter: does not mutate input groups");
  // Group order preserved across a multi-group match.
  const multi = buildShortcutGroups([
    act("g.k", "Global", "Mod+K"),
    act("t.t", "Tabs", "Mod+T"),
  ]);
  const open = filterShortcutGroups(multi, "o"); // matches "Open"/"to" labels (id-derived)
  // Whatever survives must keep curated order Global-before-Tabs.
  const titles = open.map((g) => g.title);
  expect(
    titles.indexOf("Global") === -1 ||
      titles.indexOf("Tabs") === -1 ||
      titles.indexOf("Global") < titles.indexOf("Tabs"),
    "filter: surviving groups preserve curated order",
  );
}

// --- flattenShortcutRows: flat index + group-start tags -------------
{
  const groups = buildShortcutGroups([
    act("g.a", "Global", "Mod+K"),
    act("g.b", "Global", "Mod+P"),
    act("t.a", "Tabs", "Mod+T"),
  ]);
  const flat = flattenShortcutRows(groups);
  expect(flat.length === 3, "flatten: 3 rows total");
  expect(
    flat.map((e) => e.flatIndex).join(",") === "0,1,2",
    "flatten: flat indices are sequential 0,1,2",
  );
  expect(
    flat.map((e) => (e.isGroupStart ? "1" : "0")).join("") === "101",
    "flatten: first row of each group tagged isGroupStart",
  );
  expect(
    flat.map((e) => e.groupTitle).join(",") === "Global,Global,Tabs",
    "flatten: each entry carries its group title",
  );
  expect(flat[0].row.actionId === "g.a", "flatten: underlying row preserved");
}

// --- flattenShortcutRows: works on filtered groups + walkable index --
{
  const groups = buildShortcutGroups([
    act("g.a", "Global", "Mod+K"),
    act("t.a", "Tabs", "Mod+T"),
    act("t.b", "Tabs", "Mod+W"),
  ]);
  const filtered = filterShortcutGroups(groups, "");
  const flat = flattenShortcutRows(filtered);
  expect(flat.length === 3, "flatten: filtered groups flatten too");
  // The flat list is what nextPaletteIndex walks: a single contiguous
  // index space across section boundaries.
  expect(
    flat[1].groupTitle === "Tabs" && flat[1].isGroupStart === true,
    "flatten: section boundary marked at the right flat index",
  );
}

// --- flattenShortcutRows: defensive ----------------------------------
{
  expect(flattenShortcutRows([]).length === 0, "flatten: empty -> empty");
  // @ts-expect-error null tolerance
  expect(flattenShortcutRows(null).length === 0, "flatten: null -> empty");
  // Garbage group (no rows array) skipped.
  const mixed = flattenShortcutRows([
    // @ts-expect-error garbage
    { title: "Bad" },
    { title: "Good", rows: buildShortcutGroups([act("a.a", "Global", "X")])[0].rows },
  ]);
  expect(mixed.length === 1 && mixed[0].groupTitle === "Good", "flatten: garbage group skipped");
}

// --- canonicalizeBinding --------------------------------------------
{
  expect(
    canonicalizeBinding("Mod+Shift+F") === canonicalizeBinding("shift+mod+f"),
    "canonical: modifier order does not matter",
  );
  expect(
    canonicalizeBinding("MOD+K") === canonicalizeBinding("mod+k"),
    "canonical: case-insensitive",
  );
  expect(canonicalizeBinding("Mod+Shift+F") === "mod+shift+f", "canonical: sorted mods + key");
  expect(canonicalizeBinding("Mod++") === "mod++", "canonical: literal + key tolerated");
  expect(canonicalizeBinding("+") === "+", "canonical: bare + key");
  expect(canonicalizeBinding("Escape") === "escape", "canonical: bare key");
  expect(canonicalizeBinding("") === "", "canonical: blank -> empty");
  expect(canonicalizeBinding("  ") === "", "canonical: whitespace -> empty");
  // Modifier aliases normalize.
  expect(
    canonicalizeBinding("Cmd+K") === canonicalizeBinding("Mod+K"),
    "canonical: Cmd == Mod",
  );
  expect(
    canonicalizeBinding("Option+A") === canonicalizeBinding("Alt+A"),
    "canonical: Option == Alt",
  );
  // Duplicate modifiers collapse.
  expect(canonicalizeBinding("Mod+Mod+K") === "mod+k", "canonical: duplicate mods collapse");
}

// --- detectShortcutConflicts: the real Mod+Shift+F collision ---------
{
  // This mirrors the shipping keymap: library.search and forms.open
  // both default to Mod+Shift+F.
  const groups = buildShortcutGroups([
    act("library.search", "Library", "Mod+Shift+F"),
    act("forms.open", "Forms", "Mod+Shift+F"),
    act("palette.open", "Global", "Mod+K"),
  ]);
  const conflicts = detectShortcutConflicts(groups);
  expect(conflicts.length === 1, "conflict: one colliding chord found");
  expect(conflicts[0].canonical === "mod+shift+f", "conflict: canonical chord reported");
  expect(
    conflicts[0].actionIds.length === 2 &&
      conflicts[0].actionIds.includes("library.search") &&
      conflicts[0].actionIds.includes("forms.open"),
    "conflict: both colliding action ids listed",
  );
}

// --- detectShortcutConflicts: order-independent collision -----------
{
  const groups = buildShortcutGroups([
    act("a.x", "Global", "Mod+Shift+P"),
    act("b.y", "Tabs", "Shift+Mod+P"), // same chord, different spelling
  ]);
  const conflicts = detectShortcutConflicts(groups);
  expect(conflicts.length === 1, "conflict: order-independent chords still collide");
  expect(conflicts[0].actionIds.length === 2, "conflict: both ids captured");
}

// --- detectShortcutConflicts: no false positives --------------------
{
  const groups = buildShortcutGroups(
    [
      act("a.x", "Global", "Mod+K"),
      act("b.y", "Tabs", "Mod+T"),
      act("c.z", "Reading", "Mod+F"),
    ],
    // Info rows share no-mod single keys (B, W) but must NOT be flagged.
    [
      { group: "Theater", label: "Blackout", keys: ["B"] },
      { group: "Theater", label: "Whiteboard", keys: ["W"] },
    ],
  );
  expect(detectShortcutConflicts(groups).length === 0, "conflict: distinct chords -> no conflict");
  // Same action id appearing once per chord is not a self-conflict.
  const single = buildShortcutGroups([act("solo.id", "Global", "Mod+J")]);
  expect(detectShortcutConflicts(single).length === 0, "conflict: a lone binding is not a conflict");
}

// --- detectShortcutConflicts: info rows excluded --------------------
{
  // A bindable action AND an info hint on the same literal chord must
  // NOT conflict (info rows are panel-local and excluded).
  const groups = buildShortcutGroups(
    [act("bedrock.open", "Archive", "Mod+Shift+B")],
    [{ group: "Discovery", label: "Bates", keys: ["Mod", "Shift", "B"] }],
  );
  expect(
    detectShortcutConflicts(groups).length === 0,
    "conflict: info row never collides with a bindable action",
  );
}

// --- conflictingActionIds -------------------------------------------
{
  const groups = buildShortcutGroups([
    act("library.search", "Library", "Mod+Shift+F"),
    act("forms.open", "Forms", "Mod+Shift+F"),
    act("palette.open", "Global", "Mod+K"),
  ]);
  const ids = conflictingActionIds(groups);
  expect(ids.has("library.search") && ids.has("forms.open"), "conflictIds: both colliders present");
  expect(!ids.has("palette.open"), "conflictIds: non-colliding action absent");
  expect(conflictingActionIds(buildShortcutGroups([])).size === 0, "conflictIds: empty -> empty set");
}

// --- detectShortcutConflicts: defensive ------------------------------
{
  // @ts-expect-error null tolerance
  expect(detectShortcutConflicts(null).length === 0, "conflict: null groups -> empty");
  // Blank bindings ignored (never collide on "").
  const blanks = buildShortcutGroups([
    { id: "a", label: "A", group: "Global", binding: "" },
    { id: "b", label: "B", group: "Global", binding: "" },
  ]);
  expect(detectShortcutConflicts(blanks).length === 0, "conflict: blank bindings ignored");
}

// --- formatShortcutsText: alignment + sections ----------------------
{
  const groups = buildShortcutGroups(
    [
      { id: "g.k", label: "Open command palette", group: "Global", binding: "Mod+K" },
      { id: "t.t", label: "Close tab", group: "Tabs", binding: "Mod+W" },
    ],
    [{ group: "Global", label: "Close overlay", keys: ["Esc"] }],
  );
  // Trivial resolver: bindable rows -> binding string, info -> joined keys.
  const resolve = (r: ShortcutRow): string =>
    r.actionId ? r.binding : r.staticKeys.join("+");
  const text = formatShortcutsText(groups, resolve, { title: "Slab Shortcuts" });
  const lines = text.split("\n");
  expect(lines[0] === "Slab Shortcuts", "text: title printed first");
  expect(lines.includes("Global"), "text: Global section heading present");
  expect(lines.includes("Tabs"), "text: Tabs section heading present");
  // The keys column is padded to the widest key string ("Mod+Shift..."
  // here is "Mod+K"/"Mod+W"/"Esc"; widest is 5). Every label should start
  // at the same column.
  const rowLines = lines.filter((l) => l.includes("Open command palette") || l.includes("Close tab"));
  expect(rowLines.length === 2, "text: both bindable rows rendered");
  const labelCol = rowLines.map((l) => l.indexOf(l.trimStart().split("  ").slice(-1)[0]));
  expect(labelCol[0] === labelCol[1], "text: labels aligned to the same column");
  // Section order preserved (Global before Tabs).
  expect(
    lines.indexOf("Global") < lines.indexOf("Tabs"),
    "text: sections in curated order",
  );
  // Info row included with its keys.
  expect(text.includes("Close overlay"), "text: info row exported too");
}

// --- formatShortcutsText: filtered subset ---------------------------
{
  const groups = buildShortcutGroups([
    { id: "g.k", label: "Open palette", group: "Global", binding: "Mod+K" },
    { id: "t.t", label: "Close tab", group: "Tabs", binding: "Mod+W" },
  ]);
  const filtered = filterShortcutGroups(groups, "palette");
  const text = formatShortcutsText(
    filtered,
    (r) => r.binding,
    { title: "Filtered" },
  );
  expect(text.includes("Open palette"), "text: filtered export includes the match");
  expect(!text.includes("Close tab"), "text: filtered export excludes non-matches");
}

// --- formatShortcutsText: no title + defensive ----------------------
{
  const groups = buildShortcutGroups([act("g.k", "Global", "Mod+K")]);
  const noTitle = formatShortcutsText(groups, (r) => r.binding);
  expect(noTitle.split("\n")[0] === "Global", "text: no title -> starts at first section");
  // Empty groups -> empty string (or just title).
  expect(formatShortcutsText([], (r) => r.binding) === "", "text: empty groups -> empty string");
  expect(
    formatShortcutsText([], (r) => r.binding, { title: "Only Title" }) === "Only Title",
    "text: empty groups with title -> just the title",
  );
  // @ts-expect-error null tolerance
  expect(formatShortcutsText(null, (r) => r.binding) === "", "text: null groups -> empty string");
  // Resolver returning "" never throws and aligns to label.
  const blankKeys = formatShortcutsText(groups, () => "");
  expect(blankKeys.includes("Global"), "text: blank resolver still renders sections");
}

// eslint-disable-next-line no-console
console.log(`\n${passed} passed, ${failed} failed`);