// Slides / Presenter Mode (v0.15.0 Theater) — IPC bindings.
//
// Wraps `slab_slides_analyze` so SlidesPanel doesn't need to hand-code
// `invoke()` or redefine the SlideReport DTO. Mirrors `pdf::slides`.

import { invoke } from "@tauri-apps/api/core";
import type { CmdResult } from "./types";

/** Per-page geometry + speaker notes. Mirrors `pdf::slides::SlidePage`. */
export interface SlidePage {
  page: number;
  width_pt: number;
  height_pt: number;
  /** width / height, 4 decimal places. */
  aspect: number;
  orientation: "landscape" | "portrait" | "square";
  /** Concatenated `/Annots` `/Subtype /Text` `/Contents` for the page.
   * Empty when the page has no speaker-note annotations. */
  notes: string;
}

/** Document-level slide report. Mirrors `pdf::slides::SlideReport`. */
export interface SlideReport {
  page_count: number;
  pages: SlidePage[];
  /** Dominant page size as `"WIDTHxHEIGHT"` in PDF points. */
  dominant_size: string;
  /** Friendly label, e.g. `"PowerPoint 16:9 (13.3×7.5 in)"`. */
  dominant_label: string;
  /** 0..1 fraction of pages matching `dominant_size` within ±2pt. */
  consistency: number;
  /** 0..1 fraction of pages with landscape orientation. */
  landscape_fraction: number;
  pages_with_notes: number;
  producer: string | null;
  producer_hint: boolean;
  /** 0..100 heuristic score. ≥65 → auto-classify as slides. */
  confidence: number;
  is_slides: boolean;
}

function unwrap<T>(res: CmdResult<T>): T {
  if (res.kind === "ok") return res.value;
  throw new Error(res.message);
}

/** Run `pdf::slides::analyze` on a PDF and return its SlideReport. */
export async function analyzeSlides(input: string): Promise<SlideReport> {
  const res = await invoke<CmdResult<SlideReport>>("slab_slides_analyze", {
    input,
  });
  return unwrap(res);
}
