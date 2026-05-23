#!/usr/bin/env node
// scripts/loom/codegen-matterhorn.mjs
//
// Reads docs/specs/matterhorn-1.1.json (the single source of truth for the
// Matterhorn Protocol 1.1 failure-condition registry) and emits a fully-typed
// Rust module at src-tauri/src/pdf/loom/matterhorn.rs.
//
// The generated module contains:
//
//   * Verdict enum  (Auto / Human / OutOfScope)
//   * FailureCondition struct
//   * Section struct
//   * Two const arrays:
//        SECTIONS      — every section with its failure conditions
//        CONDITIONS    — flat slice of every failure condition for fast lookup
//   * Totals struct + TOTALS const (mirrors the totals block from JSON)
//   * Helper fns:
//        find_condition(id)         -> Option<&FailureCondition>
//        section_by_id(id)          -> Option<&Section>
//        auto_conditions()          -> impl Iterator<Item = &FailureCondition>
//        human_conditions()         -> impl Iterator<Item = &FailureCondition>
//        out_of_scope_conditions()  -> impl Iterator<Item = &FailureCondition>
//
// Run with:
//
//     node scripts/loom/codegen-matterhorn.mjs
//
// or via the wrapper:
//
//     pnpm loom:codegen
//
// Idempotent: byte-identical output across runs for the same JSON input. CI
// can run this in --check mode and fail if matterhorn.rs is stale.
//
// Why codegen instead of a build.rs? Build.rs runs every `cargo build` and
// adds 200-400 ms to incremental builds. Generating ahead of time keeps the
// hot path fast and makes the registry trivially reviewable in git diffs.

