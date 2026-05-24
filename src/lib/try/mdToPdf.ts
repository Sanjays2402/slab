/**
 * mdToPdf.ts — pure-function Markdown → PDF in the browser.
 *
 * Zero new dependencies: we ship a minimal block+inline lexer and lay text
 * out with pdf-lib's `StandardFonts` (Helvetica + Helvetica-Bold +
 * Helvetica-Oblique + Courier). The supported subset is deliberately small
 * (YAGNI) — for the wedge demo, "GitHub README that paginates beautifully"
 * is enough. Everything fancier (tables, images, footnotes, math) routes
 * to the DownloadWall in /try/markdown.
 *
 * No DOM access. No Tauri imports. Identical results in Node + browser.
 */
import {
  PDFDocument,
  StandardFonts,
  rgb,
  type PDFFont,
  type PDFPage,
} from "pdf-lib";

export interface MdToPdfOptions {
  /** Page size. Defaults to `Letter`. */
  pageSize?: "A4" | "Letter" | "Legal";
  /** Margin in PDF points. Default 54 (0.75"). */
  margin?: number;
  /** Body font size in points. Default 11. */
  fontSize?: number;
  /** Line-height multiplier. Default 1.45. */
  lineHeight?: number;
  /** Optional title metadata. */
  title?: string;
  /** Optional author metadata. */
  author?: string;
}

const PAGE_SIZES: Record<NonNullable<MdToPdfOptions["pageSize"]>, [number, number]> = {
  A4: [595.28, 841.89],
  Letter: [612, 792],
  Legal: [612, 1008],
};

// ─────────────────────────────────────────────────────────────────────────────
// Lexer — block-level

export type Block =
  | { kind: "heading"; level: 1 | 2 | 3 | 4 | 5 | 6; text: string }
  | { kind: "paragraph"; text: string }
  | { kind: "ul"; items: string[] }
  | { kind: "ol"; items: string[] }
  | { kind: "blockquote"; text: string }
  | { kind: "code"; text: string; lang?: string }
  | { kind: "hr" }
  | { kind: "blank" };

