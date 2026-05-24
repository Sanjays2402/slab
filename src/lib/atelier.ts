// Atelier client — v3.12.0
//
// Typed bridge to the Rust Atelier engine in `src-tauri/src/pdf/atelier/`.
// Recipes are ordered lists of steps; the runner pipes each PDF in an input
// folder through every step in sequence and writes the result to an output
// folder. Progress events stream over a Tauri `Channel<BatchProgress>` so the
// UI can render a live per-file × per-step matrix.
//
// Wire format mirrors serde's kebab-case tagged enums:
//   { kind: "ocr", language: "eng" }
//   { kind: "auto-redact", patterns: [...], presets: [...] }
//   { event: "step-progress", file_index, inner: { event: "completed", ... } }
//
// See `src-tauri/src/pdf/atelier/{recipe,batch,run}.rs` for canonical shapes.

import { invoke, Channel } from "@tauri-apps/api/core";

export type StepKind =
  | "ocr"
  | "auto-redact"
  | "bates"
  | "watermark"
  | "flatten"
  | "compactor"
  | "linearize"
  | "convert-to-docx"
  | "convert-to-xlsx"
  | "convert-to-pptx";

export type Step =
  | { kind: "ocr"; language: string }
  | { kind: "auto-redact"; patterns: string[]; presets: string[] }
  | { kind: "bates"; prefix: string; start: number; digits: number }
  | { kind: "watermark"; text: string; opacity: number }
  | { kind: "flatten"; dpi: number }
  | { kind: "compactor" }
  | { kind: "linearize" }
  | {
      kind: "convert-to-docx";
      detect_tables: boolean;
      detect_lists: boolean;
      heading_size_ratio: number;
    }
  | {
      kind: "convert-to-xlsx";
      type_numbers: boolean;
      type_dates: boolean;
      include_non_table_text: boolean;
    }
  | {
      kind: "convert-to-pptx";
      include_speaker_notes: boolean;
      detect_titles: boolean;
    };

export interface Recipe {
  name: string;
  version: number;
  steps: Step[];
}

export type StepProgress =
  | { event: "started"; step_index: number; total_steps: number; kind: string }
  | { event: "completed"; step_index: number; kind: string }
  | { event: "failed"; step_index: number; kind: string; error: string };

export type BatchProgress =
  | { event: "file-started"; file_index: number; path: string }
  | { event: "step-progress"; file_index: number; inner: StepProgress }
  | { event: "file-completed"; file_index: number; path: string }
  | { event: "file-failed"; file_index: number; path: string; error: string };

export interface BatchReport {
  total: number;
  succeeded: number;
  failed: number;
  failures: [string, string][];
}

/** Construct a default Step record for a given kind — used by the builder palette. */
export function defaultStep(kind: StepKind): Step {
  switch (kind) {
    case "ocr":
      return { kind: "ocr", language: "eng" };
    case "auto-redact":
      return { kind: "auto-redact", patterns: [], presets: ["ssn", "email", "phone"] };
    case "bates":
      return { kind: "bates", prefix: "ACME", start: 1, digits: 6 };
    case "watermark":
      return { kind: "watermark", text: "DRAFT", opacity: 0.25 };
    case "flatten":
      return { kind: "flatten", dpi: 150 };
    case "compactor":
      return { kind: "compactor" };
    case "linearize":
      return { kind: "linearize" };
    case "convert-to-docx":
      return {
        kind: "convert-to-docx",
        detect_tables: true,
        detect_lists: true,
        heading_size_ratio: 1.25,
      };
    case "convert-to-xlsx":
      return {
        kind: "convert-to-xlsx",
        type_numbers: true,
        type_dates: true,
        include_non_table_text: false,
      };
    case "convert-to-pptx":
      return {
        kind: "convert-to-pptx",
        include_speaker_notes: true,
        detect_titles: true,
      };
  }
}

/** Human-readable label for a Step (shown in the recipe builder + matrix header). */
export function stepLabel(s: Step): string {
  switch (s.kind) {
    case "ocr":
      return `OCR (${s.language})`;
    case "auto-redact": {
      const all = [...s.presets, ...s.patterns];
      return all.length ? `Redact: ${all.join(", ")}` : "Auto-redact";
    }
    case "bates":
      return `Bates ${s.prefix}-${String(s.start).padStart(s.digits, "0")}`;
    case "watermark":
      return `Watermark "${s.text}" @${Math.round(s.opacity * 100)}%`;
    case "flatten":
      return `Flatten ${s.dpi}dpi`;
    case "compactor":
      return "Compact";
    case "linearize":
      return "Fast Web View";
    case "convert-to-docx":
      return "Convert to Word (.docx)";
    case "convert-to-xlsx":
      return "Convert to Excel (.xlsx)";
    case "convert-to-pptx":
      return "Convert to PowerPoint (.pptx)";
  }
}

/** Compact two-letter glyph for the matrix header. */
export function stepGlyph(s: Step): string {
  switch (s.kind) {
    case "ocr":
      return "◉";
    case "auto-redact":
      return "⊘";
    case "bates":
      return "№";
    case "watermark":
      return "○";
    case "flatten":
      return "▤";
    case "compactor":
      return "▣";
    case "linearize":
      return "⚡";
    case "convert-to-docx":
      return "📝";
    case "convert-to-xlsx":
      return "📊";
    case "convert-to-pptx":
      return "🎞";
  }
}

export async function listRecipes(): Promise<Recipe[]> {
  return invoke<Recipe[]>("atelier_load_recipes");
}

export async function saveRecipe(recipe: Recipe): Promise<string> {
  return invoke<string>("atelier_save_recipe", { recipe });
}

export async function deleteRecipe(name: string): Promise<void> {
  await invoke("atelier_delete_recipe", { name });
}

/**
 * Run `recipe` over every `.pdf` in `inDir`, writing to `outDir`. The
 * `onEvent` callback fires for every progress event from any worker thread.
 */
export async function runBatch(
  inDir: string,
  outDir: string,
  recipe: Recipe,
  onEvent: (e: BatchProgress) => void,
): Promise<BatchReport> {
  const ch = new Channel<BatchProgress>();
  ch.onmessage = onEvent;
  return invoke<BatchReport>("atelier_run_batch", {
    inDir,
    outDir,
    recipe,
    onEvent: ch,
  });
}
