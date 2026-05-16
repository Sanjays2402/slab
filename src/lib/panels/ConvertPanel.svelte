<script lang="ts">
  // Convert panel — two flows:
  //   1) PDF → Images: render every (or selected) page of a PDF to PNG/JPEG/WebP
  //      at a chosen DPI, then bundle into a ZIP that we save via the native
  //      file-save dialog.
  //   2) Images → PDF: pick any number of PNG/JPG/WebP images and build a PDF
  //      where each image becomes one page (fit-to-page or original-size).
  //
  // Both flows are 100% client-side. PDF.js renders pages onto a <canvas>;
  // pdf-lib assembles the output PDF; jszip bundles images. We only call out
  // to Tauri for the file dialogs and the final binary write.

  import { open, save } from "@tauri-apps/plugin-dialog";
  import { writeFile } from "@tauri-apps/plugin-fs";
  import { onMount } from "svelte";
  import JSZip from "jszip";
  import { PDFDocument } from "pdf-lib";
  import { idle, basename, stripExt, type Status } from "$lib/types";
  import { isInTauri } from "$lib/tauri";

  // ---- PDF.js (only the lib API, no viewer chrome needed here) ----
  type PdfjsModule = typeof import("pdfjs-dist");
  let pdfjsLib: PdfjsModule | null = null;
  let pdfjsReady = $state(false);

  onMount(async () => {
    pdfjsLib = await import("pdfjs-dist");
    const workerUrl = (await import("pdfjs-dist/build/pdf.worker.min.mjs?url"))
      .default;
    pdfjsLib.GlobalWorkerOptions.workerSrc = workerUrl;
    pdfjsReady = true;
  });

  // ---- Tab state ----
  type Mode = "pdf2img" | "img2pdf";
  let mode = $state<Mode>("pdf2img");

  // ---- Shared status / progress ----
  let status = $state<Status>(idle);
  let progress = $state<{ done: number; total: number } | null>(null);

  // ---- PDF → Images state ----
  let pdfInput = $state<string | null>(null);
  let pdfBytes = $state<Uint8Array | null>(null);
  let pdfPageCount = $state(0);
  let imgFormat = $state<"png" | "jpeg" | "webp">("png");
  let imgDpi = $state(150);
  let imgRangeMode = $state<"all" | "range">("all");
  let imgRangeText = $state("");

  // ---- Images → PDF state ----
  type ImgFile = { name: string; path: string | null; bytes: Uint8Array; type: string };
  let images = $state<ImgFile[]>([]);
  let pdfSizing = $state<"fit" | "original">("fit");
  let pdfPageSize = $state<"letter" | "a4" | "auto">("letter");

  // ============================================================
  // PDF → Images
  // ============================================================

  async function pickPdf() {
    if (isInTauri()) {
      const picked = await open({
        multiple: false,
        filters: [{ name: "PDF", extensions: ["pdf"] }],
      });
      if (typeof picked !== "string") return;
      pdfInput = picked;
      // Read raw bytes for pdf.js
      const { readFile } = await import("@tauri-apps/plugin-fs");
      pdfBytes = await readFile(picked);
    } else {
      // Browser fallback
      const inp = document.createElement("input");
      inp.type = "file";
      inp.accept = "application/pdf";
      const file: File = await new Promise((resolve, reject) => {
        inp.onchange = () => (inp.files?.[0] ? resolve(inp.files[0]) : reject("no file"));
        inp.click();
      });
      pdfInput = file.name;
      pdfBytes = new Uint8Array(await file.arrayBuffer());
    }
    status = idle;
    progress = null;
    // Peek page count
    if (pdfjsLib && pdfBytes) {
      try {
        const task = pdfjsLib.getDocument({ data: pdfBytes.slice() });
        const doc = await task.promise;
        pdfPageCount = doc.numPages;
        await doc.destroy();
      } catch (e) {
        status = { kind: "err", msg: `Couldn't open PDF: ${e}` };
      }
    }
  }

  function parseRange(text: string, total: number): number[] {
    // "1-3, 7, 9-12" → [1,2,3,7,9,10,11,12]; clamp + dedupe + sort.
    const out = new Set<number>();
    for (const part of text.split(",").map((s) => s.trim()).filter(Boolean)) {
      if (part.includes("-")) {
        const [a, b] = part.split("-").map((s) => parseInt(s.trim(), 10));
        if (Number.isFinite(a) && Number.isFinite(b)) {
          const lo = Math.max(1, Math.min(a, b));
          const hi = Math.min(total, Math.max(a, b));
          for (let n = lo; n <= hi; n++) out.add(n);
        }
      } else {
        const n = parseInt(part, 10);
        if (Number.isFinite(n) && n >= 1 && n <= total) out.add(n);
      }
    }
    return [...out].sort((a, b) => a - b);
  }

  async function runPdfToImages() {
    if (!pdfBytes || !pdfInput || !pdfjsLib) {
      status = { kind: "err", msg: "Pick a PDF first." };
      return;
    }
    const pages =
      imgRangeMode === "all"
        ? Array.from({ length: pdfPageCount }, (_, i) => i + 1)
        : parseRange(imgRangeText, pdfPageCount);
    if (pages.length === 0) {
      status = { kind: "err", msg: "No pages match the range." };
      return;
    }

    // pdf.js uses scale where 1.0 = 72 DPI.
    const scale = imgDpi / 72;
    const mime =
      imgFormat === "png"
        ? "image/png"
        : imgFormat === "jpeg"
          ? "image/jpeg"
          : "image/webp";
    const ext = imgFormat === "jpeg" ? "jpg" : imgFormat;
    const quality = imgFormat === "png" ? undefined : 0.92;

    status = { kind: "working", msg: "Rendering pages…" };
    progress = { done: 0, total: pages.length };

    // We rely on pdf.js types but keep these loose so we can null them out in
    // the finally{} block without TS narrowing them to `null`.
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    let doc: any = null;
    try {
      const task = pdfjsLib.getDocument({ data: pdfBytes.slice() });
      doc = await task.promise;
      const zip = new JSZip();
      const stem = stripExt(basename(pdfInput));
      const pad = String(pages[pages.length - 1]).length;

      for (let i = 0; i < pages.length; i++) {
        const n = pages[i];
        const page = await doc.getPage(n);
        const viewport = page.getViewport({ scale });
        const canvas = document.createElement("canvas");
        canvas.width = Math.ceil(viewport.width);
        canvas.height = Math.ceil(viewport.height);
        const ctx = canvas.getContext("2d");
        if (!ctx) throw new Error("Couldn't get 2D context");
        // White background for JPEG (which has no alpha)
        if (imgFormat === "jpeg") {
          ctx.fillStyle = "#ffffff";
          ctx.fillRect(0, 0, canvas.width, canvas.height);
        }
        await page.render({ canvasContext: ctx, viewport, canvas }).promise;
        const blob: Blob = await new Promise((resolve, reject) =>
          canvas.toBlob(
            (b) => (b ? resolve(b) : reject(new Error("toBlob failed"))),
            mime,
            quality,
          ),
        );
        const buf = new Uint8Array(await blob.arrayBuffer());
        const fname = `${stem}-page-${String(n).padStart(pad, "0")}.${ext}`;
        zip.file(fname, buf);
        page.cleanup();
        progress = { done: i + 1, total: pages.length };
        // Yield so the UI repaints
        await new Promise((r) => setTimeout(r, 0));
      }

      status = { kind: "working", msg: "Packing zip…" };
      const zipBlob = await zip.generateAsync({ type: "uint8array" });
      const defaultName = `${stem}-images.zip`;

      if (isInTauri()) {
        const output = await save({
          defaultPath: defaultName,
          filters: [{ name: "ZIP archive", extensions: ["zip"] }],
        });
        if (typeof output !== "string") {
          status = idle;
          progress = null;
          return;
        }
        await writeFile(output, zipBlob);
        status = {
          kind: "ok",
          msg: `Wrote ${pages.length} image(s) → ${basename(output)}`,
        };
      } else {
        // Browser: trigger download
        downloadBlob(new Blob([new Uint8Array(zipBlob)], { type: "application/zip" }), defaultName);
        status = {
          kind: "ok",
          msg: `Downloaded ${pages.length} image(s) as ${defaultName}`,
        };
      }
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    } finally {
      progress = null;
      if (doc) await doc.destroy();
    }
  }

  // ============================================================
  // Images → PDF
  // ============================================================

  async function pickImages() {
    if (isInTauri()) {
      const picked = await open({
        multiple: true,
        filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "webp"] }],
      });
      if (!picked) return;
      const arr = Array.isArray(picked) ? picked : [picked];
      const { readFile } = await import("@tauri-apps/plugin-fs");
      const next: ImgFile[] = [];
      for (const p of arr) {
        const bytes = await readFile(p);
        next.push({
          name: basename(p),
          path: p,
          bytes,
          type: mimeFromName(basename(p)),
        });
      }
      images = [...images, ...next];
    } else {
      const inp = document.createElement("input");
      inp.type = "file";
      inp.multiple = true;
      inp.accept = "image/png,image/jpeg,image/webp";
      const files: File[] = await new Promise((resolve) => {
        inp.onchange = () => resolve(inp.files ? Array.from(inp.files) : []);
        inp.click();
      });
      const next: ImgFile[] = [];
      for (const f of files) {
        next.push({
          name: f.name,
          path: null,
          bytes: new Uint8Array(await f.arrayBuffer()),
          type: f.type || mimeFromName(f.name),
        });
      }
      images = [...images, ...next];
    }
    status = idle;
  }

  function mimeFromName(name: string): string {
    const n = name.toLowerCase();
    if (n.endsWith(".png")) return "image/png";
    if (n.endsWith(".jpg") || n.endsWith(".jpeg")) return "image/jpeg";
    if (n.endsWith(".webp")) return "image/webp";
    return "application/octet-stream";
  }

  function removeImg(i: number) {
    images = images.filter((_, idx) => idx !== i);
  }

  function moveImgUp(i: number) {
    if (i === 0) return;
    const next = [...images];
    [next[i - 1], next[i]] = [next[i], next[i - 1]];
    images = next;
  }

  function moveImgDown(i: number) {
    if (i === images.length - 1) return;
    const next = [...images];
    [next[i + 1], next[i]] = [next[i], next[i + 1]];
    images = next;
  }

  async function decodeWebpToPng(bytes: Uint8Array): Promise<Uint8Array> {
    // pdf-lib only embeds PNG / JPEG. Re-encode WebP through a canvas.
    const url = URL.createObjectURL(new Blob([new Uint8Array(bytes)], { type: "image/webp" }));
    try {
      const img = new Image();
      await new Promise<void>((resolve, reject) => {
        img.onload = () => resolve();
        img.onerror = () => reject(new Error("Couldn't decode image"));
        img.src = url;
      });
      const canvas = document.createElement("canvas");
      canvas.width = img.naturalWidth;
      canvas.height = img.naturalHeight;
      const ctx = canvas.getContext("2d");
      if (!ctx) throw new Error("Couldn't get 2D context");
      ctx.drawImage(img, 0, 0);
      const blob: Blob = await new Promise((resolve, reject) =>
        canvas.toBlob((b) => (b ? resolve(b) : reject(new Error("toBlob failed"))), "image/png"),
      );
      return new Uint8Array(await blob.arrayBuffer());
    } finally {
      URL.revokeObjectURL(url);
    }
  }

  async function runImagesToPdf() {
    if (images.length === 0) {
      status = { kind: "err", msg: "Add at least one image." };
      return;
    }
    status = { kind: "working", msg: "Building PDF…" };
    progress = { done: 0, total: images.length };
    try {
      const pdf = await PDFDocument.create();
      // Letter = 612×792 pt, A4 = 595.28×841.89 pt
      const targetSize: [number, number] | null =
        pdfPageSize === "letter" ? [612, 792] : pdfPageSize === "a4" ? [595.28, 841.89] : null;

      for (let i = 0; i < images.length; i++) {
        const img = images[i];
        let bytes = img.bytes;
        let type = img.type;
        if (type === "image/webp") {
          bytes = await decodeWebpToPng(bytes);
          type = "image/png";
        }

        let embedded;
        if (type === "image/png") {
          embedded = await pdf.embedPng(bytes);
        } else if (type === "image/jpeg") {
          embedded = await pdf.embedJpg(bytes);
        } else {
          throw new Error(`Unsupported image type: ${type || "(unknown)"} for ${img.name}`);
        }

        let pageW: number;
        let pageH: number;
        let drawW: number;
        let drawH: number;
        let dx: number;
        let dy: number;

        if (pdfSizing === "original" || pdfPageSize === "auto") {
          // Page matches image dimensions exactly (at 72 DPI assumption).
          pageW = embedded.width;
          pageH = embedded.height;
          drawW = embedded.width;
          drawH = embedded.height;
          dx = 0;
          dy = 0;
        } else {
          // Fit-to-page with margin
          const [tW, tH] = targetSize ?? [612, 792];
          const margin = 36; // 0.5"
          const usableW = tW - 2 * margin;
          const usableH = tH - 2 * margin;
          const scale = Math.min(usableW / embedded.width, usableH / embedded.height);
          pageW = tW;
          pageH = tH;
          drawW = embedded.width * scale;
          drawH = embedded.height * scale;
          dx = (tW - drawW) / 2;
          dy = (tH - drawH) / 2;
        }

        const page = pdf.addPage([pageW, pageH]);
        page.drawImage(embedded, { x: dx, y: dy, width: drawW, height: drawH });
        progress = { done: i + 1, total: images.length };
        await new Promise((r) => setTimeout(r, 0));
      }

      const outBytes = await pdf.save();
      const defaultName = images[0]
        ? `${stripExt(images[0].name)}-and-${images.length - 1}-more.pdf`
        : "images.pdf";
      const cleanName = images.length === 1 ? `${stripExt(images[0].name)}.pdf` : defaultName;

      if (isInTauri()) {
        const output = await save({
          defaultPath: cleanName,
          filters: [{ name: "PDF", extensions: ["pdf"] }],
        });
        if (typeof output !== "string") {
          status = idle;
          progress = null;
          return;
        }
        await writeFile(output, outBytes);
        status = {
          kind: "ok",
          msg: `Built ${images.length}-page PDF → ${basename(output)}`,
        };
      } else {
        downloadBlob(new Blob([new Uint8Array(outBytes)], { type: "application/pdf" }), cleanName);
        status = {
          kind: "ok",
          msg: `Downloaded ${images.length}-page PDF as ${cleanName}`,
        };
      }
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    } finally {
      progress = null;
    }
  }

  function downloadBlob(blob: Blob, name: string) {
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = name;
    document.body.appendChild(a);
    a.click();
    a.remove();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  }
