// v3.29.0 "Forms Tour" — guided onboarding for the unified Forms Hub.
//
// Why this exists: v3.28.0 collapsed four scattered Acrobat-killer features
// (Detect · Design · Fill · Batch) into one tabbed workspace. The single
// "Forms" sidebar entry is great for power users but baffling for someone
// who opens Slab for the first time: which tab solves their problem?
//
// This module is the persistence + state layer for a 5-step coachmark tour
// that auto-fires on first visit to the Hub and explains, in 30 seconds,
// what each tab does and which one to start on. Tour completion (or skip)
// is remembered in localStorage so it never re-fires for the same user.
//
// The store also exposes `replayTour()` for the command palette + a
// `Cmd+Shift+/` shortcut, so the user can re-watch the tour any time.

import { writable, get, derived } from "svelte/store";

const STORAGE_KEY = "slab.quill.tour.completed.v1";

/** A single coachmark step in the Forms onboarding tour. */
export interface TourStep {
  /** CSS selector or `data-testid` value to anchor the spotlight to. */
  anchor: string;
  /** Position of the tooltip relative to the anchor's bounding box. */
  placement: "top" | "bottom" | "left" | "right" | "center";
  /** Big bold headline shown at the top of the coachmark. */
  title: string;
  /** Body copy — keep ≤ 220 chars so it fits the bubble at all sizes. */
  body: string;
  /** Optional emoji glyph rendered in the top-left of the bubble. */
  glyph?: string;
}

export const TOUR_STEPS: ReadonlyArray<TourStep> = [
  {
    anchor: "[data-testid='quill-hub']",
    placement: "center",
    glyph: "🍰",
    title: "Welcome to Forms",
    body: "Four Acrobat-killer features, one workspace. This 30-second tour shows you which tab to start on for any PDF form workflow. Press → to begin or Esc to skip.",
  },
  {
    anchor: "[data-testid='quill-tab-detect']",
    placement: "bottom",
    glyph: "✨",
    title: "Detect — start here for flat PDFs",
    body: "Got a scanned or printed PDF with empty boxes? Detect proposes fillable AcroForm fields automatically. The fastest path from PDF → fillable form.",
  },
  {
    anchor: "[data-testid='quill-tab-design']",
    placement: "bottom",
    glyph: "✎",
    title: "Design — author fields by hand",
    body: "Draw text fields, checkboxes, signatures yourself. Pixel-precise. Great for templates you'll reuse, or when Detect needs a manual touch-up.",
  },
  {
    anchor: "[data-testid='quill-tab-fill']",
    placement: "bottom",
    glyph: "📝",
    title: "Fill — type values into a form",
    body: "Got a PDF that's already a form? Inspect every field, type values, and flatten the result. Works fully offline, unlike Acrobat's cloud-tied filler.",
  },
  {
    anchor: "[data-testid='quill-tab-batch']",
    placement: "bottom",
    glyph: "⋮",
    title: "Batch — merge a CSV across many copies",
    body: "The headline trick. Drop a CSV, pick a template, get one filled PDF per row. 200 lease agreements in 200 seconds. This is the feature Adobe charges $239/yr for.",
  },
];

export interface TourState {
  /** Currently visible step index, or `null` when the tour is dismissed. */
  step: number | null;
  /** Has the user completed (or explicitly skipped) the tour before? */
  completed: boolean;
}

function loadCompleted(): boolean {
  if (typeof window === "undefined") return true; // SSR: treat as completed (no tour).
  try {
    return window.localStorage.getItem(STORAGE_KEY) === "1";
  } catch {
    // Private mode / sandboxed: be silent and skip the tour rather than crash.
    return true;
  }
}

function persistCompleted(): void {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(STORAGE_KEY, "1");
  } catch {
    /* localStorage unavailable — best effort only */
  }
}

const _tour = writable<TourState>({ step: null, completed: loadCompleted() });

/** Read-only store the UI subscribes to. Derived so external code can't write. */
export const tour = derived(_tour, ($t) => $t);

/** Open the tour from the beginning. Use after the Hub has mounted. */
export function startTour(): void {
  _tour.set({ step: 0, completed: get(_tour).completed });
}

/** Advance to the next step, or finish if we're already on the last one. */
export function nextStep(): void {
  const s = get(_tour);
  if (s.step === null) return;
  if (s.step >= TOUR_STEPS.length - 1) {
    finishTour();
    return;
  }
  _tour.set({ step: s.step + 1, completed: s.completed });
}

/** Go back one step, clamped at the first step. */
export function prevStep(): void {
  const s = get(_tour);
  if (s.step === null || s.step <= 0) return;
  _tour.set({ step: s.step - 1, completed: s.completed });
}

/** Jump directly to a specific step. Useful for pagination dots. */
export function gotoStep(i: number): void {
  if (i < 0 || i >= TOUR_STEPS.length) return;
  _tour.set({ step: i, completed: get(_tour).completed });
}

/**
 * Skip the tour. Marks it completed so it doesn't auto-fire again, but the
 * user can still manually replay it via the palette or shortcut.
 */
export function skipTour(): void {
  persistCompleted();
  _tour.set({ step: null, completed: true });
}

/** End-of-tour: same persistence as skip, semantically different intent. */
export function finishTour(): void {
  persistCompleted();
  _tour.set({ step: null, completed: true });
}

/**
 * Re-open the tour even if the user has seen it before. Wired to the
 * command palette ("Forms: Show welcome tour") and the `Cmd+Shift+/`
 * shortcut. Does NOT reset the `completed` flag — replaying is allowed.
 */
export function replayTour(): void {
  _tour.set({ step: 0, completed: get(_tour).completed });
}

/**
 * Should the Hub auto-fire the tour on mount? True only the very first
 * time a user opens the Forms workspace. Safe to call on every mount.
 */
export function shouldAutoStart(): boolean {
  return !get(_tour).completed;
}

/**
 * Test seam: wipe both in-memory state and the localStorage flag. Only
 * used by quill-tour.test.ts; production code should never call this.
 */
export function _resetForTests(): void {
  if (typeof window !== "undefined") {
    try {
      window.localStorage.removeItem(STORAGE_KEY);
    } catch {
      /* ignore */
    }
  }
  _tour.set({ step: null, completed: false });
}

/** Test seam: read the raw state for assertions. */
export function _snapshot(): TourState {
  return get(_tour);
}
