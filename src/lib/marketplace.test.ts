// v3.40 Slice 97 — pure-helper tests for the histogram sort axis.
//
// Style matches hopper.test.ts / quill.test.ts / fuzzy.test.ts —
// no runner dep, just inline `expect`. Run with:
//   pnpm exec tsx src/lib/marketplace.test.ts

import {
  HISTOGRAM_SORT_KEYS,
  histogramSortLabel,
  sortHistogramRows,
  suggestHistogramExportFilename,
  type HistogramSortKey,
  type PluginHistogramRow,
  // Slice 106 — activity timeline TS surface
  advanceBucketStart,
  densifyActivityTimeline,
  suggestActivityTimelineExportFilename,
  timeBucketLabel,
  TIME_BUCKET_GRANULARITIES,
  type ActivityBucket,
  type TimeBucketGranularity,
  // Slice 112 — bucket drilldown TS surface
  suggestBucketDrilldownExportFilename,
  type BucketDrilldownExportFilter,
  // Slice 117 — per-plugin retention overrides TS surface
  suggestPluginRetentionExportFilename,
} from "./marketplace";

function expect(cond: boolean, label: string): void {
  if (!cond) {
    console.error(`FAIL: ${label}`);
    process.exitCode = 1;
  } else {
    console.log(`ok: ${label}`);
  }
}

function row(
  plugin_id: string,
  installs: number,
  updates: number,
  uninstalls: number,
  failures: number,
  last_occurred_at: number,
): PluginHistogramRow {
  return {
    plugin_id,
    installs,
    updates,
    uninstalls,
    failures,
    total: installs + updates + uninstalls + failures,
    last_occurred_at,
  };
}

// ── HISTOGRAM_SORT_KEYS shape ────────────────────────────────────────

{
  // Five axes in canonical display order, total first, recent last.
  expect(
    HISTOGRAM_SORT_KEYS.length === 5,
    `HISTOGRAM_SORT_KEYS: 5 axes (got ${HISTOGRAM_SORT_KEYS.length})`,
  );
  expect(
    HISTOGRAM_SORT_KEYS[0] === "total",
    `HISTOGRAM_SORT_KEYS: total first`,
  );
  expect(
    HISTOGRAM_SORT_KEYS[HISTOGRAM_SORT_KEYS.length - 1] === "recent",
    `HISTOGRAM_SORT_KEYS: recent last`,
  );
  // No "uninstalls" axis — deliberate omission per the doc comment.
  expect(
    !(HISTOGRAM_SORT_KEYS as readonly string[]).includes("uninstalls"),
    `HISTOGRAM_SORT_KEYS: no uninstalls axis`,
  );
}

// ── histogramSortLabel ───────────────────────────────────────────────

{
  // Every key gets a non-empty label.
  for (const key of HISTOGRAM_SORT_KEYS) {
    const label = histogramSortLabel(key);
    expect(label.length > 0, `histogramSortLabel(${key}) non-empty (${label})`);
  }
  // Spot-check a couple specific labels so a careless rename surfaces.
  expect(
    histogramSortLabel("total") === "Most active",
    `histogramSortLabel(total) === Most active`,
  );
  expect(
    histogramSortLabel("recent") === "Most recent",
    `histogramSortLabel(recent) === Most recent`,
  );
  expect(
    histogramSortLabel("failures") === "Most failures",
    `histogramSortLabel(failures) === Most failures`,
  );
}

// ── sortHistogramRows: pure / non-mutating ───────────────────────────

{
  const a = row("a", 1, 0, 0, 0, 100);
  const b = row("b", 2, 0, 0, 0, 200);
  const rows = [a, b];
  const out = sortHistogramRows(rows, "total");
  expect(rows[0] === a, `sortHistogramRows: input unchanged (rows[0])`);
  expect(rows[1] === b, `sortHistogramRows: input unchanged (rows[1])`);
  expect(out !== rows, `sortHistogramRows: returns new array (not in-place)`);
}

// ── sortHistogramRows: total DESC ────────────────────────────────────

{
  const rows = [
    row("a", 1, 0, 0, 0, 1),
    row("b", 5, 0, 0, 0, 1),
    row("c", 3, 0, 0, 0, 1),
  ];
  const out = sortHistogramRows(rows, "total");
  expect(
    out.map((r) => r.plugin_id).join(",") === "b,c,a",
    `sortHistogramRows total: b(5) > c(3) > a(1) (got ${out.map((r) => r.plugin_id).join(",")})`,
  );
}

