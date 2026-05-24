/**
 * Sample PDF registry for the /try playground.
 *
 * Pure module — no Svelte / Tauri imports. Safe to import from anywhere.
 */

export interface Sample {
  /** Stable URL slug (kebab-case, ascii). */
  slug: string;
  /** Human label shown on the sample card. */
  label: string;
  /** Public URL the sample is served at (under `/try/samples/`). */
  path: string;
  /** Page count of the sample (rendered on the card before download). */
  pages: number;
  /** Short copy under the label. */
  description: string;
  /** Tag list used for filtering / search. */
  tags: string[];
}

export const SAMPLES: Sample[] = [
  {
    slug: "employment-offer",
    label: "Employment offer letter",
    path: "/try/samples/employment-offer.pdf",
    pages: 2,
    description: "Try filling, signing, or redacting a legal doc.",
    tags: ["legal", "sign", "redact"],
  },
  {
    slug: "scanned-invoice",
    label: "Scanned invoice (image-only)",
    path: "/try/samples/scanned-invoice.pdf",
    pages: 1,
    description: "See what OCR pulls out (desktop only).",
    tags: ["scan", "ocr", "finance"],
  },
  {
    slug: "multi-chapter-report",
    label: "Multi-chapter quarterly report",
    path: "/try/samples/multi-chapter-report.pdf",
    pages: 24,
    description: "Split by chapter, reorder, or extract pages.",
    tags: ["report", "split", "pages"],
  },
];

/**
 * Loads a sample by slug.
 * Throws if the slug is unknown or the asset is unreachable.
 */
export async function loadSample(slug: string): Promise<Uint8Array> {
  const sample = SAMPLES.find((s) => s.slug === slug);
  if (!sample) {
    throw new Error(`unknown sample: ${slug}`);
  }
  const res = await fetch(sample.path);
  if (!res.ok) {
    throw new Error(`failed to load ${sample.path}: HTTP ${res.status}`);
  }
  return new Uint8Array(await res.arrayBuffer());
}

/** Returns true if the slug is in the registry. */
export function isKnownSample(slug: string): boolean {
  return SAMPLES.some((s) => s.slug === slug);
}
