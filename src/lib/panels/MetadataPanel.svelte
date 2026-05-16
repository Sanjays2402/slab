<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { idle, basename, stripExt, type CmdResult, type Status } from "$lib/types";

  type Metadata = {
    title: string | null;
    author: string | null;
    subject: string | null;
    keywords: string | null;
    creator: string | null;
    producer: string | null;
  };

  let input = $state<string | null>(null);
  let status = $state<Status>(idle);
  let loaded = $state(false);

  // Form values (always strings — null/undefined turns into "" for the inputs).
  let title = $state("");
  let author = $state("");
  let subject = $state("");
  let keywords = $state("");
  let creator = $state("");
  let producer = $state("");

  async function pickInput() {
    const picked = await open({
      multiple: false,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof picked !== "string") return;
    input = picked;
    status = idle;
    await loadMetadata(picked);
  }

  async function loadMetadata(path: string) {
    status = { kind: "working", msg: "Reading metadata…" };
    try {
      const res = await invoke<CmdResult<Metadata>>("slab_read_metadata", { input: path });
      if (res.kind === "ok") {
        const m = res.value;
        title = m.title ?? "";
        author = m.author ?? "";
        subject = m.subject ?? "";
        keywords = m.keywords ?? "";
        creator = m.creator ?? "";
        producer = m.producer ?? "";
        loaded = true;
        status = idle;
      } else {
        status = { kind: "err", msg: res.message };
      }
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }

  async function saveMetadata() {
    if (!input) return;
    const base = stripExt(basename(input));
    const output = await save({
      defaultPath: `${base}-meta.pdf`,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof output !== "string") return;

    status = { kind: "working", msg: "Writing metadata…" };
    try {
      const res = await invoke<CmdResult<null>>("slab_write_metadata", {
        input,
        output,
        meta: {
          title: title || null,
          author: author || null,
          subject: subject || null,
          keywords: keywords || null,
          creator: creator || null,
          producer: producer || null,
        },
      });
      status =
        res.kind === "ok"
          ? { kind: "ok", msg: `Saved → ${basename(output)}` }
          : { kind: "err", msg: res.message };
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }

  async function stripAll() {
    if (!input) return;
    const base = stripExt(basename(input));
    const output = await save({
      defaultPath: `${base}-anonymous.pdf`,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof output !== "string") return;

    status = { kind: "working", msg: "Stripping all metadata…" };
    try {
      const res = await invoke<CmdResult<null>>("slab_strip_metadata", { input, output });
      if (res.kind === "ok") {
        title = "";
        author = "";
        subject = "";
        keywords = "";
        creator = "";
        producer = "";
        status = { kind: "ok", msg: `Anonymized → ${basename(output)}` };
      } else {
        status = { kind: "err", msg: res.message };
      }
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }
</script>

<header class="content-header">
  <h1>Metadata</h1>
  <p class="subtitle">View, edit, or strip every identifying field. Privacy by default.</p>
</header>

<section class="panel">
  {#if !input}
    <button class="dropzone" onclick={pickInput}>
      <span class="dz-icon">+</span>
      <span class="dz-title">Choose a PDF</span>
      <span class="dz-hint">Read Title / Author / Subject / Keywords / Creator / Producer.</span>
    </button>
  {:else}
    <div class="file-card">
      <div>
        <div class="file-name">{basename(input)}</div>
        <div class="file-meta">{loaded ? "Editing metadata" : "Loading…"}</div>
      </div>
      <button class="ghost" onclick={pickInput}>Change</button>
    </div>

    {#if loaded}
      <div class="grid">
        <label class="field">
          <span class="field-label">Title</span>
          <input type="text" bind:value={title} placeholder="(empty)" />
        </label>
        <label class="field">
          <span class="field-label">Author</span>
          <input type="text" bind:value={author} placeholder="(empty)" />
        </label>
        <label class="field">
          <span class="field-label">Subject</span>
          <input type="text" bind:value={subject} placeholder="(empty)" />
        </label>
        <label class="field">
          <span class="field-label">Keywords</span>
          <input type="text" bind:value={keywords} placeholder="comma, separated" />
        </label>
        <label class="field">
          <span class="field-label">Creator</span>
          <input type="text" bind:value={creator} placeholder="App that made the PDF" />
        </label>
        <label class="field">
          <span class="field-label">Producer</span>
          <input type="text" bind:value={producer} placeholder="PDF library used" />
        </label>
      </div>

      <div class="actions">
        <button class="primary" onclick={saveMetadata} disabled={status.kind === "working"}>
          {status.kind === "working" ? status.msg : "Save metadata"}
        </button>
        <button class="ghost" onclick={stripAll} disabled={status.kind === "working"}>
          Strip all (anonymize)
        </button>
      </div>

      <div class="note">
        “Strip all” deletes Title, Author, Subject, Keywords, Creator, Producer,
        Creation/Modification dates, and any XMP stream. The file content is untouched.
      </div>
    {/if}
  {/if}

  {#if status.kind === "ok"}
    <div class="status ok">✓ {status.msg}</div>
  {:else if status.kind === "err"}
    <div class="status err">✕ {status.msg}</div>
  {/if}
</section>

<style>
  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px 16px;
  }
  .note {
    font-size: 12px;
    color: var(--text-3);
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-left: 3px solid var(--accent);
    padding: 8px 12px;
    border-radius: var(--r-sm);
  }
  @media (max-width: 720px) {
    .grid {
      grid-template-columns: 1fr;
    }
  }
</style>
