<script lang="ts">
  /**
   * ToolboxPanel — iLovePDF-style mega tool grid.
   *
   * One landing screen that surfaces every tool Slab ships, grouped by
   * category, filterable by chip, click-to-open. Mirrors the public
   * landing page at /docs/landing so first-run users see the same
   * "every tool in one place" pitch they saw on the web.
   */
  import { createEventDispatcher } from "svelte";

  type Cat = "organize" | "optimize" | "convert" | "edit" | "security" | "intelligence";
  type Tool = {
    id: string;
    label: string;
    blurb: string;
    cat: Cat;
    badge?: string;
  };

  // Single source of truth — tool IDs match the `active` panel IDs in +page.svelte.
  const tools: Tool[] = [
    // ORGANIZE
    { id: "merge",         cat: "organize", label: "Merge",            blurb: "Combine PDFs in the order you want — drag, drop, done." },
    { id: "split",         cat: "organize", label: "Split",            blurb: "Pull one page, a range, or break a 400-page report into chapters." },
    { id: "split-chapter", cat: "organize", label: "Split by chapter", blurb: "Auto-detect chapter boundaries from the outline." },
    { id: "pages",         cat: "organize", label: "Pages",            blurb: "Visual page grid — reorder, delete, insert by drag and drop." },
    { id: "pages-list",    cat: "organize", label: "Pages (list)",     blurb: "List-view page editor for big documents." },
    { id: "insert",        cat: "organize", label: "Insert",           blurb: "Insert pages from another PDF at any position." },
    { id: "labels",        cat: "organize", label: "Page labels",      blurb: "Roman front matter, Arabic body, custom prefixes." },
    { id: "numbers",       cat: "organize", label: "Page numbers",     blurb: "Stamp page numbers — position, font, restart per section." },
    { id: "nup",           cat: "organize", label: "N-up layout",      blurb: "2, 4, 6, or 9 pages per sheet for handouts." },

    // OPTIMIZE
    { id: "compress",      cat: "optimize", label: "Compress",         blurb: "Real image downsampling + stream re-encoding. 80 MB → 12 MB." },
    { id: "compact",       cat: "optimize", label: "Compact",          blurb: "Deduplicate streams and strip unused objects." },
    { id: "ocr",           cat: "optimize", label: "OCR",              blurb: "Turn scans into searchable, selectable text." },
    { id: "metadata",      cat: "optimize", label: "Metadata",         blurb: "Title, author, subject, keywords, dates." },
    { id: "repair",        cat: "optimize", label: "Repair",           blurb: "Recover content from corrupt PDFs — rebuild xref tables." },
    { id: "grayscale",     cat: "optimize", label: "Grayscale",        blurb: "Convert color pages to grayscale to shrink and ink-save." },

    // CONVERT
    { id: "extract",       cat: "convert",  label: "Extract text",     blurb: "Pull plain text or structured blocks out." },
    { id: "tables",        cat: "convert",  label: "Tables → CSV",     blurb: "Auto-detect tables, stitch multi-page, export CSV." },
    { id: "markdown",      cat: "convert",  label: "Markdown → PDF",   blurb: "Typeset Markdown with code blocks, tables, headings." },
    { id: "convert",       cat: "convert",  label: "Convert",          blurb: "PDF ↔ images, raster, vector exports." },

    // EDIT
    { id: "edit-text",     cat: "edit",     label: "Edit text",        blurb: "Double-click text to edit. Fonts and layout preserved." },
    { id: "annotate",      cat: "edit",     label: "Annotate",         blurb: "Highlight, sticky-note, freehand draw." },
    { id: "crop",          cat: "edit",     label: "Crop",             blurb: "Trim margins with handles or millimeter input." },
    { id: "watermark",     cat: "edit",     label: "Watermark",        blurb: "Diagonal text or image, opacity, per-page or whole doc." },
    { id: "stamp",         cat: "edit",     label: "Legal stamp",      blurb: "DRAFT, APPROVED, CONFIDENTIAL — or your own image." },
    { id: "headerfooter",  cat: "edit",     label: "Header / footer",  blurb: "Page numbers, dates, paths, branding, classification." },
    { id: "forms",         cat: "edit",     label: "Forms",            blurb: "Detect, fill, or build interactive PDFs." },
    { id: "bates",         cat: "edit",     label: "Bates numbering",  blurb: "Litigation-grade sequential IDs across a production set." },
    { id: "diff",          cat: "edit",     label: "Diff",             blurb: "Visual + semantic side-by-side diff." },
    { id: "stack",         cat: "edit",     label: "Compare",          blurb: "Stack-compare versions across folders." },
    { id: "flatten",       cat: "edit",     label: "Flatten",          blurb: "Bake annotations and form fields into the page." },

    // SECURITY
    { id: "redact",        cat: "security", label: "Redact",           blurb: "Physically excise text, images, metadata." },
    { id: "autoredact",    cat: "security", label: "Auto-redact",      blurb: "Find names, SSNs, phones, emails — and bake bars in." },
    { id: "pii",           cat: "security", label: "PII scrub",        blurb: "Detect PII across the whole document tree." },
    { id: "veil",          cat: "security", label: "Veil",             blurb: "Verify redactions actually removed the content." },
    { id: "encrypt",       cat: "security", label: "Encrypt",          blurb: "Password-protect a PDF — universal-compatibility lock and unlock." },
    { id: "sanitize",      cat: "security", label: "Sanitize",         blurb: "Strip JavaScript, embedded files, hidden layers." },
    { id: "sign",          cat: "security", label: "Sign",             blurb: "Drop a signature, flatten to 150 DPI — locked." },

    // PDF INTELLIGENCE
    { id: "beacon",        cat: "intelligence", label: "Beacon AI",       blurb: "Ask your PDF anything — runs locally, no upload.",       badge: "OFFLINE" },
    { id: "search",        cat: "intelligence", label: "Beacon search",   blurb: "Semantic search across your whole library.",            badge: "LOCAL" },
    { id: "library",       cat: "intelligence", label: "Library",         blurb: "Your indexed PDF library with smart filters." },
    { id: "library-search",cat: "intelligence", label: "Library search",  blurb: "Full-text search across every indexed PDF." },
    { id: "citations",     cat: "intelligence", label: "Citations",       blurb: "Auto-extract bibliographic references." },
    { id: "study",         cat: "intelligence", label: "Study",           blurb: "Flashcards and recall from any document." },
    { id: "glossary",      cat: "intelligence", label: "Glossary",        blurb: "Build a term glossary with context-aware definitions." },
    { id: "voice",         cat: "intelligence", label: "Voice",           blurb: "Listen to any PDF — local TTS, configurable voice." },
    { id: "loom",          cat: "intelligence", label: "Loom",            blurb: "Auto-tag StructTree for PDF/UA-1 accessibility.",        badge: "PDF/UA-1" },
    { id: "bedrock",       cat: "intelligence", label: "Bedrock",         blurb: "Flatten to ISO 19005 PDF/A — 50-year archival.",         badge: "PDF/A" },
    { id: "loupe",         cat: "intelligence", label: "Loupe",           blurb: "PDF/A preflight check — severity-ranked findings." },
    { id: "press",         cat: "intelligence", label: "Press",           blurb: "FOGRA51/GRACoL2013 ICC, OutputIntent — press-ready.",    badge: "PDF/X-4" },
    { id: "slides",        cat: "intelligence", label: "Slides",          blurb: "Present your PDF as a deck with speaker notes." },
    { id: "theater",       cat: "intelligence", label: "Theater",         blurb: "Distraction-free reader for long-form study." },
  ];

  const categories: { id: Cat | "all"; label: string }[] = [
    { id: "all",          label: "All" },
    { id: "organize",     label: "Organize" },
    { id: "optimize",     label: "Optimize" },
    { id: "convert",      label: "Convert" },
    { id: "edit",         label: "Edit" },
    { id: "security",     label: "Security" },
    { id: "intelligence", label: "Intelligence" },
  ];

  let selected = $state<Cat | "all">("all");
  let query = $state("");

  function count(cat: Cat | "all"): number {
    return cat === "all" ? tools.length : tools.filter((t) => t.cat === cat).length;
  }

  const filtered = $derived(
    tools.filter((t) => {
      if (selected !== "all" && t.cat !== selected) return false;
      if (query.trim()) {
        const q = query.toLowerCase();
        if (!t.label.toLowerCase().includes(q) && !t.blurb.toLowerCase().includes(q)) return false;
      }
      return true;
    }),
  );

  const dispatch = createEventDispatcher<{ open: { id: string } }>();
  function pick(id: string) {
    dispatch("open", { id });
  }