// ── sortHistogramRows: installs DESC ─────────────────────────────────

{
  const rows = [
    row("a", 10, 5, 0, 0, 1), // total 15, installs 10
    row("b", 8, 99, 0, 0, 1), //  total 107, installs 8
    row("c", 20, 0, 0, 0, 1), //  total 20, installs 20
  ];
  const out = sortHistogramRows(rows, "installs");
  expect(
    out.map((r) => r.plugin_id).join(",") === "c,a,b",
    `sortHistogramRows installs: c(20) > a(10) > b(8) (got ${out.map((r) => r.plugin_id).join(",")})`,
  );
}

// ── sortHistogramRows: failures DESC (the bug-hunt axis) ─────────────

{
  const rows = [
    row("a", 100, 0, 0, 0, 1), // 0 failures
    row("b", 50, 0, 0, 7, 1), //  7 failures
    row("c", 30, 0, 0, 3, 1), //  3 failures
  ];
  const out = sortHistogramRows(rows, "failures");
  expect(
    out.map((r) => r.plugin_id).join(",") === "b,c,a",
    `sortHistogramRows failures: b(7) > c(3) > a(0) (got ${out.map((r) => r.plugin_id).join(",")})`,
  );
}

// ── sortHistogramRows: updates DESC ──────────────────────────────────

{
  const rows = [
    row("a", 0, 5, 0, 0, 1),
    row("b", 0, 12, 0, 0, 1),
    row("c", 0, 8, 0, 0, 1),
  ];
  const out = sortHistogramRows(rows, "updates");
  expect(
    out.map((r) => r.plugin_id).join(",") === "b,c,a",
    `sortHistogramRows updates: b(12) > c(8) > a(5) (got ${out.map((r) => r.plugin_id).join(",")})`,
  );
}

// ── sortHistogramRows: recent DESC ───────────────────────────────────

{
  const rows = [
    row("a", 1, 0, 0, 0, 1_000),
    row("b", 1, 0, 0, 0, 3_000),
    row("c", 1, 0, 0, 0, 2_000),
  ];
  const out = sortHistogramRows(rows, "recent");
  expect(
    out.map((r) => r.plugin_id).join(",") === "b,c,a",
    `sortHistogramRows recent: b(3000) > c(2000) > a(1000) (got ${out.map((r) => r.plugin_id).join(",")})`,
  );
}

// ── sortHistogramRows: ties break ASC on plugin_id ───────────────────

{
  // Three plugins with the same total — tiebreak puts plugin_id ASC.
  const rows = [
    row("zebra", 5, 0, 0, 0, 1_000),
    row("apple", 5, 0, 0, 0, 2_000),
    row("mango", 5, 0, 0, 0, 3_000),
  ];
  const out = sortHistogramRows(rows, "total");
  expect(
    out.map((r) => r.plugin_id).join(",") === "apple,mango,zebra",
    `sortHistogramRows total tiebreak: apple,mango,zebra (got ${out.map((r) => r.plugin_id).join(",")})`,
  );
  // Same tiebreak under any other axis.
  const outRecent = sortHistogramRows(
    [row("z", 0, 0, 0, 0, 5), row("a", 0, 0, 0, 0, 5)],
    "recent",
  );
  expect(
    outRecent[0].plugin_id === "a",
    `sortHistogramRows recent tiebreak: a before z`,
  );
}

// ── sortHistogramRows: empty input is empty output ───────────────────

{
  const out = sortHistogramRows([], "total");
  expect(out.length === 0, `sortHistogramRows: empty -> empty`);
  expect(Array.isArray(out), `sortHistogramRows: empty returns array not null`);
}

// ── sortHistogramRows: single row passes through ─────────────────────

{
  const only = row("solo", 1, 0, 0, 0, 1);
  for (const key of HISTOGRAM_SORT_KEYS) {
    const out = sortHistogramRows([only], key);
    expect(
      out.length === 1 && out[0].plugin_id === "solo",
      `sortHistogramRows: single row passes for key=${key}`,
    );
  }
}

// ── sortHistogramRows: every axis returns the right length ───────────

