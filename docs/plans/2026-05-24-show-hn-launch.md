# Show HN Launch — Implementation Plan

> **For Hermes:** Use subagent-driven-development skill to implement this plan task-by-task. This is a **launch-prep** plan, not a feature plan. It assumes v3.28.0 "Quill Hub" has shipped and the Quill quartet is live. Goal: convert engineering velocity into customers within 72h of posting.

**Goal:** Ship the assets, copy, infra hardening, and follow-through needed to make a single Show HN post convert ≥1% of front-page visitors into installs and ≥10% of installers into return-day-2 users.

**Architecture:** Three rails running in parallel —
1. **Demo rail** (silent ≤8s loop video + 5 still screenshots, hosted on the landing page, autoplay muted).
2. **Story rail** (the HN post body + the headline blog post on `docs/landing/index.html` + a follow-up "How we built local-first AI" technical post).
3. **Infra rail** (download CDN warm, GitHub release notes polished, `try.slab.app` deploy verified, crash-report endpoint live, an `analytics` opt-in that respects the local-first promise).

**Tech Stack:** Existing Slab repo. New: a tiny static page on `docs/landing/launch.html`, a recorded screencast (Sanjay action item), a Plausible-compatible self-hosted analytics endpoint behind the existing Slab Server Docker image, and a GitHub Discussions category for launch-day questions.

**Non-goals:** No new product features. No paid tier rollout. No Product Hunt yet (HN first, PH 2 weeks later to avoid cannibalization). No press outreach until day-3 metrics are in.

---

## Pre-flight check (run before Task 1)

```bash
# All four Quill releases must be live on GitHub before launch.
gh release view v3.25.0 --json isDraft,assets --jq '.isDraft, (.assets|length)'
gh release view v3.26.0 --json isDraft,assets --jq '.isDraft, (.assets|length)'
gh release view v3.27.0 --json isDraft,assets --jq '.isDraft, (.assets|length)'
gh release view v3.28.0 --json isDraft,assets --jq '.isDraft, (.assets|length)'
```
Expected for each: `false` (not draft) and `6` (six bundle assets). If any is draft or has <6 assets, **stop** and finish the release pipeline before continuing.

---

## Task 1: Write the HN post body (3 variants, pick one on launch day)

**Objective:** Three pre-drafted post bodies ready, each ≤220 chars title + ≤1800 char body, optimized for different audience entry points (engineer / power user / privacy crowd).

**Files:**
- Create: `docs/launch/hn-post-variants.md`
- Create: `docs/launch/title-ab.md` (just the 3 titles for quick A/B during the launch hour)

**Step 1: Write the file**

Content for `docs/launch/hn-post-variants.md`:

```markdown
# Show HN drafts — pick one at submit time

## Variant A — "Acrobat alternative" framing
**Title:** Show HN: Slab — a free, offline PDF workstation that replaces Acrobat ($239/yr)
**Body:**
Slab is a desktop PDF app for macOS, Windows, and Linux. Same core feature set as Adobe Acrobat Pro — merge, split, redact, OCR, fillable forms, three-way compare, AI chat — but free, GPL-3.0, and 100% offline. Your files never leave your machine, even for AI (we run Ollama locally).

I built it because I kept wanting to give my parents PDF software that wasn't a subscription trap or a phishing target. It also turned into the thing I use at work every day.

Three things I'd love feedback on:
1. The Quill form workflow — drag a flat PDF in, watch it become a fillable form, batch-fill 200 copies. (Acrobat's "Prepare Form" feature, but free.)
2. The Stack three-way redline — Litera Compare charges $400/seat/yr for this. We export it as a shareable PDF.
3. The plugin SDK (Foundry) — every action is a typed TS plugin you can fork.

Downloads: https://github.com/Sanjays2402/slab/releases/latest
Try in browser (no install): https://try.slab.app
Source: https://github.com/Sanjays2402/slab

Built solo over [N] months. Roast away.

## Variant B — "Local-first AI" framing
**Title:** Show HN: A PDF reader where the AI runs entirely on your laptop
**Body:**
Chat with a PDF without uploading it anywhere. Slab ships a local-first AI layer (Beacon) that talks to Ollama on your machine — summaries, semantic search, redact-PII, ask-the-document. Zero round trips to anyone's cloud. Works on a plane.

It's also a full PDF workstation: merge, split, OCR, three-way compare, fillable forms, batch automations. GPL-3.0, macOS + Windows + Linux + a self-hostable Docker server.

I built it because every "AI for PDFs" tool either uploads my files or pretends not to. Same goes for Acrobat-class editing — every alternative is a subscription.

What I'd love feedback on:
- Beacon's prompt design — it's a thin layer over Ollama models, what would you want it to do that it doesn't?
- The plugin SDK (Foundry) — typed TS, sandboxed, every command is forkable.
- The Docker server (`ghcr.io/sanjays2402/slab`) for headless / homelab use.

https://github.com/Sanjays2402/slab/releases/latest
https://try.slab.app

## Variant C — "Built this because…" framing (most HN-native)
**Title:** Show HN: Slab — I got tired of paying for PDF software
**Body:**
[Personal-voice opener: 2-3 sentences on why you started, what frustrated you, the moment you decided to build it.]

What's in the box:
- The standard PDF stuff (merge, split, OCR, redact, sign, compress, page ops).
- A few things competitors gate behind Pro tiers: three-way compare with shareable redline PDF export, batch automations (drop a folder, get clean output), fillable form designer + auto-detect + batch-fill.
- A local-first AI layer (Beacon) that runs against Ollama — your files never go anywhere.
- A plugin SDK (Foundry) so every action is forkable in TypeScript.

Free forever. GPL-3.0. No telemetry. No upsells.

https://github.com/Sanjays2402/slab/releases/latest · https://try.slab.app · https://github.com/Sanjays2402/slab

Honest feedback welcome — I've been heads-down for months and probably can't see the obvious flaws anymore.
```

