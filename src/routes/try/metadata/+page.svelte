<script lang="ts">
  import { onMount } from "svelte";
  import { loadSample, SAMPLES } from "$lib/try/samples";
  import { readMetadata, writeMetadata, type PdfMetadata } from "$lib/try/pdfOps";

  let bytes: Uint8Array | null = null;
  let sourceLabel = "";
  let meta: PdfMetadata = { title: "", author: "", subject: "", keywords: "" };
  let dirty = false;
  let busy = false;
  let status = "";

  async function load() {
    const params = new URLSearchParams(window.location.search);
    const slug = params.get("sample");
    const source = params.get("source");
    busy = true;
    try {
      if (source === "user") {
        const name = sessionStorage.getItem("try:user-pdf-name") ?? "your PDF";
        const b64 = sessionStorage.getItem("try:user-pdf-bytes");
        if (!b64) throw new Error("no user PDF in session");
        const bin = atob(b64);
        const arr = new Uint8Array(bin.length);
        for (let i = 0; i < bin.length; i++) arr[i] = bin.charCodeAt(i);
        bytes = arr;
        sourceLabel = name;
      } else {
        const chosen = slug && SAMPLES.find((s) => s.slug === slug)
          ? slug
          : SAMPLES[0].slug;
        bytes = await loadSample(chosen);
        sourceLabel = SAMPLES.find((s) => s.slug === chosen)!.label;
      }
      meta = await readMetadata(bytes);
      dirty = false;
    } catch (err) {
      status = `Could not load: ${(err as Error).message}`;
    } finally {
      busy = false;
    }
  }

  async function save() {
    if (!bytes) return;
    busy = true;
    status = "Writing metadata…";
    try {
      bytes = await writeMetadata(bytes, meta);
      // Push a blob download.
      const blob = new Blob([bytes], { type: "application/pdf" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = `slab-metadata-${Date.now()}.pdf`;
      document.body.appendChild(a);
      a.click();
      a.remove();
      URL.revokeObjectURL(url);
      dirty = false;
      status = "Saved to your downloads.";
    } catch (err) {
      status = `Failed: ${(err as Error).message}`;
    } finally {
      busy = false;
    }
  }

  function field(key: keyof PdfMetadata, val: string) {
    meta = { ...meta, [key]: val };
    dirty = true;
  }

  onMount(load);
</script>

<svelte:head>
  <title>Metadata — Slab in your browser</title>
</svelte:head>

<header class="hd">
  <div>
    <div class="crumb">Try Slab · Metadata</div>
    <h1>{sourceLabel || "Loading…"}</h1>
  </div>
  <button class="primary" disabled={!bytes || busy || !dirty} on:click={save}>
    Save as new PDF
  </button>
</header>

<form class="form" on:submit|preventDefault={save}>
  <label>
    <span class="lbl">Title</span>
    <input type="text" value={meta.title}
           on:input={(e) => field("title", e.currentTarget.value)} />
  </label>
  <label>
    <span class="lbl">Author</span>
    <input type="text" value={meta.author}
           on:input={(e) => field("author", e.currentTarget.value)} />
  </label>
  <label>
    <span class="lbl">Subject</span>
    <input type="text" value={meta.subject}
           on:input={(e) => field("subject", e.currentTarget.value)} />
  </label>
  <label>
    <span class="lbl">Keywords <span class="dim">(comma-separated)</span></span>
    <input type="text" value={meta.keywords}
           on:input={(e) => field("keywords", e.currentTarget.value)} />
  </label>
  <p class="status">{status}</p>
</form>

<style>
  .hd {
    display: flex;
    justify-content: space-between;
    align-items: flex-end;
    margin-bottom: 24px;
    gap: 16px;
  }
  .crumb {
    font-size: 12px;
    color: rgba(243, 243, 245, 0.55);
    letter-spacing: 0.06em;
    text-transform: uppercase;
    margin-bottom: 4px;
  }
  .hd h1 { margin: 0; font-size: 28px; letter-spacing: -0.02em; }

  .primary {
    padding: 10px 18px;
    border-radius: 10px;
    background: linear-gradient(135deg, #ffbf00, #ff8b00);
    color: #1a1a1a;
    border: 0;
    font-weight: 600;
    cursor: pointer;
    font-size: 14px;
  }
  .primary:disabled { opacity: 0.4; cursor: not-allowed; }

  .form {
    max-width: 560px;
    display: grid;
    gap: 14px;
  }
  label { display: grid; gap: 6px; }
  .lbl {
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: rgba(243, 243, 245, 0.6);
    font-weight: 600;
  }
  .dim { color: rgba(243, 243, 245, 0.35); text-transform: none; font-weight: 400; }
  input {
    padding: 10px 12px;
    background: rgba(255, 255, 255, 0.05);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 8px;
    color: #f3f3f5;
    font-size: 14px;
    font-family: inherit;
  }
  input:focus {
    outline: none;
    border-color: rgba(255, 191, 0, 0.6);
    box-shadow: 0 0 0 3px rgba(255, 191, 0, 0.15);
  }
  .status { font-size: 12px; color: rgba(243, 243, 245, 0.55); margin: 0; }
</style>
