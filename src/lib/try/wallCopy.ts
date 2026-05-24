/**
 * Per-feature copy for `<DownloadWall>`.
 *
 * Each entry maps a Slab feature key to the headline, body, and
 * proof-of-life line shown when the user tries a desktop-only action
 * from the browser.  Copy lives here (not in the component) so it's
 * trivially extensible — each new gated feature is a few extra lines.
 */

export interface WallCopy {
  /** Single-sentence headline. */
  headline: string;
  /** Two-line value prop. */
  body: string;
  /** Tiny "why not in browser?" footer. */
  whyNot: string;
}

export const WALL_COPY: Record<string, WallCopy> = {
  ocr: {
    headline: "OCR runs offline in Slab.",
    body:
      "Drop a folder of scanned PDFs, get searchable text in seconds — " +
      "100 languages, never leaves your machine.",
    whyNot:
      "Browsers can't ship a 1GB OCR model lazily; the desktop app does it once and you own it.",
  },
  sign: {
    headline: "Sign PDFs with a real PKCS#7 signature.",
    body:
      "Slab signs in-place with your local certificate. " +
      "Adobe-validated. No cloud round-trip, no monthly fee.",
    whyNot:
      "The browser can't access your system keychain to sign with your real identity.",
  },
  beacon: {
    headline: "Chat with your PDF — offline.",
    body:
      "Beacon AI runs on Ollama locally. Summarize a 300-page report, " +
      "find citations, redact PII — your documents never leave your laptop.",
    whyNot:
      "Beacon needs a multi-GB model running locally. We won't ship that to a browser tab.",
  },
  redact: {
    headline: "True PII redaction — pixels gone, not just hidden.",
    body:
      "Adobe's 'redact' leaves text in the stream. Slab destroys the " +
      "underlying glyphs and image regions. Auditable, legal-safe.",
    whyNot:
      "Real redaction needs the full rendering pipeline. Desktop only.",
  },
  bates: {
    headline: "Bates numbering for discovery, in one drag.",
    body:
      "Stamp 10,000 pages with prefix-aware Bates IDs in under a minute. " +
      "Customizable position, font, padding, starting index.",
    whyNot:
      "Batch jobs on tens of thousands of pages aren't a browser-tab job. Get the app.",
  },
  compress: {
    headline: "Compress a PDF without re-encoding text.",
    body:
      "Slab inspects each image, recompresses only when it helps, and " +
      "keeps text crisp. Typical savings: 40–70% with zero quality loss.",
    whyNot:
      "JPEG/JBIG2 recompression in the browser is slow and limited. Desktop wins.",
  },
  diff: {
    headline: "Compare two PDFs side-by-side.",
    body:
      "Highlight added/removed/changed text and images across revisions. " +
      "Export a redline for review.",
    whyNot:
      "Layout diffing on large docs needs background workers and disk. Desktop only.",
  },
  press: {
    headline: "Pre-flight to PDF/X for print.",
    body:
      "Slab Press validates ICC profiles, ink coverage, font embedding, " +
      "and outputs PDF/X-4-compliant files ready for your printer.",
    whyNot:
      "PDF/X validation needs the full Slab rendering pipeline.",
  },
  bind: {
    headline: "Convert PDF → EPUB locally.",
    body:
      "Slab Bind produces valid EPUB 3 — load it on a Kindle, Kobo, or " +
      "Apple Books. 13 languages of hyphenation included.",
    whyNot:
      "The conversion writes a real EPUB ZIP to disk; the browser can do it but the desktop app does it better, and you wanted to try it anyway. (Browser version coming in Marquee-II.)",
  },
  markdown: {
    headline: "PDF → Markdown, locally.",
    body:
      "Slab Markdown converts a PDF to clean Markdown — heading structure, " +
      "tables, lists, footnotes. Paste into your notes app.",
    whyNot:
      "Heuristic-heavy and CPU-bound. Faster in the desktop app.",
  },
  "md-extras": {
    headline: "Embedded images, custom fonts, footnotes, math.",
    body:
      "The /try playground converts Markdown to PDF with the StandardFonts " +
      "Helvetica family — perfect for READMEs and notes. The desktop app " +
      "additionally embeds images, ships full Unicode font subsetting, " +
      "renders footnotes & sidenotes, and lays out math via KaTeX.",
    whyNot:
      "Font subsetting and image embedding need disk + larger runtime than " +
      "the browser tab budget should carry. The desktop app makes it instant.",
  },
  slide: {
    headline: "PDF → PowerPoint.",
    body:
      "One slide per page, editable text. Drop in a deck and keep going.",
    whyNot:
      "PPTX is a ZIP of XML; we generate it natively in the desktop app.",
  },
  default: {
    headline: "This action lives in the desktop app.",
    body:
      "Slab on macOS, Windows, and Linux — same UI as /try, with the full " +
      "feature set. Free, MIT-licensed, offline.",
    whyNot: "Some features need disk + system access the browser doesn't expose.",
  },
};

export type WallFeature = keyof typeof WALL_COPY;

/** Returns the wall copy for the given feature, falling back to `default`. */
export function getWallCopy(feature: string): WallCopy {
  return WALL_COPY[feature as WallFeature] ?? WALL_COPY.default;
}