const HEADING_RE = /^(#{1,6})\s+(.+)$/;
const UL_RE = /^[-*+]\s+(.+)$/;
const OL_RE = /^\d+\.\s+(.+)$/;
const BQ_RE = /^>\s?(.*)$/;
const HR_RE = /^---+\s*$/;
const FENCE_RE = /^```(\w*)\s*$/;

export function lexBlocks(markdown: string): Block[] {
  const lines = markdown.replace(/\r\n?/g, "\n").split("\n");
  const out: Block[] = [];
  let i = 0;
  while (i < lines.length) {
    const line = lines[i];

    // Fenced code block
    const fence = FENCE_RE.exec(line);
    if (fence) {
      const lang = fence[1] || undefined;
      const buf: string[] = [];
      i++;
      while (i < lines.length && !FENCE_RE.test(lines[i])) {
        buf.push(lines[i]);
        i++;
      }
      i++; // consume closing fence (if present)
      out.push({ kind: "code", text: buf.join("\n"), lang });
      continue;
    }

    if (HR_RE.test(line)) {
      out.push({ kind: "hr" });
      i++;
      continue;
    }

    if (line.trim() === "") {
      // Collapse runs of blanks to a single blank block.
      if (out.length === 0 || out[out.length - 1].kind !== "blank") {
        out.push({ kind: "blank" });
      }
      i++;
      continue;
    }

    const hMatch = HEADING_RE.exec(line);
    if (hMatch) {
      out.push({
        kind: "heading",
        level: hMatch[1].length as 1 | 2 | 3 | 4 | 5 | 6,
        text: hMatch[2].trim(),
      });
      i++;
      continue;
    }

    // Lists — gather consecutive matching lines
    if (UL_RE.test(line)) {
      const items: string[] = [];
      while (i < lines.length) {
        const m = UL_RE.exec(lines[i]);
        if (!m) break;
        items.push(m[1].trim());
        i++;
      }
      out.push({ kind: "ul", items });
      continue;
    }
    if (OL_RE.test(line)) {
      const items: string[] = [];
      while (i < lines.length) {
        const m = OL_RE.exec(lines[i]);
        if (!m) break;
        items.push(m[1].trim());
        i++;
      }
      out.push({ kind: "ol", items });
      continue;
    }

    // Blockquote — gather consecutive `> ` lines
    if (BQ_RE.test(line)) {
      const buf: string[] = [];
      while (i < lines.length) {
        const m = BQ_RE.exec(lines[i]);
        if (!m) break;
        buf.push(m[1]);
        i++;
      }
      out.push({ kind: "blockquote", text: buf.join(" ").trim() });
      continue;
    }

    // Paragraph — gather until blank, heading, list, fence, hr, or quote
    const buf: string[] = [line];
    i++;
    while (i < lines.length) {
      const n = lines[i];
      if (
        n.trim() === "" ||
        HEADING_RE.test(n) ||
        UL_RE.test(n) ||
        OL_RE.test(n) ||
        BQ_RE.test(n) ||
        HR_RE.test(n) ||
        FENCE_RE.test(n)
      ) {
        break;
      }
      buf.push(n);
      i++;
    }
    out.push({ kind: "paragraph", text: buf.join(" ").trim() });
  }
  return out.filter((b) => b.kind !== "blank" || true); // keep blanks; renderer uses them as spacing
}

// ─────────────────────────────────────────────────────────────────────────────
// Inline lexer — bold / italic / code

export type InlineSpan = {
  text: string;
  bold: boolean;
  italic: boolean;
  code: boolean;
};

/**
 * Parse inline markdown into runs of styled text. Supports **bold**,
 * *italic*, `code`. Nesting is *intentionally* shallow — we don't try to
 * handle `***bold-italic***` (rare in real READMEs; YAGNI).
 */
export function lexInline(s: string): InlineSpan[] {
  const out: InlineSpan[] = [];
  let i = 0;
  let buf = "";
  let bold = false;
  let italic = false;

  const flush = () => {
    if (buf.length > 0) {
      out.push({ text: buf, bold, italic, code: false });
      buf = "";
    }
  };

  while (i < s.length) {
    const c = s[i];
    const next = s[i + 1];

    // Inline code
    if (c === "`") {
      flush();
      let j = i + 1;
      while (j < s.length && s[j] !== "`") j++;
      const codeText = s.slice(i + 1, j);
      if (codeText.length > 0) {
        out.push({ text: codeText, bold: false, italic: false, code: true });
      }
      i = j + 1;
      continue;
    }

    // Bold **
    if (c === "*" && next === "*") {
      flush();
      bold = !bold;
      i += 2;
      continue;
    }

    // Italic *
    if (c === "*") {
      flush();
      italic = !italic;
      i += 1;
      continue;
    }

    // Escape
    if (c === "\\" && next) {
      buf += next;
      i += 2;
      continue;
    }

    buf += c;
    i++;
  }
  flush();
  return out;
}

// ─────────────────────────────────────────────────────────────────────────────
// Layout primitives

interface FontKit {
  regular: PDFFont;
  bold: PDFFont;
  italic: PDFFont;
  boldItalic: PDFFont;
  mono: PDFFont;
}

interface LayoutState {
  doc: PDFDocument;
  page: PDFPage;
  fonts: FontKit;
  pageWidth: number;
  pageHeight: number;
  margin: number;
  fontSize: number;
  lineHeight: number;
  /** y-cursor in PDF coordinates (0 = bottom). */
  y: number;
}

function newPage(st: LayoutState) {
  st.page = st.doc.addPage([st.pageWidth, st.pageHeight]);
  st.y = st.pageHeight - st.margin;
}

function ensureSpace(st: LayoutState, needed: number) {
  if (st.y - needed < st.margin) {
    newPage(st);
  }
}

function pickFont(fonts: FontKit, span: InlineSpan): PDFFont {
  if (span.code) return fonts.mono;
  if (span.bold && span.italic) return fonts.boldItalic;
  if (span.bold) return fonts.bold;
  if (span.italic) return fonts.italic;
  return fonts.regular;
}

/** Width of a list of spans rendered at the given size. */
function spansWidth(spans: InlineSpan[], fonts: FontKit, size: number): number {
  let w = 0;
  for (const sp of spans) {
    const f = pickFont(fonts, sp);
    w += f.widthOfTextAtSize(sp.text, size);
  }
  return w;
}