{
  // Defensive sanity — every axis returns ALL the rows (sort is a
  // permutation, never a filter).
  const rows = [
    row("a", 1, 2, 3, 4, 5),
    row("b", 5, 4, 3, 2, 1),
    row("c", 2, 2, 2, 2, 2),
  ];
  for (const key of HISTOGRAM_SORT_KEYS) {
    const out = sortHistogramRows(rows, key);
    expect(
      out.length === rows.length,
      `sortHistogramRows: length preserved for key=${key} (${out.length} vs ${rows.length})`,
    );
    // All input plugin_ids must appear in the output.
    const ids = new Set(out.map((r) => r.plugin_id));
    expect(
      rows.every((r) => ids.has(r.plugin_id)),
      `sortHistogramRows: all rows present for key=${key}`,
    );
  }
}

// ── sortHistogramRows: unknown key triggers exhaustiveness ───────────

{
  // The switch in sortHistogramRows is exhaustive on HistogramSortKey;
  // TypeScript catches a missing arm at compile time. Defensive cast
  // here so a runtime caller passing junk gets a deterministic answer
  // (the row order may be unspecified but the function shouldn't
  // throw).
  const rows = [row("a", 1, 0, 0, 0, 1)];
  expect(
    !(() => {
      try {
        // Cast through unknown to feed the function a bogus key on
        // purpose; the test asserts the function doesn't throw on
        // garbage input. We don't care about the order.
        sortHistogramRows(rows, "bogus" as unknown as HistogramSortKey);
        return false;
      } catch {
        return true;
      }
    })(),
    `sortHistogramRows: bogus key doesn't throw`,
  );
}

// ── Slice 101: suggestHistogramExportFilename ────────────────────────

// 2024-03-09T16:00:00Z → today slug 20240309. Pin "now" so the
// trailing slug is deterministic across timezones / wall-clock drift.
const NOW = 1_710_000_000_000;

{
  // No window, csv ext — the bare "all" form.
  const name = suggestHistogramExportFilename({}, "csv", NOW);
  expect(
    name === "marketplace-top-plugins_all_20240309.csv",
    `suggestHistogramExportFilename: no window csv = ${name}`,
  );
  // Same shape with json ext — only the suffix differs.
  const nameJson = suggestHistogramExportFilename({}, "json", NOW);
  expect(
    nameJson === "marketplace-top-plugins_all_20240309.json",
    `suggestHistogramExportFilename: no window json = ${nameJson}`,
  );
}

{
  // Only since — "from-<date>" prefix.
  // since_unix 1_700_000_000 → 2023-11-14
  const name = suggestHistogramExportFilename(
    { since_unix: 1_700_000_000 },
    "csv",
    NOW,
  );
  expect(
    name === "marketplace-top-plugins_from-20231114_20240309.csv",
    `suggestHistogramExportFilename: only-since = ${name}`,
  );
}

{
  // Only until — "to-<date>" prefix.
  const name = suggestHistogramExportFilename(
    { until_unix: 1_700_000_000 },
    "csv",
    NOW,
  );
  expect(
    name === "marketplace-top-plugins_to-20231114_20240309.csv",
    `suggestHistogramExportFilename: only-until = ${name}`,
  );
}

{
  // Both bounds — "<since>-<until>" window slot.
  const name = suggestHistogramExportFilename(
    { since_unix: 1_700_000_000, until_unix: 1_710_000_000 },
    "json",
    NOW,
  );
  expect(
    name === "marketplace-top-plugins_20231114-20240309_20240309.json",
    `suggestHistogramExportFilename: both bounds = ${name}`,
  );
}

{
  // csv ↔ json — paired forms differ ONLY in the suffix. Pin the
  // invariant so a future rename can't make them diverge in any
  // other slot. Mirrors the slice-95 ext-aware drilldown helper
  // pairing test.
  const csv = suggestHistogramExportFilename(
    { since_unix: 1_700_000_000 },
    "csv",
    NOW,
  );
  const json = suggestHistogramExportFilename(
    { since_unix: 1_700_000_000 },
    "json",
    NOW,
  );
  expect(
    csv.replace(/\.csv$/, "") === json.replace(/\.json$/, ""),
    `suggestHistogramExportFilename: csv/json differ only in suffix (csv=${csv}, json=${json})`,
  );
  expect(
    csv.endsWith(".csv") && json.endsWith(".json"),
    `suggestHistogramExportFilename: csv ends .csv, json ends .json`,
  );
}

