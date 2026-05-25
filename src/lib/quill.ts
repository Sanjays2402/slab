// v3.28.0 "Quill Hub" — shared state across the four sub-tabs
// (Detect · Design · Fill · Batch). All four panels read/write this
// store so users never re-pick a file mid-flow, and the bottom
// "Next: …" call-to-action can stay one step ahead of them.
//
// This is the consolidation layer that turns four separate Acrobat-killer
// features (auto-detect, designer, fill, batch CSV merge) into one unified
// "Forms" workspace. Without this store, each panel re-opens its own
// file-picker and the user has to remember which feature lives where.

import { writable, derived, get } from "svelte/store";

export type QuillTab = "detect" | "design" | "fill" | "batch";

export type FormsReport = {
  has_acroform: boolean;
  need_appearances: boolean;
  has_xfa: boolean;
  fields: Array<{ name: string; value?: string | null; type?: string }>;
};

export type DetectionReport = {
  candidates: Array<{ suggested_name: string; [k: string]: unknown }>;
  pages_scanned: number;
  already_has_acroform: boolean;
  warnings: string[];
};

export type QuillState = {
  input: string | null;
  formsReport: FormsReport | null;
  detection: DetectionReport | null;
  activeTab: QuillTab;
};

const initial: QuillState = {
  input: null,
  formsReport: null,
  detection: null,
  activeTab: "detect",
};

const _quill = writable<QuillState>({ ...initial });

/**
 * Public read-only store. The derived form attaches `suggestedNextTab`
 * so the hub footer always knows the smart next step for the user.
 */
export const quill = derived(_quill, ($s) => ({
  ...$s,
  suggestedNextTab: suggestNext($s),
}));

function suggestNext(s: QuillState): QuillTab {
  if (!s.input) return "detect";
  // A field with a non-null/non-empty value = user has started filling,
  // so the natural next step is to batch the work across many copies.
  if (s.formsReport?.fields?.some((f) => f.value)) return "batch";
  if (s.formsReport?.has_acroform) return "fill";
  if (s.detection && s.detection.candidates.length > 0) return "design";
  return "detect";
}

export function setInput(path: string) {
  _quill.update((s) => ({
    ...s,
    input: path,
    formsReport: null,
    detection: null,
  }));
}

export function clearInput() {
  _quill.update((s) => ({
    ...s,
    input: null,
    formsReport: null,
    detection: null,
  }));
}

export function recordDetection(d: DetectionReport) {
  _quill.update((s) => ({ ...s, detection: d }));
}

export function recordFormsReport(r: FormsReport) {
  _quill.update((s) => ({ ...s, formsReport: r }));
}

export function setActiveTab(t: QuillTab) {
  _quill.update((s) => ({ ...s, activeTab: t }));
}

export function resetQuill() {
  _quill.set({ ...initial });
}

export function snapshot(): QuillState {
  return get(_quill);
}