/**
 * Word-wrap a single inline-span sequence into multiple visual lines that fit
 * inside `maxWidth`. The wrapping unit is the word.
 */
function wrapSpans(
  spans: InlineSpan[],
  fonts: FontKit,
  size: number,
  maxWidth: number,
): InlineSpan[][] {
  // First, atomize into per-word spans so we can word-wrap while keeping styling.
  const atoms: InlineSpan[] = [];
  for (const sp of spans) {
    const words = sp.text.split(/(\s+)/); // keep whitespace
    for (const w of words) {
      if (w.length === 0) continue;
      atoms.push({ ...sp, text: w });
    }
  }

  const lines: InlineSpan[][] = [];
  let current: InlineSpan[] = [];
  let currentW = 0;

  for (const atom of atoms) {
    const f = pickFont(fonts, atom);
    const w = f.widthOfTextAtSize(atom.text, size);
    // Skip a leading whitespace-only atom at the start of a wrapped line.
    if (current.length === 0 && /^\s+$/.test(atom.text)) continue;
    if (currentW + w > maxWidth && current.length > 0) {
      lines.push(current);
      current = [];
      currentW = 0;
      if (/^\s+$/.test(atom.text)) continue;
    }
    current.push(atom);
    currentW += w;
  }
  if (current.length > 0) lines.push(current);
  return lines;
}

function drawSpansLine(
  st: LayoutState,
  spans: InlineSpan[],
  x: number,
  size: number,
) {
  let cursorX = x;
  for (const sp of spans) {
    const f = pickFont(st.fonts, sp);
    st.page.drawText(sp.text, {
      x: cursorX,
      y: st.y,
      size,
      font: f,
      color: sp.code ? rgb(0.6, 0.15, 0.35) : rgb(0.1, 0.1, 0.1),
    });
    cursorX += f.widthOfTextAtSize(sp.text, size);
  }
}

// ─────────────────────────────────────────────────────────────────────────────
// Block renderers

function renderHeading(st: LayoutState, level: number, text: string) {
  const sizes = [0, 26, 21, 17, 14, 12, 11];
  const size = sizes[level] ?? 11;
  const lineH = size * 1.25;
  ensureSpace(st, lineH + 6);
  st.y -= size; // baseline drop
  const spans = lexInline(text);
  // Use bold font for headings regardless of inline.
  const bolded: InlineSpan[] = spans.map((s) => ({ ...s, bold: true }));
  const maxW = st.pageWidth - 2 * st.margin;
  const lines = wrapSpans(bolded, st.fonts, size, maxW);
  for (let k = 0; k < lines.length; k++) {
    if (k > 0) {
      ensureSpace(st, lineH);
      st.y -= lineH;
    }
    drawSpansLine(st, lines[k], st.margin, size);
  }
  st.y -= 8;
}

function renderParagraph(st: LayoutState, text: string) {
  const spans = lexInline(text);
  const maxW = st.pageWidth - 2 * st.margin;
  const lineH = st.fontSize * st.lineHeight;
  const lines = wrapSpans(spans, st.fonts, st.fontSize, maxW);
  for (const line of lines) {
    ensureSpace(st, lineH);
    st.y -= st.fontSize;
    drawSpansLine(st, line, st.margin, st.fontSize);
    st.y -= lineH - st.fontSize;
  }
  st.y -= 6;
}

function renderList(st: LayoutState, items: string[], ordered: boolean) {
  const indent = 18;
  const lineH = st.fontSize * st.lineHeight;
  const maxW = st.pageWidth - 2 * st.margin - indent;
  for (let idx = 0; idx < items.length; idx++) {
    const bullet = ordered ? `${idx + 1}.` : "•";
    const spans = lexInline(items[idx]);
    const lines = wrapSpans(spans, st.fonts, st.fontSize, maxW);
    for (let k = 0; k < lines.length; k++) {
      ensureSpace(st, lineH);
      st.y -= st.fontSize;
      if (k === 0) {
        st.page.drawText(bullet, {
          x: st.margin,
          y: st.y,
          size: st.fontSize,
          font: st.fonts.regular,
          color: rgb(0.3, 0.3, 0.3),
        });
      }
      drawSpansLine(st, lines[k], st.margin + indent, st.fontSize);
      st.y -= lineH - st.fontSize;
    }
  }
  st.y -= 6;
}