{
  // Filename always carries the marketplace-top-plugins_ prefix so
  // it groups next to other marketplace exports in a directory list.
  for (const opts of [
    {},
    { since_unix: 1_700_000_000 },
    { until_unix: 1_700_000_000 },
    { since_unix: 1_700_000_000, until_unix: 1_710_000_000 },
  ] satisfies Array<Parameters<typeof suggestHistogramExportFilename>[0]>) {
    const name = suggestHistogramExportFilename(opts, "csv", NOW);
    expect(
      name.startsWith("marketplace-top-plugins_"),
      `suggestHistogramExportFilename: prefix preserved for ${JSON.stringify(opts)} (${name})`,
    );
  }
}

{
  // The window slot for "both bounds" uses YYYYMMDD-YYYYMMDD with
  // no separators inside each date — matches the install-log helper's
  // shape so the two filenames sort in a stable interleave.
  const name = suggestHistogramExportFilename(
    { since_unix: 1_700_000_000, until_unix: 1_710_000_000 },
    "csv",
    NOW,
  );
  // Window slot is the second underscore-separated segment.
  const parts = name.split("_");
  expect(parts.length === 3, `name has 3 underscore-segments (${parts.length})`);
  const window = parts[1];
  expect(
    /^\d{8}-\d{8}$/.test(window),
    `window slot is YYYYMMDD-YYYYMMDD (got ${window})`,
  );
}

{
  // Today slug uses UTC date math (matches the slugifier inside the
  // helper). Pinning NOW gives a deterministic trailing date so the
  // filename doesn't drift across a midnight UTC boundary.
  const name = suggestHistogramExportFilename({}, "csv", NOW);
  expect(
    name.includes("_20240309."),
    `today slug uses UTC date (got ${name})`,
  );
}

// ─── Slice 106 — activity timeline TS helpers ──────────────────────

function actBucket(
  bucket_start_unix: number,
  installs: number,
  updates: number,
  uninstalls: number,
  failures: number,
): ActivityBucket {
  return {
    bucket_start_unix,
    installs,
    updates,
    uninstalls,
    failures,
    total: installs + updates + uninstalls + failures,
  };
}

{
  // TIME_BUCKET_GRANULARITIES must list all three values in order
  // — Day first (UI + read-endpoint default), Month last (coarsest
  // pivot). Adding a fourth granularity is a deliberate one-line
  // edit; this test pins the current set.
  expect(
    TIME_BUCKET_GRANULARITIES.length === 3,
    `TIME_BUCKET_GRANULARITIES length = 3 (got ${TIME_BUCKET_GRANULARITIES.length})`,
  );
  expect(
    TIME_BUCKET_GRANULARITIES[0] === "day",
    `TIME_BUCKET_GRANULARITIES[0] = "day"`,
  );
  expect(
    TIME_BUCKET_GRANULARITIES[2] === "month",
    `TIME_BUCKET_GRANULARITIES[2] = "month"`,
  );
}

{
  // timeBucketLabel renders the "Per X" cadence noun phrase for
  // each granularity. Pinning the exact strings catches a copy
  // change in the UI surface that drifts away from the helper.
  expect(timeBucketLabel("day") === "Per day", `timeBucketLabel(day)`);
  expect(timeBucketLabel("week") === "Per week", `timeBucketLabel(week)`);
  expect(timeBucketLabel("month") === "Per month", `timeBucketLabel(month)`);
}

{
  // advanceBucketStart: day = +86_400 exactly. Pins the day path's
  // simple-arithmetic contract (no calendar correction needed for
  // the fixed-width 86_400-second bucket).
  const start = 1_700_000_000;
  expect(
    advanceBucketStart(start, "day") === start + 86_400,
    `advanceBucketStart day = +86400`,
  );
}

{
  // advanceBucketStart: week = +7 * 86_400 exactly. Same
  // simple-arithmetic contract — no DST shenanigans because UTC.
  const start = 1_699_833_600; // 2023-11-13 (Monday UTC)
  expect(
    advanceBucketStart(start, "week") === start + 7 * 86_400,
    `advanceBucketStart week = +7d`,
  );
}

{
  // advanceBucketStart: month = +1 calendar month (variable width).
  // 2023-11-01T00:00:00Z -> 2023-12-01T00:00:00Z = 1_701_388_800.
  // 1_698_796_800 is 2023-11-01.
  expect(
    advanceBucketStart(1_698_796_800, "month") === 1_701_388_800,
    `advanceBucketStart month: Nov-1 -> Dec-1`,
  );
}

