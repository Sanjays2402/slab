<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import SavedFiles from "$lib/components/SavedFiles.svelte";
  import { isInTauri } from "$lib/tauri";
  import { idle, basename, stripExt, type CmdResult, type Status } from "$lib/types";

  let input = $state<string | null>(null);
  let lang = $state("eng");
  let busy = $state(false);
  let status = $state<Status>(idle);
  let outputs = $state<string[]>([]);
  const validLanguage = $derived(/^[a-zA-Z0-9_]+(?:\+[a-zA-Z0-9_]+)*$/.test(lang.trim()));

  async function pickInput() {
    if (busy) return;
    if (!isInTauri()) { status = { kind: "err", msg: "OCR needs the Slab desktop app and local Poppler and Tesseract tools." }; return; }
    busy = true;
    try {
      const picked = await open({ multiple: false, filters: [{ name: "PDF", extensions: ["pdf"] }] });
      if (typeof picked === "string") { input = picked; outputs = []; status = idle; }
    } catch (e) { status = { kind: "err", msg: String(e) }; }
    finally { busy = false; }
  }

  async function runOcr() {
    if (!input || busy || !validLanguage) return;
    busy = true;
    try {
      const output = await save({ defaultPath: `${stripExt(basename(input))}-ocr.pdf`, filters: [{ name: "PDF", extensions: ["pdf"] }] });
      if (typeof output !== "string") return;
      const normalize = (path: string) => path.replace(/\\/g, "/").toLowerCase();
      if (normalize(input) === normalize(output)) { status = { kind: "err", msg: "Choose a new output file to keep the original scan unchanged." }; return; }
      outputs = [];
      status = { kind: "working", msg: "Recognizing text…" };
      const result = await invoke<CmdResult<{ pages: number }>>("slab_ocr", { input, output, opts: { lang: lang.trim(), dpi: 300 } });
      if (result.kind === "ok") { outputs = [output]; status = { kind: "ok", msg: `Recognized ${result.value.pages} page(s).` }; }
      else status = { kind: "err", msg: result.message };
    } catch (e) { status = { kind: "err", msg: String(e) }; }
    finally { busy = false; }
  }
</script>

<header class="content-header"><h1>OCR PDF</h1><p class="subtitle">Turn scanned pages into searchable text with local OCR. Save a new PDF and keep the original.</p></header>
<section class="panel" aria-busy={busy}>
  <button class="dropzone" onclick={pickInput} disabled={busy}><span class="dz-title">{input ? basename(input) : "Choose a scanned PDF"}</span><span class="dz-hint">{input ? "Click to change the input" : "PDF processing stays on your computer."}</span></button>
  <div class="field"><label class="field-label" for="ocr-language">OCR language</label><input id="ocr-language" bind:value={lang} disabled={busy} aria-invalid={!validLanguage} aria-describedby="ocr-language-hint"><span class="field-hint" id="ocr-language-hint">Use installed Tesseract language codes, such as eng, fra, or eng+fra.</span></div>
  <p class="field-hint">Requires Poppler (pdftoppm) and Tesseract on your PATH. Scanned pages are rasterized at 300 DPI.</p>
  <div class="actions"><button class="primary" onclick={runOcr} disabled={!input || busy || !validLanguage}>{busy ? "Working…" : "Create searchable PDF"}</button></div>
  {#if status.kind === "err"}<div class="status err" role="alert">{status.msg}</div>{:else if status.kind === "working"}<div class="status" role="status">{status.msg}</div>{:else if status.kind === "ok"}<p role="status">{status.msg}</p>{/if}
  <SavedFiles paths={outputs} />
</section>