function renderBlockquote(st: LayoutState, text: string) {
  const indent = 14;
  const spans = lexInline(text).map((s) => ({ ...s, italic: true }));
  const maxW = st.pageWidth - 2 * st.margin - indent;
  const lineH = st.fontSize * st.lineHeight;
  const lines = wrapSpans(spans, st.fonts, st.fontSize, maxW);
  for (const line of lines) {
    ensureSpace(st, lineH);
    st.y -= st.fontSize;
    // Quote bar
    st.page.drawRectangle({
      x: st.margin,
      y: st.y - 2,
      width: 3,
      height: st.fontSize + 4,
      color: rgb(0.7, 0.7, 0.75),
    });
    drawSpansLine(st, line, st.margin + indent, st.fontSize);
    st.y -= lineH - st.fontSize;
  }
  st.y -= 6;
}

function renderCode(st: LayoutState, text: string) {
  const lineH = st.fontSize * 1.35;
  const lines = text.split("\n");
  // Subtle background block.
  const blockH = lines.length * lineH + 10;
  ensureSpace(st, blockH);
  st.page.drawRectangle({
    x: st.margin - 4,
    y: st.y - blockH + 8,
    width: st.pageWidth - 2 * st.margin + 8,
    height: blockH,
    color: rgb(0.96, 0.96, 0.97),
  });
  for (const line of lines) {
    st.y -= st.fontSize;
    st.page.drawText(line, {
      x: st.margin,
      y: st.y,
      size: st.fontSize,
      font: st.fonts.mono,
      color: rgb(0.15, 0.15, 0.2),
    });
    st.y -= lineH - st.fontSize;
  }
  st.y -= 10;
}

function renderHr(st: LayoutState) {
  ensureSpace(st, 14);
  st.y -= 6;
  st.page.drawLine({
    start: { x: st.margin, y: st.y },
    end: { x: st.pageWidth - st.margin, y: st.y },
    thickness: 0.5,
    color: rgb(0.8, 0.8, 0.8),
  });
  st.y -= 10;
}

// ─────────────────────────────────────────────────────────────────────────────
// Public API

export async function mdToPdf(
  markdown: string,
  opts: MdToPdfOptions = {},
): Promise<Uint8Array> {
  const pageSize = opts.pageSize ?? "Letter";
  const [pageWidth, pageHeight] = PAGE_SIZES[pageSize];
  const margin = opts.margin ?? 54;
  const fontSize = opts.fontSize ?? 11;
  const lineHeight = opts.lineHeight ?? 1.45;

  const doc = await PDFDocument.create();
  if (opts.title) doc.setTitle(opts.title);
  if (opts.author) doc.setAuthor(opts.author);
  doc.setProducer("Slab (try.slab.app)");
  doc.setCreator("Slab Marquee");

  const fonts: FontKit = {
    regular: await doc.embedFont(StandardFonts.Helvetica),
    bold: await doc.embedFont(StandardFonts.HelveticaBold),
    italic: await doc.embedFont(StandardFonts.HelveticaOblique),
    boldItalic: await doc.embedFont(StandardFonts.HelveticaBoldOblique),
    mono: await doc.embedFont(StandardFonts.Courier),
  };

  const page = doc.addPage([pageWidth, pageHeight]);
  const st: LayoutState = {
    doc,
    page,
    fonts,
    pageWidth,
    pageHeight,
    margin,
    fontSize,
    lineHeight,
    y: pageHeight - margin,
  };

  const blocks = lexBlocks(markdown);
  for (const b of blocks) {
    switch (b.kind) {
      case "heading":
        renderHeading(st, b.level, b.text);
        break;
      case "paragraph":
        renderParagraph(st, b.text);
        break;
      case "ul":
        renderList(st, b.items, false);
        break;
      case "ol":
        renderList(st, b.items, true);
        break;
      case "blockquote":
        renderBlockquote(st, b.text);
        break;
      case "code":
        renderCode(st, b.text);
        break;
      case "hr":
        renderHr(st);
        break;
      case "blank":
        st.y -= fontSize * 0.5;
        break;
    }
  }

  return await doc.save();
}