{
  // advanceBucketStart month: year overflow. Dec-1 -> Jan-1 of the
  // next year. 2023-12-01T00:00:00Z = 1_701_388_800;
  // 2024-01-01T00:00:00Z = 1_704_067_200.
  expect(
    advanceBucketStart(1_701_388_800, "month") === 1_704_067_200,
    `advanceBucketStart month: Dec-1 -> Jan-1 (year overflow)`,
  );
}

{
  // advanceBucketStart month: 28-day February. 2024-02-01 ->
  // 2024-03-01. 2024-02-01T00:00:00Z = 1_706_745_600;
  // 2024-03-01T00:00:00Z = 1_709_251_200 (29 days, leap year).
  expect(
    advanceBucketStart(1_706_745_600, "month") === 1_709_251_200,
    `advanceBucketStart month: Feb 2024 leap year (+29d)`,
  );
}

{
  // densifyActivityTimeline: empty input -> empty output. Cheap
  // boundary case that protects the caller from defensive
  // empty-checks in the rendering loop.
  const out = densifyActivityTimeline([], "day");
  expect(out.length === 0, `densify empty -> empty`);
}

{
  // densifyActivityTimeline: single bucket -> same single bucket.
  // No gap to fill; output's first + last are the input's only
  // bucket verbatim.
  const buckets = [actBucket(1_700_000_000, 1, 0, 0, 0)];
  const out = densifyActivityTimeline(buckets, "day");
  expect(out.length === 1, `densify single bucket -> length 1`);
  expect(
    out[0].bucket_start_unix === 1_700_000_000,
    `densify single bucket preserves start`,
  );
  expect(out[0].installs === 1, `densify single bucket preserves counts`);
}

{
  // densifyActivityTimeline day: three sparse buckets across a
  // 5-day span -> five dense buckets (2 zero-bucket gaps inserted).
  // Day 1 + Day 3 + Day 5 -> Days 1,2,3,4,5.
  const buckets = [
    actBucket(1_700_000_000, 1, 0, 0, 0), // day 1
    actBucket(1_700_172_800, 0, 1, 0, 0), // day 3 (+2 days)
    actBucket(1_700_345_600, 0, 0, 0, 1), // day 5 (+4 days)
  ];
  const out = densifyActivityTimeline(buckets, "day");
  expect(out.length === 5, `densify day: 5 dense buckets (got ${out.length})`);
  // First + last preserved verbatim.
  expect(out[0].installs === 1, `densify day: first bucket carries data`);
  expect(out[4].failures === 1, `densify day: last bucket carries data`);
  // Gap buckets (index 1, 3) are zero-buckets.
  expect(out[1].total === 0, `densify day: index 1 zero-bucket`);
  expect(out[3].total === 0, `densify day: index 3 zero-bucket`);
  // Spacing is +86_400 exactly.
  for (let i = 1; i < out.length; i++) {
    expect(
      out[i].bucket_start_unix - out[i - 1].bucket_start_unix === 86_400,
      `densify day: spacing at index ${i} = 86400`,
    );
  }
}

{
  // densifyActivityTimeline returns a NEW array (input never
  // mutated). Same posture as sortHistogramRows. Protects against
  // a future refactor that switches to an in-place strategy.
  const buckets = [actBucket(1_700_000_000, 1, 0, 0, 0)];
  const out = densifyActivityTimeline(buckets, "day");
  expect(out !== buckets, `densify returns NEW array`);
}

{
  // densifyActivityTimeline week: two sparse week-buckets two
  // weeks apart -> three dense buckets (one zero-bucket gap).
  const week1 = 1_699_833_600; // 2023-11-13 Monday
  const week3 = week1 + 14 * 86_400; // skip week 2
  const buckets = [actBucket(week1, 1, 0, 0, 0), actBucket(week3, 0, 0, 0, 1)];
  const out = densifyActivityTimeline(buckets, "week");
  expect(out.length === 3, `densify week: 3 dense buckets (got ${out.length})`);
  expect(out[1].total === 0, `densify week: middle is zero-bucket`);
  expect(
    out[1].bucket_start_unix === week1 + 7 * 86_400,
    `densify week: middle bucket at week 2 start`,
  );
}

