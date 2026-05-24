# Slab v3.19.0 — Marquee

**Slab now lives in your browser too.**
`try.slab.app` opens, drops a PDF in, and you're editing it offline before
the Acrobat installer would have finished downloading.

Same SvelteKit SPA that ships inside the desktop app. Same pdf-lib +
pdfjs-dist pipeline. Same zero-upload promise. Now reachable from any
laptop, library computer, or "I lost my install disk" emergency — no
account, no install, no ads, no upload.

## ✨ What's new

- **`try.slab.app` — the browser playground.** Drop a PDF onto the page,
  rotate, remove, reorder, and re-download. No upload. No account. Bytes
  never leave the tab.
- **Live Markdown → PDF editor** at `/try/markdown`. Split-pane editor on
  the left, live PDF preview on the right, **Cmd/Ctrl + S** to download.
  Headings, lists, blockquotes, inline `code`, fenced code blocks,
  rules — all rendered to a clean A4/Letter/Legal PDF in the browser,
  zero dependencies beyond what Slab already ships.
- **`<DownloadWall>` everywhere.** Every advanced action (OCR, batch ops,
  bulk redaction, EPUB, DOCX, custom fonts, math, table extraction…)
  shows a tasteful "this needs the desktop app" CTA with a one-click
  download link. The browser is the demo; the app is the product.
- **Privacy banner** on every playground route — `"0 bytes uploaded —
  everything happens in this tab"`. Click-through opens a plain-English
  explainer of why local-first matters.
- **Page-ops route** at `/try/pages` — visual thumbnail grid, drag to
  reorder, click to rotate, X to remove, hit Download for a fresh PDF.
  The fast common case, instantly.
- **Metadata editor** at `/try/metadata` — title, author, subject,
  keywords, edit in place, download. The thing you Google "online PDF
  metadata editor" for, but trustable.
- **Deploy pipeline** — fully automated CI/CD to Cloudflare Pages. Every
  push to `main` that touches the front-end rebuilds and redeploys
  `try.slab.app` in under 2 minutes.

## 🎯 Why this matters

Slab is the privacy-first, free, cross-platform PDF workstation. But
"download a desktop app" is a hill — and Adobe's $239/yr subscription
has trained users to type "edit PDF online" into Google instead.

`try.slab.app` flips that funnel. Anyone with a browser can do the
common PDF chores instantly — reorder pages, fix metadata, write a
markdown-based one-pager — and discover that for the hard stuff
(batch, OCR, redaction, EPUB) there's a real desktop app waiting.

The browser is the funnel. The desktop is the funnel exit. The privacy
guarantee is the same on both ends.

## 🏗️ Quality

- **0 new dependencies** for the Markdown→PDF pipeline — a custom
  lexer + pdf-lib StandardFonts, ~440 LOC, fully tested with a
  17-assertion node smoke test.
- All existing tests still green: **1,588 Rust tests + 17 pdfops + 17
  mdToPdf** scripts.
- SvelteKit static adapter output, no SSR, no Node.js runtime needed —
  hosts cleanly on any static CDN.
- `_redirects` + `_headers` baked into CI build for Cloudflare Pages
  (immutable cache for hashed assets, security headers for `/try/**`).

## 🔐 The wedge, in two domains

- **Desktop:** free, offline, no telemetry, no upload. Same on macOS,
  Windows, Linux.
- **Web:** runs entirely in your browser. No server. No analytics.
  No third-party scripts. We can't see your PDFs even if we wanted to.

Adobe charges $239/year and ships your file to their cloud.
We charge $0 and never touch it.

---

Try it: **https://try.slab.app**
Download the full desktop app: **https://github.com/Sanjays2402/slab/releases**
