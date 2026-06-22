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