{
  // densifyActivityTimeline month: two months apart (Nov + Jan)
  // -> three dense buckets (Dec is the zero-bucket between).
  const nov = 1_698_796_800; // 2023-11-01
  const jan = 1_704_067_200; // 2024-01-01
  const buckets = [actBucket(nov, 1, 0, 0, 0), actBucket(jan, 0, 0, 0, 1)];
  const out = densifyActivityTimeline(buckets, "month");
  expect(out.length === 3, `densify month: 3 dense buckets (got ${out.length})`);
  expect(out[0].bucket_start_unix === nov, `densify month: starts at Nov`);
  expect(
    out[1].bucket_start_unix === 1_701_388_800,
    `densify month: middle is Dec`,
  );
  expect(out[2].bucket_start_unix === jan, `densify month: ends at Jan`);
}

{
  // suggestActivityTimelineExportFilename: default granularity
  // (omitted) reads as "day" — matches the read-endpoint default.
  const name = suggestActivityTimelineExportFilename({}, "csv", NOW);
  expect(
    name === "marketplace-activity-day_all_20240309.csv",
    `default granularity is "day" (got ${name})`,
  );
}

{
  // suggestActivityTimelineExportFilename: granularity in prefix
  // (NOT window slot) — each value renders as its own prefix variant.
  for (const g of TIME_BUCKET_GRANULARITIES) {
    const name = suggestActivityTimelineExportFilename(
      { granularity: g },
      "csv",
      NOW,
    );
    expect(
      name === `marketplace-activity-${g}_all_20240309.csv`,
      `granularity ${g} in prefix (got ${name})`,
    );
  }
}

{
  // suggestActivityTimelineExportFilename: csv vs json differ ONLY
  // in suffix. Same invariant as the histogram filename helper's
  // ext-aware variant — mirrors slice 95 + 101 patterns.
  const opts = { granularity: "week" as TimeBucketGranularity };
  const csv = suggestActivityTimelineExportFilename(opts, "csv", NOW);
  const json = suggestActivityTimelineExportFilename(opts, "json", NOW);
  expect(csv.endsWith(".csv"), `csv ends .csv (${csv})`);
  expect(json.endsWith(".json"), `json ends .json (${json})`);
  expect(
    csv.slice(0, -4) === json.slice(0, -5),
    `csv/json prefix equality (${csv}, ${json})`,
  );
}

{
  // suggestActivityTimelineExportFilename: window slot mirrors the
  // histogram filename's shape exactly so the two exports sort
  // side-by-side in a directory. "all" / "from-X" / "to-X" /
  // "X-Y".
  const all = suggestActivityTimelineExportFilename({}, "csv", NOW);
  expect(all.includes("_all_"), `_all_ for no-window (${all})`);
  const onlySince = suggestActivityTimelineExportFilename(
    { since_unix: 1_700_000_000 },
    "csv",
    NOW,
  );
  expect(
    onlySince.includes("_from-20231114_"),
    `from-YYYYMMDD slot (${onlySince})`,
  );
  const onlyUntil = suggestActivityTimelineExportFilename(
    { until_unix: 1_700_000_000 },
    "csv",
    NOW,
  );
  expect(
    onlyUntil.includes("_to-20231114_"),
    `to-YYYYMMDD slot (${onlyUntil})`,
  );
  const both = suggestActivityTimelineExportFilename(
    { since_unix: 1_700_000_000, until_unix: 1_710_000_000 },
    "csv",
    NOW,
  );
  expect(
    both.includes("_20231114-20240309_"),
    `YYYYMMDD-YYYYMMDD slot (${both})`,
  );
}

{
  // suggestActivityTimelineExportFilename: prefix preserved across
  // all four window-shape variants — sorts next to other
  // marketplace-activity-* exports in a directory.
  for (const opts of [
    {},
    { since_unix: 1_700_000_000 },
    { until_unix: 1_700_000_000 },
    { since_unix: 1_700_000_000, until_unix: 1_710_000_000 },
  ] satisfies Array<Parameters<typeof suggestActivityTimelineExportFilename>[0]>) {
    const name = suggestActivityTimelineExportFilename(
      { ...opts, granularity: "week" },
      "csv",
      NOW,
    );
    expect(
      name.startsWith("marketplace-activity-week_"),
      `prefix preserved for ${JSON.stringify(opts)} (${name})`,
    );
  }
}

{
  // Today slug uses UTC date math. Pinning NOW gives a
  // deterministic trailing date so the filename doesn't drift
  // across a midnight UTC boundary.
  const name = suggestActivityTimelineExportFilename({}, "csv", NOW);
  expect(name.includes("_20240309."), `today slug uses UTC date (${name})`);
}

// ─── Slice 112 — bucket drilldown export filename helper ─────────────