</script>

<div class="toolbox">
  <header class="head">
    <h1>Every tool you need to work with PDFs.</h1>
    <p class="lede">
      {tools.length} tools. Local-first. Zero cloud round-trip. Merge, split, compress, convert, OCR,
      redact, sign, flatten, archive, tag for accessibility, prep for press — all in one app.
    </p>
    <div class="search">
      <input
        type="search"
        placeholder="Search tools…"
        bind:value={query}
        aria-label="Search tools"
      />
    </div>
  </header>

  <div class="chips" role="tablist" aria-label="Filter tools by category">
    {#each categories as c}
      <button
        type="button"
        class="chip"
        class:active={selected === c.id}
        data-cat={c.id}
        onclick={() => (selected = c.id)}
        role="tab"
        aria-selected={selected === c.id}
      >
        {c.label} <span class="num">{count(c.id)}</span>
      </button>
    {/each}
  </div>

  <div class="grid">
    {#each filtered as t (t.id)}
      <button class="tile" data-cat={t.cat} onclick={() => pick(t.id)}>
        <span class="ic" aria-hidden="true">
          {#if t.cat === "organize"}
            <svg viewBox="0 0 24 24"><rect x="3" y="3" width="7" height="7" rx="1" fill="none" stroke="currentColor" stroke-width="2"/><rect x="14" y="3" width="7" height="7" rx="1" fill="none" stroke="currentColor" stroke-width="2"/><rect x="3" y="14" width="7" height="7" rx="1" fill="none" stroke="currentColor" stroke-width="2"/><rect x="14" y="14" width="7" height="7" rx="1" fill="none" stroke="currentColor" stroke-width="2"/></svg>
          {:else if t.cat === "optimize"}
            <svg viewBox="0 0 24 24"><path d="M20 4L12 12M4 20l8-8M4 4l4 4M20 20l-4-4" stroke="currentColor" stroke-width="2" fill="none"/></svg>
          {:else if t.cat === "convert"}
            <svg viewBox="0 0 24 24"><path d="M5 8h12l-3-3M19 16H7l3 3" stroke="currentColor" stroke-width="2" fill="none" stroke-linecap="round"/></svg>
          {:else if t.cat === "edit"}
            <svg viewBox="0 0 24 24"><path d="M4 20l4-1L20 7l-3-3L5 16z" fill="none" stroke="currentColor" stroke-width="2"/></svg>
          {:else if t.cat === "security"}
            <svg viewBox="0 0 24 24"><rect x="5" y="11" width="14" height="9" rx="1" fill="none" stroke="currentColor" stroke-width="2"/><path d="M8 11V8a4 4 0 018 0v3" fill="none" stroke="currentColor" stroke-width="2"/></svg>
          {:else}
            <svg viewBox="0 0 24 24"><path d="M12 3a4 4 0 014 4v3h2v4h-2v3a4 4 0 11-8 0v-3H6v-4h2V7a4 4 0 014-4z" fill="none" stroke="currentColor" stroke-width="2"/></svg>
          {/if}
        </span>
        <h3>{t.label}</h3>
        <p>{t.blurb}</p>
        {#if t.badge}
          <span class="badge">{t.badge}</span>
        {/if}
      </button>
    {/each}
  </div>

  {#if filtered.length === 0}
    <p class="empty">No tools match “{query}”.</p>
  {/if}
</div>

<style>
  .toolbox {
    height: 100%; overflow: auto;
    padding: 38px 44px 60px;
    background: var(--bg, #0c0e12); color: var(--ink, #e9ecf2);
  }
  .head { max-width: 820px; margin: 0 auto 28px; text-align: center; }
  .head h1 {
    font-size: clamp(24px, 2.6vw, 34px);
    letter-spacing: -0.02em; margin: 0 0 12px;
    color: var(--ink, #e9ecf2);
  }
  .lede { color: var(--muted, #9aa3b2); font-size: 15px; line-height: 1.6; margin: 0 0 22px; }
  .search { display: flex; justify-content: center; }
  .search input {
    width: min(440px, 100%);
    padding: 10px 14px;
    background: var(--panel, #14171d);
    border: 1px solid var(--line, #232831);
    border-radius: 10px;
    color: var(--ink, #e9ecf2);
    font-size: 14px;
  }
  .search input:focus { outline: 2px solid var(--accent-2, #7dd3fc); outline-offset: -1px; }

  .chips {
    display: flex; flex-wrap: wrap; justify-content: center;
    gap: 8px; margin: 0 auto 28px; max-width: 980px;
  }
  .chip {
    background: var(--panel, #14171d);
    border: 1px solid var(--line, #232831);
    color: var(--muted, #9aa3b2);
    font: 600 13px ui-sans-serif, system-ui;
    padding: 8px 14px; border-radius: 999px;
    cursor: pointer; transition: all .15s ease;
    display: inline-flex; align-items: center; gap: 8px;
  }
  .chip .num {
    background: var(--panel-2, #1b1f26);
    color: var(--muted, #9aa3b2);
    font-size: 11px; padding: 1px 7px; border-radius: 999px; font-weight: 700;
  }
  .chip:hover { color: var(--ink, #e9ecf2); border-color: #3a3f4a; }
  .chip.active {
    background: var(--ink, #e9ecf2); color: var(--bg, #0c0e12);
    border-color: var(--ink, #e9ecf2);
  }
  .chip.active .num { background: rgba(0,0,0,.15); color: var(--bg, #0c0e12); }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: 14px;
    max-width: 1280px; margin: 0 auto;
  }
  .tile {
    text-align: left;
    background: var(--panel, #14171d);
    border: 1px solid var(--line, #232831);
    border-radius: 12px;
    padding: 16px 14px 14px;
    display: flex; flex-direction: column; gap: 6px;
    cursor: pointer;
    transition: transform .12s ease, border-color .12s ease, box-shadow .12s ease;
    color: var(--ink, #e9ecf2);
    position: relative;
    min-height: 138px;
    font-family: inherit;
  }
  .tile[data-cat="organize"]     { --c: #3b82f6; }
  .tile[data-cat="optimize"]     { --c: #14b8a6; }
  .tile[data-cat="convert"]      { --c: #22c55e; }
  .tile[data-cat="edit"]         { --c: #eab308; }
  .tile[data-cat="security"]     { --c: #ef4444; }
  .tile[data-cat="intelligence"] { --c: #a855f7; }
  .tile:hover {
    transform: translateY(-2px);
    border-color: var(--c);
    box-shadow: 0 10px 24px rgba(0,0,0,.4);
  }
  .tile:focus-visible { outline: 2px solid var(--c); outline-offset: 2px; }
  .ic {
    width: 34px; height: 34px; border-radius: 8px;
    display: inline-flex; align-items: center; justify-content: center;
    background: color-mix(in srgb, var(--c) 16%, transparent);
    color: var(--c);
    margin-bottom: 4px;
  }
  .ic :global(svg) { width: 20px; height: 20px; }
  .tile h3 { font-size: 14px; margin: 0; font-weight: 700; }
  .tile p  { font-size: 12px; margin: 0; color: var(--muted, #9aa3b2); line-height: 1.45; }
  .badge {
    font-size: 9px; font-weight: 700; letter-spacing: .05em;
    text-transform: uppercase;
    background: color-mix(in srgb, var(--c) 22%, transparent);
    color: var(--c); padding: 2px 7px; border-radius: 999px;
    position: absolute; top: 12px; right: 12px;
  }
  .empty {
    text-align: center; color: var(--muted, #9aa3b2);
    margin: 40px auto; font-size: 14px;
  }
</style>
