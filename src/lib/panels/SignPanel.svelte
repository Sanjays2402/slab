<script lang="ts">
  /**
   * Sign / Stamp
   *
   * Drop an image (signature scan, logo, "APPROVED" stamp, anything you'd
   * normally fight Acrobat to insert) and we composite it onto a chosen page
   * at the position + size you set. Everything happens client-side via
   * pdf-lib so no Rust hop is needed.
   */
  import { onMount } from "svelte";
  import { PDFDocument } from "pdf-lib";
  import { isInTauri } from "$lib/tauri";
  import { idle, type Status } from "$lib/types";

  type Corner = "top-left" | "top-right" | "bottom-left" | "bottom-right" | "center";

  let pdfName = $state<string | null>(null);
  let pdfBytes = $state<Uint8Array | null>(null);
  let pageCount = $state(0);
  let targetPage = $state(1);

  let stampName = $state<string | null>(null);
  let stampBytes = $state<Uint8Array | null>(null);
  let stampType = $state<"png" | "jpg" | null>(null);
  let stampPreviewUrl = $state<string | null>(null);

  let widthPct = $state(30);
  let opacity = $state(1.0);
  let corner = $state<Corner>("bottom-right");
  let marginPt = $state(36);

  let status = $state<Status>(idle);

  let pdfInput: HTMLInputElement;
  let stampInput: HTMLInputElement;

  onMount(() => () => {
    if (stampPreviewUrl) URL.revokeObjectURL(stampPreviewUrl);
  });

  async function pickPdf() {
    if (isInTauri()) {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const picked = await open({
        multiple: false,
        filters: [{ name: "PDF", extensions: ["pdf"] }],
      });
      if (typeof picked !== "string") return;
      const fs = await import("@tauri-apps/plugin-fs");
      const data = await fs.readFile(picked);
      await loadPdf(picked.split("/").pop() || "input.pdf", data);
    } else {
      pdfInput?.click();
    }
  }

  async function onPdfChange(e: Event) {
    const file = (e.target as HTMLInputElement).files?.[0];
    if (!file) return;
    await loadPdf(file.name, new Uint8Array(await file.arrayBuffer()));
  }

  async function loadPdf(name: string, bytes: Uint8Array) {
    status = idle;
    try {
      const doc = await PDFDocument.load(bytes, { ignoreEncryption: true });
      pdfName = name;
      pdfBytes = bytes;
      pageCount = doc.getPageCount();
      targetPage = Math.min(targetPage, pageCount) || 1;
    } catch (e) {
      status = { kind: "err", msg: `Could not open PDF: ${e}` };
    }
  }

  async function pickStamp() {
    if (isInTauri()) {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const picked = await open({
        multiple: false,
        filters: [{ name: "Image", extensions: ["png", "jpg", "jpeg"] }],
      });
      if (typeof picked !== "string") return;
      const fs = await import("@tauri-apps/plugin-fs");
      const data = await fs.readFile(picked);
      await loadStamp(picked.split("/").pop() || "stamp", data);
    } else {
      stampInput?.click();
    }
  }

  async function onStampChange(e: Event) {
    const file = (e.target as HTMLInputElement).files?.[0];
    if (!file) return;
    await loadStamp(file.name, new Uint8Array(await file.arrayBuffer()));
  }

  async function loadStamp(name: string, bytes: Uint8Array) {
    const lower = name.toLowerCase();
    if (lower.endsWith(".png")) {
      stampType = "png";
    } else if (lower.endsWith(".jpg") || lower.endsWith(".jpeg")) {
      stampType = "jpg";
    } else {
      status = { kind: "err", msg: "Use PNG or JPEG (PNG preserves transparency)." };
      return;
    }
    if (stampPreviewUrl) URL.revokeObjectURL(stampPreviewUrl);
    const blob = new Blob([bytes as unknown as ArrayBuffer], {
      type: stampType === "png" ? "image/png" : "image/jpeg",
    });
    stampPreviewUrl = URL.createObjectURL(blob);
    stampName = name;
    stampBytes = bytes;
  }

  function reset() {
    pdfName = null;
    pdfBytes = null;
    pageCount = 0;
    targetPage = 1;
    if (stampPreviewUrl) URL.revokeObjectURL(stampPreviewUrl);
    stampName = null;
    stampBytes = null;
    stampType = null;
    stampPreviewUrl = null;
    status = idle;
  }

  async function run() {
    if (!pdfBytes || !stampBytes || !stampType) {
      status = { kind: "err", msg: "Pick a PDF and a stamp image first." };
      return;
    }
    status = { kind: "working", msg: "Stamping…" };
    try {
      const pdf = await PDFDocument.load(pdfBytes, { ignoreEncryption: true });
      const embedded =
        stampType === "png" ? await pdf.embedPng(stampBytes) : await pdf.embedJpg(stampBytes);

      const idx = Math.max(0, Math.min(pageCount - 1, targetPage - 1));
      const page = pdf.getPage(idx);
      const { width, height } = page.getSize();

      const drawW = (width * widthPct) / 100;
      const drawH = (embedded.height / embedded.width) * drawW;

      const [x, y] = positionFor(corner, width, height, drawW, drawH, marginPt);
      page.drawImage(embedded, { x, y, width: drawW, height: drawH, opacity });

      const out = await pdf.save();
      const outBytes = new Uint8Array(out);

      if (isInTauri()) {
        const { save } = await import("@tauri-apps/plugin-dialog");
        const fs = await import("@tauri-apps/plugin-fs");
        const base = (pdfName ?? "output").replace(/\.pdf$/i, "");
        const target = await save({
          defaultPath: `${base}-stamped.pdf`,
          filters: [{ name: "PDF", extensions: ["pdf"] }],
        });
        if (typeof target !== "string") {
          status = idle;
          return;
        }
        await fs.writeFile(target, outBytes);
        status = { kind: "ok", msg: `Saved → ${target.split("/").pop()}` };
      } else {
        const base = (pdfName ?? "output").replace(/\.pdf$/i, "");
        const blob = new Blob([outBytes as unknown as ArrayBuffer], { type: "application/pdf" });
        const url = URL.createObjectURL(blob);
        const a = document.createElement("a");
        a.href = url;
        a.download = `${base}-stamped.pdf`;
        a.click();
        setTimeout(() => URL.revokeObjectURL(url), 1000);
        status = { kind: "ok", msg: "Downloaded stamped PDF." };
      }
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }

  function positionFor(
    c: Corner,
    pageW: number,
    pageH: number,
    w: number,
    h: number,
    m: number,
  ): [number, number] {
    switch (c) {
      case "top-left":
        return [m, pageH - h - m];
      case "top-right":
        return [pageW - w - m, pageH - h - m];
      case "bottom-left":
        return [m, m];
      case "bottom-right":
        return [pageW - w - m, m];
      case "center":
        return [(pageW - w) / 2, (pageH - h) / 2];
    }
  }
</script>

<header class="content-header">
  <h1>Sign & Stamp</h1>
  <p class="subtitle">Drop a signature, logo, or "APPROVED" stamp onto any page.</p>
</header>

<section class="panel">
  <div class="row two">
    <!-- PDF side -->
    <div class="col">
      <h3 class="col-title">PDF</h3>
      {#if !pdfName}
        <button class="dropzone small" onclick={pickPdf}>
          <span class="dz-icon">+</span>
          <span class="dz-title">Choose PDF</span>
        </button>
      {:else}
        <div class="file-card">
          <div>
            <div class="file-name">{pdfName}</div>
            <div class="file-meta">{pageCount} pages</div>
          </div>
          <button class="ghost" onclick={pickPdf}>Change</button>
        </div>
        <label class="field">
          <span class="field-label">Target page</span>
          <input type="number" min="1" max={pageCount} bind:value={targetPage} />
        </label>
      {/if}
    </div>

    <!-- Stamp side -->
    <div class="col">
      <h3 class="col-title">Stamp</h3>
      {#if !stampName}
        <button class="dropzone small" onclick={pickStamp}>
          <span class="dz-icon">+</span>
          <span class="dz-title">PNG or JPG</span>
          <span class="dz-hint">PNG keeps transparency.</span>
        </button>
      {:else}
        <div class="stamp-card">
          <img src={stampPreviewUrl} alt="stamp preview" />
          <div>
            <div class="file-name">{stampName}</div>
            <div class="file-meta">{stampType?.toUpperCase()}</div>
            <button class="ghost mini" onclick={pickStamp}>Change</button>
          </div>
        </div>
      {/if}
    </div>
  </div>

  {#if pdfName && stampName}
    <div class="row two">
      <label class="field">
        <span class="field-label">Width: {widthPct}% of page</span>
        <input type="range" min="5" max="100" step="1" bind:value={widthPct} />
      </label>
      <label class="field">
        <span class="field-label">Opacity: {opacity.toFixed(2)}</span>
        <input type="range" min="0.05" max="1" step="0.05" bind:value={opacity} />
      </label>
    </div>

    <label class="field">
      <span class="field-label">Position</span>
      <div class="positions">
        {#each [["top-left", "↖"], ["top-right", "↗"], ["center", "•"], ["bottom-left", "↙"], ["bottom-right", "↘"]] as [id, glyph] (id)}
          <button
            class="pos-btn"
            class:active={corner === id}
            onclick={() => (corner = id as Corner)}
            title={id}
          >
            {glyph}
          </button>
        {/each}
      </div>
    </label>

    <label class="field">
      <span class="field-label">Margin: {marginPt}pt</span>
      <input type="range" min="0" max="144" step="2" bind:value={marginPt} />
    </label>

    <div class="actions">
      <button class="primary" onclick={run} disabled={status.kind === "working"}>
        {status.kind === "working" ? status.msg : "Stamp PDF"}
      </button>
      <button class="ghost" onclick={reset} disabled={status.kind === "working"}>Reset</button>
    </div>
  {/if}

  {#if status.kind === "ok"}
    <div class="status ok">✓ {status.msg}</div>
  {:else if status.kind === "err"}
    <div class="status err">✕ {status.msg}</div>
  {/if}

  <!-- Browser fallback file inputs (hidden — opened via .click()). -->
  <input
    bind:this={pdfInput}
    type="file"
    accept="application/pdf"
    style="display: none"
    onchange={onPdfChange}
  />
  <input
    bind:this={stampInput}
    type="file"
    accept="image/png,image/jpeg"
    style="display: none"
    onchange={onStampChange}
  />
</section>

<style>
  .row.two {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 16px;
  }
  .col {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .col-title {
    margin: 0;
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-3);
  }
  .dropzone.small {
    padding: 24px;
  }
  .stamp-card {
    display: flex;
    gap: 12px;
    align-items: center;
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    padding: 8px;
  }
  .stamp-card img {
    width: 64px;
    height: 64px;
    object-fit: contain;
    background: repeating-conic-gradient(
        var(--bg-1) 0% 25%,
        var(--bg-3) 25% 50%
      ) 0 / 16px 16px;
    border-radius: 4px;
  }
  .mini {
    padding: 2px 8px;
    font-size: 11px;
    margin-top: 4px;
  }
  .positions {
    display: flex;
    gap: 4px;
  }
  .pos-btn {
    width: 40px;
    height: 40px;
    background: var(--bg-2);
    border: 1px solid var(--border);
    color: var(--text-2);
    border-radius: var(--r-sm);
    cursor: pointer;
    font-size: 18px;
  }
  .pos-btn:hover {
    background: var(--bg-3);
    color: var(--text-1);
  }
  .pos-btn.active {
    background: var(--accent);
    color: white;
    border-color: var(--accent);
  }
  @media (max-width: 720px) {
    .row.two {
      grid-template-columns: 1fr;
    }
  }
</style>