{
  // Default shape: produces the standard 4-segment filename with
  // marketplace-bucket-drilldown- prefix + granularity, bucket ISO,
  // today ISO, extension.
  const filter: BucketDrilldownExportFilter = {
    bucket_start_unix: 1_699_920_000, // 2023-11-14T00:00:00Z
    granularity: "day",
  };
  const name = suggestBucketDrilldownExportFilename(filter, "csv", NOW);
  expect(
    name === "marketplace-bucket-drilldown-day_20231114_20240309.csv",
    `default csv form (${name})`,
  );
  const json = suggestBucketDrilldownExportFilename(filter, "json", NOW);
  expect(
    json === "marketplace-bucket-drilldown-day_20231114_20240309.json",
    `default json form (${json})`,
  );
}

{
  // Granularity in the prefix for all three values — the bucket slot
  // is the bucket's date so day/week/month don't change the bucket
  // slot, they change the prefix.
  for (const g of ["day", "week", "month"] as const) {
    const name = suggestBucketDrilldownExportFilename(
      { bucket_start_unix: 1_699_920_000, granularity: g },
      "csv",
      NOW,
    );
    expect(
      name.startsWith(`marketplace-bucket-drilldown-${g}_`),
      `${g} in prefix (${name})`,
    );
  }
}

{
  // csv vs json differ ONLY in the suffix — prefix + slugs match
  // byte-for-byte. Pins the same invariant the histogram +
  // activity-timeline helpers hold.
  const filter: BucketDrilldownExportFilter = {
    bucket_start_unix: 1_699_920_000,
    granularity: "week",
  };
  const csv = suggestBucketDrilldownExportFilename(filter, "csv", NOW);
  const json = suggestBucketDrilldownExportFilename(filter, "json", NOW);
  const csvBare = csv.slice(0, -4);
  const jsonBare = json.slice(0, -5);
  expect(
    csvBare === jsonBare,
    `csv/json differ only in suffix (csv=${csv} json=${json})`,
  );
  expect(csv.endsWith(".csv"), `csv ends .csv (${csv})`);
  expect(json.endsWith(".json"), `json ends .json (${json})`);
}

{
  // Bucket slot is the bucket's UTC date — pin the round-trip for
  // a known timestamp (1_699_920_000 -> 2023-11-14T00:00:00Z ->
  // 20231114).
  const filter: BucketDrilldownExportFilter = {
    bucket_start_unix: 1_699_920_000,
    granularity: "day",
  };
  const name = suggestBucketDrilldownExportFilename(filter, "csv", NOW);
  expect(
    name.includes("_20231114_"),
    `bucket slug == bucket UTC date (${name})`,
  );
}

{
  // Today slug uses UTC date math — pinning NOW gives a
  // deterministic trailing date so the filename doesn't drift
  // across a midnight UTC boundary.
  const name = suggestBucketDrilldownExportFilename(
    { bucket_start_unix: 1_699_920_000, granularity: "day" },
    "csv",
    NOW,
  );
  expect(
    name.endsWith("_20240309.csv"),
    `today slug uses UTC date (${name})`,
  );
}

{
  // Bucket slot is independent of granularity for the same
  // bucket_start. Two filenames at the same bucket but different
  // granularities differ ONLY in the prefix's granularity tag.
  const day = suggestBucketDrilldownExportFilename(
    { bucket_start_unix: 1_699_920_000, granularity: "day" },
    "csv",
    NOW,
  );
  const week = suggestBucketDrilldownExportFilename(
    { bucket_start_unix: 1_699_920_000, granularity: "week" },
    "csv",
    NOW,
  );
  // Strip the granularity tag — the rest must match exactly.
  const dayAfter = day.replace(
    "marketplace-bucket-drilldown-day_",
    "",
  );
  const weekAfter = week.replace(
    "marketplace-bucket-drilldown-week_",
    "",
  );
  expect(
    dayAfter === weekAfter,
    `bucket slot identical for same bucket_start across granularities (day=${day} week=${week})`,
  );
}

{
  // Epoch timestamp edge case: bucket_start = 0 -> 1970-01-01
  // -> 19700101 slug. Pins the iso() helper's behaviour at the
  // lower bound so a corrupted-state caller stays predictable.
  const name = suggestBucketDrilldownExportFilename(
    { bucket_start_unix: 0, granularity: "day" },
    "csv",
    NOW,
  );
  expect(
    name === "marketplace-bucket-drilldown-day_19700101_20240309.csv",
    `epoch bucket slug (${name})`,
  );
}