**Step 2: Commit**

```bash
git add docs/launch/hn-post-variants.md
git -c user.email='51058514+Sanjays2402@users.noreply.github.com' \
    -c user.name='Cake (cron)' \
    commit -m "docs(launch): three HN post variants for Show HN day"
```

---

## Task 2: Record + encode the ≤8-second silent demo loop

**Objective:** One looping ≤8s, ≤2MB, h.264 muted MP4 + WebM that lives on the landing page hero. Demonstrates the full Quill story: drag flat PDF → it becomes a form → batch-fill 200. Three clicks. No talking.

**Files:**
- Create: `docs/landing/demos/quill-hero.mp4` (Sanjay records — see action item)
- Create: `docs/landing/demos/quill-hero.webm`
- Create: `docs/landing/demos/quill-hero.poster.png`
- Modify: `docs/landing/index.html` (swap hero `<img>` for `<video autoplay muted loop playsinline poster=…>`)

**Step 1: Write the recording script for Sanjay**

Create `docs/launch/recording-script.md`:

```markdown
# Demo recording — Quill Hero (≤8 seconds, silent, no cursor jitter)

## Setup
- macOS, retina display, scale capture to 1920×1080 final
- Slab in dark mode, accent: default purple
- Use `docs/launch/sample-pdfs/W9-flat.pdf` (single page IRS form, no fields)
- Pre-stage a CSV of 5 fake names + addresses at `~/Desktop/recipients.csv`
- QuickTime → File → New Screen Recording → select window only
- 60fps if possible

## The 8 seconds
- **0.0–1.2s** Slab open, empty state. Drag `W9-flat.pdf` into the window.
- **1.2–3.0s** Press `Cmd+Shift+Q` — Quill Hub opens. Auto-detect tab is active.
  Fields appear highlighted on the page (yellow outlines fading in).
- **3.0–5.0s** Click "Accept all" — fields become editable.
- **5.0–7.0s** Click "Batch" tab → drag the CSV → "Generate 5 PDFs".
- **7.0–8.0s** Output folder opens with 5 PDFs named after each row.

## Encode
```bash
# After QuickTime export to ~/Desktop/quill-hero-raw.mov:
ffmpeg -i ~/Desktop/quill-hero-raw.mov \
  -vf "scale=1920:-2,fps=30" -an \
  -c:v libx264 -crf 26 -preset slow -movflags +faststart \
  docs/landing/demos/quill-hero.mp4

ffmpeg -i ~/Desktop/quill-hero-raw.mov \
  -vf "scale=1920:-2,fps=30" -an \
  -c:v libvpx-vp9 -crf 35 -b:v 0 \
  docs/landing/demos/quill-hero.webm

ffmpeg -i docs/landing/demos/quill-hero.mp4 \
  -vf "select=eq(n\,0)" -q:v 2 docs/landing/demos/quill-hero.poster.png
```

Both files must be ≤2.0 MB. If over, drop crf to 30 and re-encode.
```

**Step 2: Patch the landing page**

In `docs/landing/index.html`, locate the hero image block (the first `<img src="…hero…">`) and replace with:

```html
<video class="hero-demo"
       autoplay muted loop playsinline
       poster="demos/quill-hero.poster.png"
       width="1920" height="1080"
       aria-label="Slab demo: a flat PDF becomes a fillable form, then batch-fills 5 copies, in three clicks.">
  <source src="demos/quill-hero.webm" type="video/webm">
  <source src="demos/quill-hero.mp4"  type="video/mp4">
  <!-- Fallback for browsers that block autoplay or lack <video>. -->
  <img src="demos/quill-hero.poster.png" alt="Slab demo poster">
</video>
```

Add to `docs/landing/styles.css`:

```css
.hero-demo {
  width: 100%;
  max-width: 1100px;
  aspect-ratio: 16 / 9;
  border-radius: 14px;
  box-shadow: 0 30px 80px -20px rgba(0,0,0,.45);
  background: #0b0d12;
}
@media (prefers-reduced-motion: reduce) {
  .hero-demo { animation: none; }
  /* Browser will respect the user's motion preference and pause autoplay-loop in many engines. */
}
```

**Step 3: Verify locally**

```bash
python3 -m http.server -d docs/landing 8765 &
sleep 1
curl -sI http://localhost:8765/index.html | head -1   # expect HTTP/1.0 200 OK
curl -sI http://localhost:8765/demos/quill-hero.mp4 | grep -i content-length
kill %1
```
Expected: video file size ≤2,000,000 bytes.

**Step 4: Commit**

```bash
git add docs/launch/recording-script.md docs/landing/demos docs/landing/index.html docs/landing/styles.css
git -c user.email='51058514+Sanjays2402@users.noreply.github.com' \
    -c user.name='Cake (cron)' \
    commit -m "feat(landing): hero demo video for Show HN launch"
```

**Sanjay action item:** Record the 8-second loop using the script above. Cake cannot do this — needs a real Slab window on a real screen. **This is the launch blocker.**

---

## Task 3: Five hero screenshots (no chrome, no cursor, dark mode)

**Objective:** Five stills that tell the full product story when scrolled. Each ≤500 KB PNG, 16:9 at 1920×1080.

**Files:**
- Create: `docs/landing/screens/01-reader.png`
- Create: `docs/landing/screens/02-quill-hub.png`
- Create: `docs/landing/screens/03-stack-redline.png`
- Create: `docs/landing/screens/04-beacon-chat.png`
- Create: `docs/landing/screens/05-foundry-plugins.png`
- Modify: `docs/landing/index.html` (add a "What's in the box" grid below the hero)

**Step 1: Capture script for Sanjay**

Create `docs/launch/screenshots.md`:

```markdown
# Launch screenshots — capture checklist

Each screenshot:
- 1920×1080 (16:9), retina @2x source then downscale.
- Dark mode, default accent.
- Window only (no menu bar, no dock).
- Cursor hidden (defaults write com.apple.QuickTime.X11 IgnoreCursor 1, or use Screenshot.app → Options → Hide cursor).
- ImageOptim → PNG ≤500 KB.

## 01 — Reader
A real-looking research paper (e.g. the Anthropic constitutional AI PDF), page 5 visible, sidebar showing TOC, a highlight on a quote, reading-progress dots in corner.

## 02 — Quill Hub
QuillHubPanel open on the "Design" sub-tab, mid-drag of a checkbox field on a W-9. Five existing fields visible in green outlines.

## 03 — Stack three-way redline
Diff3Panel showing base/mine/theirs columns, two conflicts highlighted, one resolved (green checkmark), one pending. "Export Redline PDF" button visible.

## 04 — Beacon chat
A scientific paper open, Beacon side panel with a 3-message conversation ending in a cited summary. Citation chip in panel highlights the source paragraph in the document.

## 05 — Foundry plugins
Plugin Store panel showing 6-8 plugin cards (the three first-party samples + sample marketplace entries). One card hovered showing an "Install" CTA.
```

**Step 2: Patch the landing grid**

Append to `docs/landing/index.html` (immediately after the hero `<video>` block, before the existing first content section):

```html
<section class="feature-grid" aria-label="What's in the box">
  <h2>What's in the box</h2>
  <div class="grid">
    <figure>
      <img src="screens/01-reader.png" alt="Slab reader with a paper open, sidebar TOC, highlighted quote.">
      <figcaption><strong>Reader</strong> — keyboard-first, dark-mode native, reading progress that persists.</figcaption>
    </figure>
    <figure>
      <img src="screens/02-quill-hub.png" alt="Quill Hub mid-design, dragging a checkbox onto a W-9.">
      <figcaption><strong>Quill</strong> — detect, design, fill, batch. Adobe charges $239/yr for less.</figcaption>
    </figure>
    <figure>
      <img src="screens/03-stack-redline.png" alt="Three-way diff panel with conflicts and an Export Redline button.">
      <figcaption><strong>Stack</strong> — three-way compare with shareable redline PDF. Litera charges $400/yr.</figcaption>
    </figure>
    <figure>
      <img src="screens/04-beacon-chat.png" alt="Beacon chat panel with cited summary referencing the open paper.">
      <figcaption><strong>Beacon</strong> — local AI. Your files never leave the machine. Works on a plane.</figcaption>
    </figure>
    <figure>
      <img src="screens/05-foundry-plugins.png" alt="Foundry plugin store with sample plugins and an Install button.">
      <figcaption><strong>Foundry</strong> — every action is a typed TypeScript plugin you can fork.</figcaption>
    </figure>
  </div>
</section>
```