</script>

<header class="content-header">
  <h1>Convert</h1>
  <p class="subtitle">
    Turn PDF pages into image files, or stitch images into a PDF. All offline, on your machine.
  </p>
</header>

<div class="mode-tabs">
  <button
    class="mode-tab"
    class:active={mode === "pdf2img"}
    onclick={() => (mode = "pdf2img")}
  >
    <span class="mt-icon">▥ → ▤</span>
    <span class="mt-label">PDF → Images</span>
  </button>
  <button
    class="mode-tab"
    class:active={mode === "img2pdf"}
    onclick={() => (mode = "img2pdf")}
  >
    <span class="mt-icon">▤ → ▥</span>
    <span class="mt-label">Images → PDF</span>
  </button>
</div>

<section class="panel">
  {#if mode === "pdf2img"}
    {#if !pdfInput}
      <button class="dropzone" onclick={pickPdf} disabled={!pdfjsReady}>
        <span class="dz-icon">+</span>
        <span class="dz-title">{pdfjsReady ? "Choose a PDF" : "Loading…"}</span>
        <span class="dz-hint">Each page becomes its own image file. Bundled as a ZIP.</span>
      </button>
    {:else}
      <div class="file-card">
        <div>
          <div class="file-name">{basename(pdfInput)}</div>
          <div class="file-meta">
            {pdfPageCount} page{pdfPageCount === 1 ? "" : "s"}
          </div>
        </div>
        <button class="ghost" onclick={pickPdf}>Change</button>
      </div>

      <div class="opt-grid">
        <div class="opt-row">
          <span class="opt-label">Format</span>
          <div class="seg">
            <button class:active={imgFormat === "png"} onclick={() => (imgFormat = "png")}>PNG</button>
            <button class:active={imgFormat === "jpeg"} onclick={() => (imgFormat = "jpeg")}>JPEG</button>
            <button class:active={imgFormat === "webp"} onclick={() => (imgFormat = "webp")}>WebP</button>
          </div>
        </div>

        <div class="opt-row">
          <span class="opt-label">DPI</span>
          <div class="seg">
            <button class:active={imgDpi === 72} onclick={() => (imgDpi = 72)}>72</button>
            <button class:active={imgDpi === 150} onclick={() => (imgDpi = 150)}>150</button>
            <button class:active={imgDpi === 300} onclick={() => (imgDpi = 300)}>300</button>
            <button class:active={imgDpi === 600} onclick={() => (imgDpi = 600)}>600</button>
          </div>
        </div>

        <div class="opt-row">
          <span class="opt-label">Pages</span>
          <div class="seg">
            <button class:active={imgRangeMode === "all"} onclick={() => (imgRangeMode = "all")}>All</button>
            <button class:active={imgRangeMode === "range"} onclick={() => (imgRangeMode = "range")}>Range</button>
          </div>
          {#if imgRangeMode === "range"}
            <input
              class="opt-input"
              placeholder="e.g. 1-3, 7, 9-12"
              bind:value={imgRangeText}
            />
          {/if}
        </div>
      </div>

      <div class="actions">
        <button
          class="primary"
          onclick={runPdfToImages}
          disabled={status.kind === "working"}
        >
          {status.kind === "working"
            ? "Working…"
            : imgRangeMode === "all"
              ? `Export ${pdfPageCount} page${pdfPageCount === 1 ? "" : "s"}`
              : "Export selected pages"}
        </button>
      </div>
    {/if}
  {:else}
    {#if images.length === 0}
      <button class="dropzone" onclick={pickImages}>
        <span class="dz-icon">+</span>
        <span class="dz-title">Choose images</span>
        <span class="dz-hint">PNG, JPEG, WebP. Drag to reorder once added.</span>
      </button>
    {:else}
      <ul class="file-list">
        {#each images as img, i (img.name + i)}
          <li class="file-row">
            <span class="row-handle" aria-hidden="true">⋮⋮</span>
            <span class="row-idx">{i + 1}</span>
            <span class="row-name" title={img.path ?? img.name}>{img.name}</span>
            <span class="row-meta">{(img.bytes.length / 1024).toFixed(1)} KB</span>
            <div class="row-actions">
              <button class="ghost" onclick={() => moveImgUp(i)} aria-label="Up">↑</button>
              <button class="ghost" onclick={() => moveImgDown(i)} aria-label="Down">↓</button>
              <button class="ghost remove" onclick={() => removeImg(i)} aria-label="Remove">✕</button>
            </div>
          </li>
        {/each}
      </ul>

      <div class="opt-grid">
        <div class="opt-row">
          <span class="opt-label">Page size</span>
          <div class="seg">
            <button class:active={pdfPageSize === "letter"} onclick={() => (pdfPageSize = "letter")}>Letter</button>
            <button class:active={pdfPageSize === "a4"} onclick={() => (pdfPageSize = "a4")}>A4</button>
            <button class:active={pdfPageSize === "auto"} onclick={() => (pdfPageSize = "auto")}>Match image</button>
          </div>
        </div>

        {#if pdfPageSize !== "auto"}
          <div class="opt-row">
            <span class="opt-label">Sizing</span>
            <div class="seg">
              <button class:active={pdfSizing === "fit"} onclick={() => (pdfSizing = "fit")}>Fit to page</button>
              <button class:active={pdfSizing === "original"} onclick={() => (pdfSizing = "original")}>Original</button>
            </div>
          </div>
        {/if}
      </div>

      <div class="actions">
        <button onclick={pickImages}>+ Add more</button>
        <button
          class="primary"
          onclick={runImagesToPdf}
          disabled={status.kind === "working"}
        >
          {status.kind === "working"
            ? "Building…"
            : `Build ${images.length}-page PDF`}
        </button>
      </div>
    {/if}
  {/if}

  {#if progress}
    <div class="progress">
      <div class="progress-bar">
        <div class="progress-fill" style="width: {(progress.done / progress.total) * 100}%"></div>
      </div>
      <span class="progress-text">{progress.done} / {progress.total}</span>
    </div>
  {/if}

  {#if status.kind === "ok"}
    <div class="status ok">✓ {status.msg}</div>
  {:else if status.kind === "err"}
    <div class="status err">✕ {status.msg}</div>
  {:else if status.kind === "working" && !progress}
    <div class="status working">⋯ {status.msg}</div>
  {/if}
</section>

<style>
  .mode-tabs {
    display: flex;
    gap: 4px;
    margin-bottom: 14px;
    padding: 4px;
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    width: fit-content;
  }
  .mode-tab {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 12px;
    background: transparent;
    border: 1px solid transparent;
    color: var(--text-2);
    border-radius: var(--r-sm);
    font-size: 13px;
    cursor: pointer;
  }
  .mode-tab:hover { color: var(--text); }
  .mode-tab.active {
    background: var(--bg-3);
    color: var(--text);
    border-color: var(--border);
  }
  .mt-icon { color: var(--accent); font-size: 11px; letter-spacing: 1px; }

  .opt-grid {
    display: flex;
    flex-direction: column;
    gap: 12px;
    margin: 14px 0;
    padding: 14px;
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
  }
  .opt-row {
    display: flex;
    align-items: center;
    gap: 14px;
    flex-wrap: wrap;
  }
  .opt-label {
    font-size: 11px;
    text-transform: uppercase;
    color: var(--text-3);
    letter-spacing: 0.5px;
    width: 80px;
    flex-shrink: 0;
  }
  .seg {
    display: inline-flex;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    padding: 2px;
  }
  .seg button {
    background: transparent;
    border: 1px solid transparent;
    color: var(--text-2);
    padding: 4px 10px;
    border-radius: 4px;
    font-size: 12px;
    cursor: pointer;
  }
  .seg button:hover:not(.active) { color: var(--text); }
  .seg button.active {
    background: var(--bg-3);
    color: var(--text);
    border-color: var(--border);
  }
  .opt-input {
    flex: 1;
    background: var(--bg);
    border: 1px solid var(--border);
    color: var(--text);
    padding: 6px 10px;
    border-radius: var(--r-sm);
    font-size: 12px;
    font-family: var(--font-mono);
  }
  .opt-input:focus {
    outline: none;
    border-color: var(--accent);
  }

  .file-list {
    list-style: none;
    padding: 0;
    margin: 0 0 14px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    max-height: 280px;
    overflow-y: auto;
  }
  .file-row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 12px;
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
  }
  .row-handle { color: var(--text-3); font-size: 11px; user-select: none; }
  .row-idx {
    width: 22px;
    height: 22px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-radius: 50%;
    background: var(--bg-3);
    color: var(--text-2);
    font-size: 11px;
    font-weight: 600;
  }
  .row-name {
    flex: 1;
    font-size: 13px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .row-meta {
    font-size: 11px;
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
  }
  .row-actions { display: flex; gap: 4px; }
  .row-actions button {
    padding: 4px 8px;
    font-size: 12px;
    border-radius: 6px;
  }
  .row-actions .remove:hover { color: var(--danger); }

  .progress {
    margin-top: 10px;
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .progress-bar {
    flex: 1;
    height: 6px;
    background: var(--bg-2);
    border-radius: 3px;
    overflow: hidden;
  }
  .progress-fill {
    height: 100%;
    background: var(--accent);
    transition: width 80ms ease-out;
  }
  .progress-text {
    font-size: 11px;
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
    min-width: 60px;
    text-align: right;
  }

  .status.working {
    background: rgba(245, 158, 11, 0.08);
    border-color: rgba(245, 158, 11, 0.3);
    color: var(--text);
  }
</style>