import { readFile, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { argv, exit } from "node:process";
import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";

const HERE = dirname(fileURLToPath(import.meta.url));
const REPO = resolve(HERE, "..", "..");
const SRC_JSON = resolve(REPO, "docs/specs/matterhorn-1.1.json");
const SRC_SCHEMA = resolve(REPO, "docs/specs/matterhorn-1.1.schema.json");
const OUT_RS = resolve(REPO, "src-tauri/src/pdf/loom/matterhorn.rs");

const VERDICT_VARIANT = {
  auto: "Auto",
  human: "Human",
  outOfScope: "OutOfScope",
};

function bail(msg) {
  console.error(`[codegen-matterhorn] ERROR: ${msg}`);
  exit(1);
}

function checkMode() {
  return argv.includes("--check");
}

function validate(registry) {
  const errs = [];
  if (registry.protocol !== "Matterhorn Protocol")
    errs.push(`protocol must be "Matterhorn Protocol"`);
  if (!/^\d+\.\d+$/.test(registry.version ?? ""))
    errs.push(`version must match \\d+\\.\\d+`);
  if (!Array.isArray(registry.sections) || registry.sections.length === 0)
    errs.push(`sections must be a non-empty array`);

  const seenSection = new Set();
  const seenCondition = new Set();
  let countAuto = 0;
  let countHuman = 0;
  let countOoS = 0;

  for (const sec of registry.sections ?? []) {
    if (!/^\d{2}$/.test(sec.id ?? ""))
      errs.push(`section id "${sec.id}" must match \\d\\d`);
    if (seenSection.has(sec.id))
      errs.push(`duplicate section id ${sec.id}`);
    seenSection.add(sec.id);
    if (!sec.title) errs.push(`section ${sec.id} has empty title`);
    if (!Array.isArray(sec.failureConditions) || sec.failureConditions.length === 0)
      errs.push(`section ${sec.id} has no failureConditions`);

    for (const fc of sec.failureConditions ?? []) {
      if (!/^\d{2}-\d{3}$/.test(fc.id ?? ""))
        errs.push(`condition id "${fc.id}" must match \\d\\d-\\d\\d\\d`);
      if (!fc.id?.startsWith(`${sec.id}-`))
        errs.push(`condition ${fc.id} does not belong to section ${sec.id}`);
      if (seenCondition.has(fc.id))
        errs.push(`duplicate condition id ${fc.id}`);
      seenCondition.add(fc.id);
      if (!fc.title) errs.push(`condition ${fc.id} has empty title`);
      if (!VERDICT_VARIANT[fc.verdict])
        errs.push(`condition ${fc.id} has unknown verdict "${fc.verdict}"`);
      else {
        if (fc.verdict === "auto") countAuto++;
        else if (fc.verdict === "human") countHuman++;
        else countOoS++;
      }
    }
  }

  // Cross-check totals.
  const totals = registry.totals ?? {};
  if (totals.auto !== countAuto)
    errs.push(`totals.auto=${totals.auto} but counted ${countAuto}`);
  if (totals.human !== countHuman)
    errs.push(`totals.human=${totals.human} but counted ${countHuman}`);
  if (totals.outOfScope_v3_1_0 !== countOoS)
    errs.push(
      `totals.outOfScope_v3_1_0=${totals.outOfScope_v3_1_0} but counted ${countOoS}`,
    );
  if (totals.sections !== seenSection.size)
    errs.push(
      `totals.sections=${totals.sections} but registry has ${seenSection.size}`,
    );
  if (totals.failureConditions_inThisRegistry !== seenCondition.size)
    errs.push(
      `totals.failureConditions_inThisRegistry=${totals.failureConditions_inThisRegistry} but registry has ${seenCondition.size}`,
    );

  if (errs.length) {
    for (const e of errs) console.error(`[validate] ${e}`);
    bail(`${errs.length} validation error(s) in ${SRC_JSON}`);
  }

  return { countAuto, countHuman, countOoS, total: seenCondition.size };
}

function rustString(s) {
  // Rust string literal — escape \ and " and stay ASCII-safe. Source is ASCII
  // except em-dashes used as placeholder in isoClause; encode those as \u{...}.
  return (
    '"' +
    [...s]
      .map((ch) => {
        const cp = ch.codePointAt(0);
        if (ch === "\\") return "\\\\";
        if (ch === '"') return '\\"';
        if (cp < 0x20 || cp > 0x7e) return `\\u{${cp.toString(16)}}`;
        return ch;
      })
      .join("") +
    '"'
  );
}

function genRust(registry, counts) {
  const lines = [];
  const p = (s = "") => lines.push(s);

  p("// @generated — DO NOT EDIT BY HAND.");
  p("// Source: docs/specs/matterhorn-1.1.json");
  p("// Schema: docs/specs/matterhorn-1.1.schema.json");
  p("// Generator: scripts/loom/codegen-matterhorn.mjs");
  p("//");
  p("// To regenerate, run `pnpm loom:codegen` from the repo root.");
  p("// CI verifies this file is in sync via `pnpm loom:codegen --check`.");
  p("//");
  p("// Matterhorn Protocol 1.1, published by the PDF Association (2021),");
  p("// covers ISO 14289-1:2014/Amd.1:2018 (PDF/UA-1). See");
  p("// docs/adr/2026-05-23-pdf-ua-conformance.md for the conformance target.");
  p("");
  p("#![allow(dead_code)] // helpers consumed in Slice 2+ of v3.1.0 Loom");
  p("");
  p("use serde::Serialize;");
  p("");
  p("/// Whether a Matterhorn failure condition is decidable by the validate");
  p("/// pass alone (`Auto`), requires human review (`Human`), or depends on");
  p("/// Slab features not yet shipped (`OutOfScope`).");
  p("#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]");
  p("pub enum Verdict {");
  p("    Auto,");
  p("    Human,");
  p("    OutOfScope,");
  p("}");
  p("");
  p("impl Verdict {");
  p("    pub const fn as_str(self) -> &'static str {");
  p("        match self {");
  p("            Verdict::Auto => \"auto\",");
  p("            Verdict::Human => \"human\",");
  p("            Verdict::OutOfScope => \"outOfScope\",");
  p("        }");
  p("    }");
  p("}");
  p("");
  p("#[derive(Debug, Clone, Copy, Serialize)]");
  p("pub struct FailureCondition {");
  p("    /// Hyphenated id, e.g. `\"01-007\"`.");
  p("    pub id: &'static str,");
  p("    /// One-line description, verbatim from the Matterhorn Protocol.");
  p("    pub title: &'static str,");
  p("    pub verdict: Verdict,");
  p("    pub section_id: &'static str,");
  p("}");
  p("");
  p("#[derive(Debug, Clone, Copy, Serialize)]");
  p("pub struct Section {");
  p("    pub id: &'static str,");
  p("    pub title: &'static str,");
  p("    pub iso_clause: &'static str,");
  p("    pub conditions: &'static [FailureCondition],");
  p("}");
  p("");
  p("#[derive(Debug, Clone, Copy, Serialize)]");
  p("pub struct Totals {");
  p("    pub sections: usize,");
  p("    pub failure_conditions_in_this_registry: usize,");
  p("    pub failure_conditions_in_full_protocol: usize,");
  p("    pub auto: usize,");
  p("    pub human: usize,");
  p("    pub out_of_scope: usize,");
  p("    pub not_yet_transcribed: usize,");
  p("}");
  p("");
  p("pub const TOTALS: Totals = Totals {");
  p(`    sections: ${registry.totals.sections},`);
  p(`    failure_conditions_in_this_registry: ${registry.totals.failureConditions_inThisRegistry},`);
  p(`    failure_conditions_in_full_protocol: ${registry.totals.failureConditions_inFullProtocol},`);
  p(`    auto: ${registry.totals.auto},`);
  p(`    human: ${registry.totals.human},`);
  p(`    out_of_scope: ${registry.totals.outOfScope_v3_1_0},`);
  p(`    not_yet_transcribed: ${registry.totals.notYetTranscribed},`);
  p("};");
  p("");
  p("pub const PROTOCOL_VERSION: &str = " + rustString(registry.version) + ";");
  p("pub const APPLIES_TO: &str = " + rustString(registry.appliesTo) + ";");
  p("");

  // Per-section condition arrays so SECTIONS can hold &'static [...].
  for (const sec of registry.sections) {
    p(`const SECTION_${sec.id}_CONDITIONS: &[FailureCondition] = &[`);
    for (const fc of sec.failureConditions) {
      p(
        `    FailureCondition { id: ${rustString(fc.id)}, title: ${rustString(
          fc.title,
        )}, verdict: Verdict::${VERDICT_VARIANT[fc.verdict]}, section_id: ${rustString(
          sec.id,
        )} },`,
      );
    }
    p("];");
    p("");
  }

  p(`pub const SECTIONS: &[Section] = &[`);
  for (const sec of registry.sections) {
    p(
      `    Section { id: ${rustString(sec.id)}, title: ${rustString(
        sec.title,
      )}, iso_clause: ${rustString(sec.isoClause)}, conditions: SECTION_${sec.id}_CONDITIONS },`,
    );
  }
  p("];");
  p("");

  // Flat CONDITIONS slice = concatenation of all per-section slices.
  p(`pub const CONDITIONS_COUNT: usize = ${counts.total};`);
  p("");
  p("/// Returns every failure condition in the registry, in registry order.");
  p("pub fn all_conditions() -> impl Iterator<Item = &'static FailureCondition> {");
  p("    SECTIONS.iter().flat_map(|s| s.conditions.iter())");
  p("}");
  p("");
  p("/// Look up a single failure condition by hyphenated id (e.g. \"01-007\").");
  p("pub fn find_condition(id: &str) -> Option<&'static FailureCondition> {");
  p("    all_conditions().find(|c| c.id == id)");
  p("}");
  p("");
  p("/// Look up a section by two-digit id (e.g. \"01\").");
  p("pub fn section_by_id(id: &str) -> Option<&'static Section> {");
  p("    SECTIONS.iter().find(|s| s.id == id)");
  p("}");
  p("");
  p("pub fn auto_conditions() -> impl Iterator<Item = &'static FailureCondition> {");
  p("    all_conditions().filter(|c| c.verdict == Verdict::Auto)");
  p("}");
  p("");
  p("pub fn human_conditions() -> impl Iterator<Item = &'static FailureCondition> {");
  p("    all_conditions().filter(|c| c.verdict == Verdict::Human)");
  p("}");
  p("");
  p("pub fn out_of_scope_conditions() -> impl Iterator<Item = &'static FailureCondition> {");
  p("    all_conditions().filter(|c| c.verdict == Verdict::OutOfScope)");
  p("}");
  p("");

  // Tests
  p("#[cfg(test)]");
  p("mod tests {");
  p("    use super::*;");
  p("");
  p("    #[test]");
  p("    fn totals_match_condition_counts() {");
  p("        let auto = auto_conditions().count();");
  p("        let human = human_conditions().count();");
  p("        let oos = out_of_scope_conditions().count();");
  p("        assert_eq!(auto, TOTALS.auto, \"auto count drift\");");
  p("        assert_eq!(human, TOTALS.human, \"human count drift\");");
  p("        assert_eq!(oos, TOTALS.out_of_scope, \"out_of_scope count drift\");");
  p("        assert_eq!(");
  p("            auto + human + oos,");
  p("            TOTALS.failure_conditions_in_this_registry,");
  p("            \"sum != registry total\",");
  p("        );");
  p("    }");
  p("");
  p("    #[test]");
  p("    fn every_section_has_at_least_one_condition() {");
  p("        for s in SECTIONS {");
  p("            assert!(!s.conditions.is_empty(), \"section {} empty\", s.id);");
  p("        }");
  p("    }");
  p("");
  p("    #[test]");
  p("    fn condition_ids_are_unique() {");
  p("        let mut ids: Vec<&str> = all_conditions().map(|c| c.id).collect();");
  p("        let total = ids.len();");
  p("        ids.sort_unstable();");
  p("        ids.dedup();");
  p("        assert_eq!(ids.len(), total, \"duplicate condition ids\");");
  p("    }");
  p("");
  p("    #[test]");
  p("    fn find_condition_round_trips() {");
  p("        for c in all_conditions() {");
  p("            let got = find_condition(c.id).expect(\"found\");");
  p("            assert_eq!(got.id, c.id);");
  p("            assert_eq!(got.section_id, c.section_id);");
  p("        }");
  p("    }");
  p("");
  p("    #[test]");
  p("    fn section_prefix_matches_condition_ids() {");
  p("        for s in SECTIONS {");
  p("            for c in s.conditions {");
  p("                assert_eq!(c.section_id, s.id);");
  p("                assert!(");
  p("                    c.id.starts_with(&format!(\"{}-\", s.id)),");
  p("                    \"condition {} not in section {}\",");
  p("                    c.id,");
  p("                    s.id,");
  p("                );");
  p("            }");
  p("        }");
  p("    }");
  p("");
  p("    #[test]");
  p("    fn coverage_at_least_two_thirds() {");
  p("        // Slice 0 transcribed 91 of 136 conditions = 66.9%. This test");
  p("        // guards against accidental deletions in future slices; it does");
  p("        // NOT prevent intentional growth.");
  p("        let ratio = TOTALS.failure_conditions_in_this_registry as f64");
  p("            / TOTALS.failure_conditions_in_full_protocol as f64;");
  p("        assert!(ratio > 0.66, \"registry coverage regressed: {ratio}\");");
  p("    }");
  p("}");
  p("");
  return lines.join("\n");
}

function formatRust(src) {
  // Pipe through `rustfmt` so codegen output matches `cargo fmt --check`.
  // If rustfmt is not on PATH (rare in CI), fall back to the raw output —
  // the in-repo cargo fmt check will then flag it as expected.
  const result = spawnSync("rustfmt", ["--edition", "2021", "--emit", "stdout"], {
    input: src,
    encoding: "utf8",
  });
  if (result.error || result.status !== 0) {
    if (result.error && result.error.code === "ENOENT") {
      console.warn(
        "[codegen-matterhorn] WARNING: rustfmt not found on PATH; emitting unformatted source.",
      );
      return src;
    }
    bail(
      `rustfmt failed (status ${result.status}): ${result.stderr || result.error}`,
    );
  }
  return result.stdout;
}

async function main() {
  let registry;
  try {
    const raw = await readFile(SRC_JSON, "utf8");
    registry = JSON.parse(raw);
  } catch (e) {
    bail(`cannot read ${SRC_JSON}: ${e.message}`);
  }

  // Schema presence check (we don't pull in ajv here — schema is human + IDE aid).
  try {
    await readFile(SRC_SCHEMA, "utf8");
  } catch {
    bail(`missing schema at ${SRC_SCHEMA}`);
  }

  const counts = validate(registry);
  const rustRaw = genRust(registry, counts);
  const rust = formatRust(rustRaw);

  if (checkMode()) {
    let existing = "";
    try {
      existing = await readFile(OUT_RS, "utf8");
    } catch {
      bail(`--check: ${OUT_RS} missing; run without --check to generate it.`);
    }
    if (existing !== rust) {
      const want = createHash("sha256").update(rust).digest("hex").slice(0, 12);
      const have = createHash("sha256").update(existing).digest("hex").slice(0, 12);
      bail(
        `--check: ${OUT_RS} is stale (have ${have}, want ${want}). ` +
          `Run \`pnpm loom:codegen\` and commit.`,
      );
    }
    console.log(
      `[codegen-matterhorn] --check OK — ${counts.total} conditions ` +
        `(${counts.countAuto} auto, ${counts.countHuman} human, ${counts.countOoS} out-of-scope)`,
    );
    return;
  }

  await writeFile(OUT_RS, rust, "utf8");
  console.log(
    `[codegen-matterhorn] wrote ${OUT_RS} — ${counts.total} conditions ` +
      `(${counts.countAuto} auto, ${counts.countHuman} human, ${counts.countOoS} out-of-scope) ` +
      `across ${registry.sections.length} sections.`,
  );
}

main().catch((e) => {
  console.error(e);
  exit(1);
});
