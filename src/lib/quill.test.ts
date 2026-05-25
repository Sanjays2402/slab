// Smoke tests for the v3.28.0 "Quill Hub" shared store.
//
// Style matches `marketplace/fuzzy.test.ts` — no test runner dep, just
// a tiny inline `expect` so future hands can read the contract at a
// glance. Run with:
//   node --import tsx src/lib/quill.test.ts
// (or copy assertions into the browser console after importing $lib/quill).
//
// The store's job is small but load-bearing: it's the single source of
// truth that lets four separate Acrobat-killer features (auto-detect,
// designer, fill, batch CSV merge) feel like one product surface.

import {
  quill,
  setInput,
  clearInput,
  recordDetection,
  recordFormsReport,
  resetQuill,
  snapshot,
  setActiveTab,
} from "./quill";
import { get } from "svelte/store";

function expect(cond: boolean, label: string): void {
  if (!cond) {
    // eslint-disable-next-line no-console
    console.error("FAIL:", label);
    if (typeof process !== "undefined") process.exitCode = 1;
  } else {
    // eslint-disable-next-line no-console
    console.log("ok  ", label);
  }
}

function reset() {
  resetQuill();
}

// --- starts empty -----------------------------------------------------------
reset();
{
  const s = get(quill);
  expect(s.input === null, "starts with no input");
  expect(s.formsReport === null, "starts with no forms report");
  expect(s.detection === null, "starts with no detection");
  expect(s.activeTab === "detect", "starts on detect tab");
  expect(s.suggestedNextTab === "detect", "starts suggesting detect");
}

// --- setInput clears stale reports -----------------------------------------
reset();
{
  recordFormsReport({
    has_acroform: true,
    need_appearances: false,
    has_xfa: false,
    fields: [],
  });
  setInput("/tmp/foo.pdf");
  const s = get(quill);
  expect(s.input === "/tmp/foo.pdf", "setInput stores path");
  expect(s.formsReport === null, "setInput clears stale forms report");
  expect(s.detection === null, "setInput clears stale detection");
}

// --- suggestion: design after detection ------------------------------------
reset();
{
  setInput("/tmp/foo.pdf");
  recordDetection({
    candidates: [{ suggested_name: "x" }],
    pages_scanned: 1,
    already_has_acroform: false,
    warnings: [],
  });
  expect(get(quill).suggestedNextTab === "design", "candidates → design");
}

// --- suggestion: fill once AcroForm exists ---------------------------------
reset();
{
  setInput("/tmp/foo.pdf");
  recordFormsReport({
    has_acroform: true,
    need_appearances: true,
    has_xfa: false,
    fields: [{ name: "a" }],
  });
  expect(get(quill).suggestedNextTab === "fill", "acroform → fill");
}

// --- suggestion: batch once a value is set ---------------------------------
reset();
{
  setInput("/tmp/foo.pdf");
  recordFormsReport({
    has_acroform: true,
    need_appearances: true,
    has_xfa: false,
    fields: [{ name: "a", value: "Alice" }],
  });
  expect(get(quill).suggestedNextTab === "batch", "values set → batch");
}

// --- setActiveTab keeps file context ---------------------------------------
reset();
{
  setInput("/tmp/foo.pdf");
  setActiveTab("batch");
  const s = get(quill);
  expect(s.activeTab === "batch", "setActiveTab updates activeTab");
  expect(s.input === "/tmp/foo.pdf", "setActiveTab preserves input");
}

// --- clearInput clears file but keeps tab ----------------------------------
reset();
{
  setInput("/tmp/foo.pdf");
  setActiveTab("batch");
  clearInput();
  const s = get(quill);
  expect(s.input === null, "clearInput drops input");
  expect(s.activeTab === "batch", "clearInput preserves activeTab");
}

// --- snapshot ---------------------------------------------------------------
reset();
{
  setInput("/tmp/foo.pdf");
  expect(snapshot().input === "/tmp/foo.pdf", "snapshot reads state");
}

// --- resetQuill -------------------------------------------------------------
reset();
{
  setInput("/tmp/foo.pdf");
  setActiveTab("fill");
  resetQuill();
  const s = get(quill);
  expect(s.input === null, "resetQuill clears input");
  expect(s.activeTab === "detect", "resetQuill returns to detect");
}