CSS to append to `docs/landing/styles.css`:

```css
.feature-grid { max-width: 1180px; margin: 96px auto; padding: 0 24px; }
.feature-grid h2 { font-size: 36px; font-weight: 700; margin-bottom: 36px; letter-spacing: -0.02em; }
.feature-grid .grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(280px, 1fr));
  gap: 28px;
}
.feature-grid figure { margin: 0; }
.feature-grid img {
  width: 100%; aspect-ratio: 16/9; border-radius: 10px;
  background: #0b0d12;
  box-shadow: 0 14px 38px -14px rgba(0,0,0,.5);
}
.feature-grid figcaption {
  font-size: 14px; line-height: 1.55; color: #cbd0d8;
  margin-top: 14px;
}
.feature-grid figcaption strong { color: #fff; font-weight: 600; }
```

**Step 3: Verify image sizes**

```bash
find docs/landing/screens -name '*.png' -size +500k
```
Expected: empty output. Any file listed is over budget — rerun ImageOptim.

**Step 4: Commit**

```bash
git add docs/launch/screenshots.md docs/landing/screens docs/landing/index.html docs/landing/styles.css
git -c user.email='51058514+Sanjays2402@users.noreply.github.com' \
    -c user.name='Cake (cron)' \
    commit -m "feat(landing): five hero screenshots + What's-in-the-box grid"
```

---

## Task 4: Harden `try.slab.app` for launch-day traffic

**Objective:** Verify the in-browser demo handles a 10× traffic spike without falling over. Add a graceful "we're popular right now" overlay if Cloudflare Pages or the Worker is rate-limiting.

**Files:**
- Modify: `try-slab/src/lib/Overload.svelte` (create)
- Modify: `try-slab/src/routes/+layout.svelte` (mount overlay)
- Modify: `docs/ops/try-slab-deploy.md` (add launch-day section)

**Step 1: Write the overload-detection component**

Create `try-slab/src/lib/Overload.svelte`:

```svelte
<script lang="ts">
  let { } = $props();
  let visible = $state(false);
  let dismissed = $state(false);

  function show() { if (!dismissed) visible = true; }
  function hide() { visible = false; dismissed = true; }

  // The page bootstrap fires this if any /api/* call returns 429 or 503.
  if (typeof window !== 'undefined') {
    window.addEventListener('slab:overloaded', show as EventListener);
  }
</script>

{#if visible}
  <div class="overload" role="status" aria-live="polite">
    <h3>Slab is getting more traffic than usual.</h3>
    <p>
      The in-browser demo is rate-limited so it doesn't melt. Two faster paths:
    </p>
    <ul>
      <li><a href="https://github.com/Sanjays2402/slab/releases/latest">Download the desktop app</a> — it's the same engine, fully offline.</li>
      <li><a href="https://github.com/Sanjays2402/slab#run-it-as-a-server">Run the Docker image</a> — <code>docker run -p 8080:8080 ghcr.io/sanjays2402/slab:latest</code>.</li>
    </ul>
    <button onclick={hide}>Dismiss</button>
  </div>
{/if}

<style>
  .overload {
    position: fixed; bottom: 24px; right: 24px;
    max-width: 380px; padding: 18px 20px;
    background: rgba(20,22,28,.95);
    border: 1px solid rgba(255,255,255,.08);
    border-radius: 12px; color: #e6e9ef;
    box-shadow: 0 24px 60px -10px rgba(0,0,0,.6);
    backdrop-filter: blur(14px);
    z-index: 50; font: 14px/1.5 system-ui, sans-serif;
  }
  .overload h3 { margin: 0 0 10px; font-size: 16px; }
  .overload ul { margin: 10px 0 14px; padding-left: 20px; }
  .overload code { background: rgba(255,255,255,.08); padding: 2px 6px; border-radius: 4px; font-size: 12px; }
  .overload button {
    background: transparent; border: 1px solid rgba(255,255,255,.18);
    color: #fff; padding: 6px 12px; border-radius: 6px; cursor: pointer;
  }
  .overload button:hover { background: rgba(255,255,255,.08); }
</style>
```

**Step 2: Mount it in the layout**

In `try-slab/src/routes/+layout.svelte`, just before `</body>` or the slot end:

```svelte
<script lang="ts">
  import Overload from '$lib/Overload.svelte';
  // …existing
</script>

<!-- existing layout -->
<slot />
<Overload />
```

And in whatever module makes the API calls, on any 429/503 response:

```ts
if (res.status === 429 || res.status === 503) {
  window.dispatchEvent(new CustomEvent('slab:overloaded'));
}
```

**Step 3: Add launch-day ops checklist**

Append to `docs/ops/try-slab-deploy.md`:

```markdown
## Launch-day checklist

- [ ] Cloudflare Pages: bump the project's "Performance" plan one tier for the launch week if not already.
- [ ] Set up a Cloudflare Worker rate limit at 30 req/min per IP on `/api/*` to keep abuse contained without breaking real users.
- [ ] Verify `try.slab.app` 404 page links to the GitHub releases page.
- [ ] Warm the CDN: `for path in / /quill /stack /beacon; do curl -s -o /dev/null https://try.slab.app$path; done`
- [ ] Confirm `Overload.svelte` shows when API returns 429 by curling 50 requests against `/api/health` in 5 seconds.
```

**Step 4: Verify locally**

```bash
cd try-slab && pnpm dev &
sleep 4
curl -s http://localhost:5173/ | grep -c "Slab" # expect ≥1
kill %1
```

**Step 5: Commit**

```bash
git add try-slab/src/lib/Overload.svelte try-slab/src/routes/+layout.svelte docs/ops/try-slab-deploy.md
git -c user.email='51058514+Sanjays2402@users.noreply.github.com' \
    -c user.name='Cake (cron)' \
    commit -m "feat(try): overload banner + launch-day ops checklist"
```

---

## Task 5: Polish the v3.28.0 GitHub release notes into launch-day copy

**Objective:** Rewrite `docs/release-notes/v3.28.0.md` (or wherever the Quill Hub release notes live) into a body that reads like Stripe's changelog, not git log. Lead with the wow, screenshot at the top, table of new keyboard shortcuts at the bottom.

**Files:**
- Modify: `docs/releases/v3.28.0.md` (rewrite — keep technical changelog in an Appendix)

**Step 1: Template**

```markdown
# v3.28.0 — Quill Hub

**One panel. Four powers. Three clicks from flat PDF to 200 filled copies.**

![Quill Hub mid-design](../landing/screens/02-quill-hub.png)

Adobe Acrobat Pro charges $239/yr for the "Prepare Form" workflow. Slab's Quill Hub gives you the same workflow — plus auto-detection, batch fill, and CSV merge — in a single panel, offline, GPL-3.0.

## What you can do in v3.28.0

- **Detect** — drag a flat PDF in; Slab finds the blanks, checkboxes, and signature lines automatically.
- **Design** — drag-to-draw any field you want to add or correct.
- **Fill** — type into the resulting form like any other PDF reader (but faster, with Tab key navigation that actually works).
- **Batch** — drop a CSV of recipients; get back a folder of filled PDFs named per row.

## The shortcut to remember

`Cmd/Ctrl + Shift + Q` — opens the Hub from anywhere, on the right tab for what you just did.

## What changed under the hood (Appendix)

- New `QuillHubPanel.svelte` unifying the four previous panels into one with sub-tabs.
- Shared `src/lib/quill.ts` store coordinates state between tabs (no re-detection on every navigation).
- One new shortcut, one new palette entry, one new settings section, one new What's-New row.
- Empty state + "Next: …" CTA footer so the workflow never dead-ends.

## Compatibility

- All v3.25–v3.27 keyboard shortcuts continue to work (`Cmd+Shift+A` for auto-detect, `Cmd+Shift+F` for design, etc.). Cmd+Shift+Q is additive.
- Existing form files open as before; round-trip safe.

## Get it

- Desktop: [Releases page](https://github.com/Sanjays2402/slab/releases/tag/v3.28.0)
- Browser demo: https://try.slab.app
- Docker: `docker pull ghcr.io/sanjays2402/slab:v3.28.0`
```

**Step 2: Commit**

```bash
git add docs/releases/v3.28.0.md
git -c user.email='51058514+Sanjays2402@users.noreply.github.com' \
    -c user.name='Cake (cron)' \
    commit -m "docs(release): polish v3.28.0 release notes for Show HN day"
```

**Step 3: Update the live GitHub release**

```bash
gh release edit v3.28.0 --notes-file docs/releases/v3.28.0.md
gh release view v3.28.0 --web   # eyeball it
```

---

## Task 6: Add a `/launch` static page with the canonical pitch + downloads

**Objective:** A single URL — `slab.app/launch` (or `sanjays2402.github.io/slab/launch.html`) — that HN visitors land on. Above-the-fold: hero video, 3 download buttons (mac/win/linux), "try in browser" link. Below: the five-panel grid, a fold-down FAQ, GitHub link.

**Files:**
- Create: `docs/landing/launch.html`
- Create: `docs/landing/launch.css`

**Step 1: Write the file**

`docs/landing/launch.html` (copy-paste, then tweak copy):

```html
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>Slab — a free, offline PDF workstation</title>
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <meta name="description" content="Slab is a free, GPL-3.0, fully-offline PDF workstation for macOS, Windows, and Linux. Replaces Adobe Acrobat. Your files never leave your machine — even for AI.">
  <meta property="og:title" content="Slab — a free, offline PDF workstation">
  <meta property="og:description" content="Replaces Adobe Acrobat. 100% local. GPL-3.0.">
  <meta property="og:image" content="https://sanjays2402.github.io/slab/social-preview.png">
  <link rel="stylesheet" href="styles.css">
  <link rel="stylesheet" href="launch.css">
</head>
<body class="launch-page">

<header class="launch-hero">
  <h1>The PDF app your files never leave.</h1>
  <p class="sub">Free. GPL-3.0. macOS · Windows · Linux. Replaces Adobe Acrobat ($239/yr).</p>

  <video class="hero-demo"
         autoplay muted loop playsinline
         poster="demos/quill-hero.poster.png"
         aria-label="Demo: a flat PDF becomes a fillable form, then batch-fills 5 copies, in three clicks.">
    <source src="demos/quill-hero.webm" type="video/webm">
    <source src="demos/quill-hero.mp4"  type="video/mp4">
    <img src="demos/quill-hero.poster.png" alt="Slab demo poster">
  </video>

  <div class="cta-row">
    <a class="cta primary" href="https://github.com/Sanjays2402/slab/releases/latest/download/Slab.dmg">Download for Mac</a>
    <a class="cta"         href="https://github.com/Sanjays2402/slab/releases/latest/download/Slab-x64.msi">Windows</a>
    <a class="cta"         href="https://github.com/Sanjays2402/slab/releases/latest/download/Slab.AppImage">Linux</a>
  </div>
  <p class="alt">…or <a href="https://try.slab.app">try it in your browser</a> · <a href="https://github.com/Sanjays2402/slab">view source</a></p>
</header>

<section class="feature-grid" aria-label="What's in the box">
  <h2>What's in the box</h2>
  <div class="grid">
    <!-- The same five figures as in index.html — keep in sync. -->
  </div>
</section>

<section class="faq">
  <h2>Common questions</h2>
  <details>
    <summary>Is it really free? What's the catch?</summary>
    <p>It really is. GPL-3.0, no telemetry, no upsells. I built it because I was tired of paying for PDF software. If a paid tier ever appears, the core feature set stays free.</p>
  </details>
  <details>
    <summary>What about the AI features — do my files get uploaded?</summary>
    <p>No. Beacon (the AI layer) talks to Ollama running on your own machine. Air-gap your laptop and the AI still works. If you want cloud models, you can plug in your own API key — your choice, not the default.</p>
  </details>
  <details>
    <summary>How does it compare to Acrobat / PDF Expert / Foxit?</summary>
    <p>Acrobat is $239/yr and ships your files to Adobe's cloud for AI. PDF Expert is Mac-only and $79/yr. Foxit is $129/yr. Slab is free, cross-platform, and local-first. We don't have decades of compatibility edge cases handled (yet) — file bugs and we'll fix them.</p>
  </details>
  <details>
    <summary>Can I self-host the server piece?</summary>
    <p>Yes — <code>docker run -p 8080:8080 ghcr.io/sanjays2402/slab:latest</code>. Same Rust core, headless HTTP API, drop a PDF on the page. See <a href="https://github.com/Sanjays2402/slab/blob/main/docs/server.md">docs/server.md</a>.</p>
  </details>
  <details>
    <summary>Can I extend it?</summary>
    <p>Foundry is the plugin SDK — typed TypeScript, sandboxed, every command and AI provider is forkable. Three sample plugins ship with the binary; read their source from inside the app.</p>
  </details>
</section>

<footer class="launch-footer">
  <p>Made by <a href="https://github.com/Sanjays2402">Sanjay</a>. <a href="https://news.ycombinator.com/from?site=github.com/Sanjays2402/slab">Discuss on HN</a>.</p>
</footer>

</body>
</html>
```

**Step 2: `launch.css`** — minimal, inherits typography from `styles.css`:

```css
.launch-page { background: #0a0c10; color: #e6e9ef; }
.launch-hero { padding: 80px 24px 60px; text-align: center; max-width: 1180px; margin: 0 auto; }
.launch-hero h1 { font-size: 56px; font-weight: 700; letter-spacing: -0.03em; margin: 0 0 18px; }
.launch-hero .sub { font-size: 20px; color: #b0b6c2; margin: 0 0 36px; }
.launch-hero .hero-demo { margin: 0 auto 30px; max-width: 1100px; }
.cta-row { display: flex; gap: 12px; justify-content: center; flex-wrap: wrap; }
.cta {
  background: rgba(255,255,255,.06); color: #fff; border: 1px solid rgba(255,255,255,.12);
  padding: 12px 20px; border-radius: 8px; text-decoration: none;
  font-weight: 500; transition: background 120ms;
}
.cta:hover { background: rgba(255,255,255,.12); }
.cta.primary { background: #7c5cff; border-color: #7c5cff; }
.cta.primary:hover { background: #8c70ff; }
.alt { font-size: 13px; color: #8a8f9b; margin-top: 18px; }
.alt a { color: #b0b6c2; }
.faq { max-width: 760px; margin: 0 auto 96px; padding: 0 24px; }
.faq h2 { font-size: 28px; font-weight: 700; margin-bottom: 24px; }
.faq details {
  border-top: 1px solid rgba(255,255,255,.08); padding: 18px 0;
}
.faq summary {
  font-size: 17px; font-weight: 500; cursor: pointer; list-style: none;
}
.faq summary::-webkit-details-marker { display: none; }
.faq summary::after { content: " +"; color: #8a8f9b; }
.faq details[open] summary::after { content: " –"; }
.faq p { color: #cbd0d8; line-height: 1.6; margin: 12px 0 0; }
.launch-footer { text-align: center; padding: 30px; color: #8a8f9b; font-size: 13px; }
.launch-footer a { color: #b0b6c2; }
```

**Step 3: Verify**

```bash
python3 -m http.server -d docs/landing 8765 &
sleep 1
curl -sI http://localhost:8765/launch.html | head -1
kill %1
```

**Step 4: Commit**

```bash
git add docs/landing/launch.html docs/landing/launch.css
git -c user.email='51058514+Sanjays2402@users.noreply.github.com' \
    -c user.name='Cake (cron)' \
    commit -m "feat(landing): /launch page for Show HN day"
```

---

## Task 7: Launch-day runbook (the 4-hour playbook)

**Objective:** A single file Sanjay reads top-to-bottom on launch morning. No ambiguity, no creative choices, just commands and timestamps.

**Files:**
- Create: `docs/launch/RUNBOOK.md`

**Step 1: Write the file**

```markdown
# Launch-day runbook (T-0 = HN post submit time)

## T-24h (the day before)

- [ ] `gh release view v3.28.0` → confirm not draft, 6 assets.
- [ ] Visit `try.slab.app` from an incognito tab. Drag a PDF. Confirm Quill Hub works.
- [ ] Visit https://sanjays2402.github.io/slab/launch.html — verify video autoplays.
- [ ] Confirm hero video ≤2MB, screenshots ≤500KB each.
- [ ] Pre-warm Cloudflare cache:
      `for p in / /launch.html /demos/quill-hero.mp4 /screens/01-reader.png; do
         curl -s -o /dev/null https://sanjays2402.github.io/slab$p; done`
- [ ] Pick the HN post variant (A/B/C from `hn-post-variants.md`).
- [ ] Set phone to silent. Hydrate.

## T-1h

- [ ] Post draft into HN's submit form, don't click submit yet.
- [ ] Have these three URLs in a bookmark folder, in this order:
      1. https://news.ycombinator.com/item?id=<your-post> (will redirect after submit)
      2. https://github.com/Sanjays2402/slab (to monitor stars)
      3. https://sanjays2402.github.io/slab/launch.html (your own landing)

## T-0 — Submit

- [ ] Submit at **7:30 AM Pacific Tuesday/Wednesday/Thursday** (best HN window per data).
- [ ] Do NOT upvote your own post. Do NOT ask friends to upvote — flagging is automatic and brutal.
- [ ] The first comment within 15 minutes shapes the thread. If nobody has commented, post a substantive technical comment yourself (architecture, why-this-not-that). Not "thanks for checking it out."

## T+0 to T+4h — Active monitoring

- [ ] Refresh the post every 5 minutes. Reply to every comment within 15 minutes.
- [ ] Lead every reply with the answer, not "Great question!". Cake will be annoyed otherwise.
- [ ] If someone reports a bug: thank them, link the issue tracker, file the issue *in front of them* in the thread: `Filed as #N`. This signals competence.
- [ ] If `try.slab.app` overloads, the Overload banner should fire. If it doesn't, post a comment with the Docker one-liner: `docker run -p 8080:8080 ghcr.io/sanjays2402/slab:latest`.
- [ ] Track stars/installs/issues every hour in `.cron-state/launch-day-stats.md` (Cake auto-updates this on every off-hours tick).

## T+24h

- [ ] Write the follow-up "lessons learned" post for the README + a blog post on what surprised you.
- [ ] Tag every legitimate bug filed during launch as `launch-day` for prioritization.
- [ ] Schedule the v3.28.1 patch release if any high-severity bugs landed.

## What NOT to do on launch day

- ❌ Don't ship new features. Bugs only.
- ❌ Don't post to Product Hunt on the same day. Wait 2 weeks.
- ❌ Don't argue with trolls. The thread reads better when you don't.
- ❌ Don't promise a roadmap item without checking `docs/plans/`. The roadmap is on GitHub.

## Cake's role during launch (automated)

Every off-hours tick during the launch window will:
- Poll `gh api repos/Sanjays2402/slab --jq '.stargazers_count'` and write to `.cron-state/launch-day-stats.md`.
- Poll `gh issue list --label launch-day` and notify on new entries.
- NOT ship new features. NOT touch `main`. Just observe, log, and surface anything that needs Sanjay's eyes.
```

**Step 2: Commit**

```bash
git add docs/launch/RUNBOOK.md
git -c user.email='51058514+Sanjays2402@users.noreply.github.com' \
    -c user.name='Cake (cron)' \
    commit -m "docs(launch): T-24h to T+24h launch-day runbook"
```

---

## Task 8: Set up the launch-stats tracking + freeze-feature mode

**Objective:** Two cron behaviors flipped during launch week — (a) auto-log GitHub stars / issues every tick to `.cron-state/launch-day-stats.md`, (b) refuse to ship new features (any tick on a `feature/*` branch is auto-aborted with a `[cron] launch-week-freeze` message). Re-enabled by deleting `.cron-state/LAUNCH_MODE`.

**Files:**
- Create: `.cron-state/launch-day-stats.md` (initial empty template)
- Modify: STATE.md to document the LAUNCH_MODE flag semantics

**Step 1: Create the stats file**

`.cron-state/launch-day-stats.md`:

```markdown
# Launch-day stats — append-only

Each row: `<ISO-timestamp> <stars> <open-issues> <new-issues-since-prev>`

```

**Step 2: Document the flag in STATE.md**

Add a new section after the current STATUS header:

```markdown
## LAUNCH_MODE semantics (when present)

If `.cron-state/LAUNCH_MODE` exists, off-hours ticks must:
1. Append a row to `.cron-state/launch-day-stats.md` with current stars + open issues count.
2. Refuse to ship any commit on a `feature/*` branch — only `hotfix/*` and `main` are eligible for changes.
3. Surface any new issue tagged `launch-day` in the Telegram delivery.
4. Skip the SHIP-SIZE rule (a tick that just logs stats is acceptable).

Remove `LAUNCH_MODE` to return to normal behavior. The file's contents don't matter; presence is the signal.
```

**Step 3: Commit**

```bash
git add .cron-state/launch-day-stats.md .cron-state/STATE.md
git -c user.email='51058514+Sanjays2402@users.noreply.github.com' \
    -c user.name='Cake (cron)' \
    commit -m "feat(cron): launch-mode flag + stats tracking"
```

---

## Execution sequencing (multi-tick)

A single off-hours tick fits **2-3 tasks** of this plan comfortably (each task is ~50-150 LOC). Suggested grouping:

| Tick | Tasks | Why |
|------|-------|-----|
| 1 | 1, 7 | HN post variants + runbook — pure copy, fast wins. |
| 2 | 4, 8 | Overload banner + launch-mode flag — infra hardening, no demo assets needed yet. |
| 3 | 5, 6 | Release notes polish + /launch page. |
| 4 | 2, 3 (markdown only) | Recording + screenshot scripts. Sanjay action items flagged. |
| 5 | wait for Sanjay's assets | Cake cannot record video or take screenshots in cron context. |
| 6 | wire the assets into the HTML once they exist | Final assembly tick. |

Total: ~6 ticks of off-hours work, plus one Sanjay action block (recording + screenshots, ~2h of his time).

---

## Verification before declaring launch-ready

```bash
# All assets present?
test -f docs/landing/demos/quill-hero.mp4 && \
test -f docs/landing/demos/quill-hero.webm && \
test -f docs/landing/demos/quill-hero.poster.png && \
test $(find docs/landing/screens -name '*.png' | wc -l) -ge 5 && \
echo "ASSETS OK" || echo "ASSETS MISSING"

# Pages serve cleanly?
python3 -m http.server -d docs/landing 8765 &
sleep 1
for p in / launch.html demos/quill-hero.mp4 screens/01-reader.png; do
  code=$(curl -s -o /dev/null -w "%{http_code}" http://localhost:8765/$p)
  echo "$p → $code"
done
kill %1

# Release is real?
gh release view v3.28.0 --json isDraft,assets --jq '"draft=\(.isDraft) assets=\(.assets|length)"'
# Expect: draft=false assets=6

# Runbook exists?
test -f docs/launch/RUNBOOK.md && echo "RUNBOOK OK"
```

When all four checks return their expected output, **the launch is armed**. Sanjay picks the post variant and submits.

---

## Buy-Button verdict on this plan

Not a feature plan — but it is the bridge that turns four feature plans (v3.25–28) into actual paying customers. Without it, Quill Hub ships into a void.

- **Pay-for-it test:** N/A (no new feature).
- **Notice-it test:** ✅ Landing page hero changes from static to live video.
- **Pick-us test:** ✅ HN visitors arrive at a page that explains why Slab beats Acrobat in 8 seconds.
- **Tell-a-friend test:** ✅ The whole point.

## Wow quotient

The 8-second silent loop showing "flat PDF → fillable form → 200 batch-filled copies in 3 clicks" IS the wow. It's the single most screenshottable thing in the product. Mark `LAST_WOW_TICK_AT` when Task 2 lands.