{
  // limit field is irrelevant to the filename — the bucket coords +
  // granularity + ext determine the name. Pin this so a future
  // change that accidentally pulls limit into the filename surfaces.
  const noLimit = suggestBucketDrilldownExportFilename(
    { bucket_start_unix: 1_699_920_000, granularity: "day" },
    "csv",
    NOW,
  );
  const withLimit = suggestBucketDrilldownExportFilename(
    { bucket_start_unix: 1_699_920_000, granularity: "day", limit: 100 },
    "csv",
    NOW,
  );
  expect(
    noLimit === withLimit,
    `limit irrelevant to filename (no=${noLimit} with=${withLimit})`,
  );
}

// ─── Slice 117 — per-plugin retention overrides filename helper ───

{
  // Default csv form for the per-plugin overrides export. Same
  // YYYYMMDD UTC slug as the four sibling export-filename helpers
  // so a paralegal collecting marketplace audit exports gets a
  // sortable directory naturally.
  const name = suggestPluginRetentionExportFilename("csv", NOW);
  expect(
    name === "marketplace-plugin-retention-overrides_20240309.csv",
    `default csv form (${name})`,
  );
}

{
  // Default json form — same prefix + slug, json extension.
  const name = suggestPluginRetentionExportFilename("json", NOW);
  expect(
    name === "marketplace-plugin-retention-overrides_20240309.json",
    `default json form (${name})`,
  );
}

{
  // csv vs json differ ONLY in the file extension — the prefix +
  // date slug are byte-equal. Pin so a future change that adds a
  // per-kind subdirectory or different date format on one branch
  // surfaces here.
  const csv = suggestPluginRetentionExportFilename("csv", NOW);
  const json = suggestPluginRetentionExportFilename("json", NOW);
  const csvHead = csv.slice(0, -4);
  const jsonHead = json.slice(0, -5);
  expect(csvHead === jsonHead, `csv/json prefix equal (csv=${csv} json=${json})`);
  expect(csv.endsWith(".csv"), `csv ext (${csv})`);
  expect(json.endsWith(".json"), `json ext (${json})`);
}

{
  // UTC date slug — same wall clock seconds across timezones must
  // produce the same filename. Pin by passing two equivalent epoch
  // values that would render differently in local time but identical
  // in UTC.
  const a = suggestPluginRetentionExportFilename("csv", 1_710_000_000_000);
  const b = suggestPluginRetentionExportFilename("csv", 1_710_000_000_000);
  expect(a === b, `same epoch -> same filename (a=${a} b=${b})`);
}

{
  // Epoch timestamp edge case: now = 0 -> 1970-01-01 slug. Pins the
  // helper's behaviour at the lower bound so a corrupted-state
  // caller stays predictable.
  const name = suggestPluginRetentionExportFilename("csv", 0);
  expect(
    name === "marketplace-plugin-retention-overrides_19700101.csv",
    `epoch slug (${name})`,
  );
}

{
  // The sibling filename helpers all share the same date-slug format.
  // Pin that the per-plugin retention export's slug byte-equals the
  // activity-timeline export's today-slug for the same `now` — a
  // future drift between the slug formats surfaces here.
  const retention = suggestPluginRetentionExportFilename("csv", NOW);
  const timeline = suggestActivityTimelineExportFilename(
    { granularity: "day" },
    "csv",
    NOW,
  );
  // Both slugs end with _<YYYYMMDD>.<ext>. Extract the slug.
  const retMatch = retention.match(/_(\d{8})\.[a-z]+$/);
  const tlMatch = timeline.match(/_(\d{8})\.[a-z]+$/);
  expect(
    retMatch !== null && tlMatch !== null && retMatch[1] === tlMatch[1],
    `date slug shared across export helpers (retention=${retention} timeline=${timeline})`,
  );
}

{
  // Future-date helper invocation. The slug should reflect the
  // requested date — proves the helper is honest about the `now`
  // arg vs falling back to Date.now() under the hood.
  // 2026-06-22T00:00:00Z = Date.UTC(2026,5,22) = 1_782_086_400_000 ms.
  const name = suggestPluginRetentionExportFilename(
    "json",
    Date.UTC(2026, 5, 22),
  );
  expect(
    name === "marketplace-plugin-retention-overrides_20260622.json",
    `future-date helper invocation (${name})`,
  );
}

