<script lang="ts">
  // Beacon Study panel — turn the current PDF into a deck of Q&A
  // flashcards and drive a spaced-repetition review session.
  //
  // Workflow:
  //   1. Pick (or inherit from slab:open-recent) a PDF.
  //   2. "Generate deck" → slab_beacon_generate_deck. New cards land in
  //      ~/.slab/study.sqlite, dedupe-on-conflict.
  //   3. "Start review" → slab_beacon_study_due fetches due cards.
  //   4. Render one card. Click "Reveal" to flip. Pick ease →
  //      slab_beacon_study_review records + advances.
  //   5. Footer: stats from slab_beacon_study_stats.
  //
  // Errors map through the same friendly toast pattern used by
  // BeaconChatPanel / BeaconCitationsPanel.

  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { onMount } from "svelte";
  import { basename, idle, type CmdResult, type Status } from "$lib/types";

  type Flashcard = { page: number; q: string; a: string };
  type DeckReport = {
    cards: Flashcard[];
    model: string;
    chunks_processed: number;
    dropped: number;
  };
  type StoredCard = {
    id: number;
    pdf_hash: string;
    page: number;
    q: string;
    a: string;
    ease_factor: number;
    interval_days: number;
    due_at: number;
    last_seen_at: number;
  };
  type StudyStats = {
    total_cards: number;
    due_now: number;
    reviewed_last_24h: number;
  };
  type Ease = "again" | "hard" | "good" | "easy";

  let pdfPath = $state<string | null>(null);
  let queue = $state<StoredCard[]>([]);
  let current = $state<StoredCard | null>(null);
  let revealed = $state(false);
  let stats = $state<StudyStats | null>(null);
  let status = $state<Status>(idle);
  let cardsPerChunk = $state(3);

  onMount(() => {
    const onOpenRecent = (e: Event) => {
      const d = (e as CustomEvent).detail as { path: string } | undefined;
      if (d?.path) {
        pdfPath = d.path;
        queue = [];
        current = null;
        revealed = false;
      }
    };
    window.addEventListener("slab:open-recent", onOpenRecent);
    void refreshStats();
    return () => window.removeEventListener("slab:open-recent", onOpenRecent);
  });

  async function pickPdf() {
    const picked = await open({
      multiple: false,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof picked !== "string") return;
    pdfPath = picked;
    queue = [];
    current = null;
    revealed = false;
    status = idle;
    await refreshStats();
  }

  async function refreshStats() {
    try {
      const res = await invoke<CmdResult<StudyStats>>("slab_beacon_study_stats", {
        pdfPath,
      });
      if (res.kind === "ok") stats = res.value;
    } catch {
      /* swallow — stats are non-essential */
    }
  }

  async function generate() {
    if (!pdfPath) {
      status = { kind: "err", msg: "Pick a PDF first." };
      return;
    }
    status = { kind: "working", msg: "Generating flashcards…" };
    try {
      const res = await invoke<CmdResult<DeckReport>>("slab_beacon_generate_deck", {
        pdfPath,
        opts: { cards_per_chunk: cardsPerChunk, max_cards: 200 },
      });
      if (res.kind === "ok") {
        const r = res.value;
        status = {
          kind: "ok",
          msg: `Generated ${r.cards.length} new cards (${r.dropped} dropped) from ${r.chunks_processed} chunks · model ${r.model || "(local)"}`,
        };
        await refreshStats();
      } else {
        status = { kind: "err", msg: res.message };
      }
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }

  async function startReview() {
    status = { kind: "working", msg: "Loading due cards…" };
    try {
      const res = await invoke<CmdResult<StoredCard[]>>("slab_beacon_study_due", {
        pdfPath,
        limit: 50,
      });
      if (res.kind === "ok") {
        queue = res.value;
        current = queue.shift() ?? null;
        revealed = false;
        status = current
          ? { kind: "ok", msg: `${queue.length + 1} card(s) queued.` }
          : { kind: "ok", msg: "Nothing due right now — come back later 🎉" };
      } else {
        status = { kind: "err", msg: res.message };
      }
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }

  async function rate(ease: Ease) {
    if (!current) return;
    try {
      await invoke<CmdResult<StoredCard>>("slab_beacon_study_review", {
        cardId: current.id,
        ease,
      });
      current = queue.shift() ?? null;
      revealed = false;
      await refreshStats();
      if (!current) {
        status = { kind: "ok", msg: "Session complete 🎓" };
      }
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }

  function jumpToPage() {
    if (!current) return;
    window.dispatchEvent(
      new CustomEvent("slab:beacon-goto-page", {
        detail: { page: current.page, path: pdfPath },
      }),
    );
  }
</script>

<section class="panel">
  <header>
    <h2>🎓 Study Mode</h2>
    <p class="subtitle">
      Beacon turns your PDF into Q&A flashcards. Spaced-repetition
      scheduling (SM-2 lite) decides what to show you next.
    </p>
  </header>

  <div class="picker">
    <button class="btn" onclick={pickPdf}>Pick PDF…</button>
    <span class="path">{pdfPath ? basename(pdfPath) : "no PDF selected"}</span>
  </div>

  <div class="row">
    <label class="cpc">
      Cards / chunk:
      <input
        type="number"
        min="1"
        max="10"
        bind:value={cardsPerChunk}
      />
    </label>
    <button class="btn primary" onclick={generate} disabled={!pdfPath}>
      Generate deck
    </button>
    <button class="btn" onclick={startReview}>Start review</button>
  </div>

  {#if status.kind !== "idle"}
    <p class="status {status.kind}">{status.msg}</p>
  {/if}

  {#if current}
    <article class="card" class:revealed>
      <header class="card-head">
        <span class="page">page {current.page}</span>
        <button class="link" onclick={jumpToPage}>jump →</button>
      </header>
      <p class="q">{current.q}</p>
      {#if revealed}
        <hr />
        <p class="a">{current.a}</p>
        <div class="ease">
          <button class="ease-btn again" onclick={() => rate("again")}>Again</button>
          <button class="ease-btn hard" onclick={() => rate("hard")}>Hard</button>
          <button class="ease-btn good" onclick={() => rate("good")}>Good</button>
          <button class="ease-btn easy" onclick={() => rate("easy")}>Easy</button>
        </div>
      {:else}
        <button class="btn primary wide" onclick={() => (revealed = true)}>
          Reveal answer
        </button>
      {/if}
    </article>
  {/if}

  {#if stats}
    <footer class="stats">
      <span>{stats.total_cards} cards</span>
      <span>·</span>
      <span>{stats.due_now} due</span>
      <span>·</span>
      <span>{stats.reviewed_last_24h} reviewed today</span>
    </footer>
  {/if}
</section>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    padding: 1rem 1.25rem;
    max-width: 720px;
    font-family: var(--font-ui, system-ui);
  }
  header h2 {
    font-size: 1.1rem;
    margin: 0;
  }
  .subtitle {
    color: var(--text-muted, #888);
    font-size: 0.85rem;
    margin: 0.25rem 0 0;
  }
  .picker,
  .row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex-wrap: wrap;
  }
  .path {
    color: var(--text-muted, #888);
    font-size: 0.85rem;
    font-family: var(--font-mono, ui-monospace);
  }
  .cpc {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    font-size: 0.85rem;
  }
  .cpc input {
    width: 3.5rem;
  }
  .btn {
    padding: 0.4rem 0.8rem;
    border-radius: 6px;
    border: 1px solid var(--border, #ccc);
    background: var(--bg-elev, #fff);
    cursor: pointer;
    font: inherit;
  }
  .btn.primary {
    background: var(--accent, #4a8df0);
    color: #fff;
    border-color: transparent;
  }
  .btn.primary:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  .btn.wide {
    width: 100%;
    margin-top: 0.75rem;
  }
  .status {
    font-size: 0.85rem;
    padding: 0.4rem 0.6rem;
    border-radius: 4px;
  }
  .status.working {
    background: var(--info-bg, #e8f1ff);
    color: var(--info-fg, #1d3a8a);
  }
  .status.ok {
    background: var(--ok-bg, #e6f6ec);
    color: var(--ok-fg, #186a3b);
  }
  .status.err {
    background: var(--err-bg, #fde8e8);
    color: var(--err-fg, #a8160f);
  }
  .card {
    border: 1px solid var(--border, #ddd);
    border-radius: 10px;
    padding: 1.25rem;
    background: var(--bg-elev, #fff);
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.05);
  }
  .card-head {
    display: flex;
    justify-content: space-between;
    font-size: 0.75rem;
    color: var(--text-muted, #888);
    margin-bottom: 0.5rem;
  }
  .link {
    border: 0;
    background: none;
    color: var(--accent, #4a8df0);
    cursor: pointer;
    font: inherit;
  }
  .q {
    font-size: 1.05rem;
    margin: 0.5rem 0;
  }
  .a {
    font-size: 0.95rem;
    margin: 0.5rem 0;
    color: var(--text, #222);
  }
  hr {
    border: 0;
    border-top: 1px dashed var(--border, #ddd);
    margin: 0.75rem 0;
  }
  .ease {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 0.4rem;
    margin-top: 0.5rem;
  }
  .ease-btn {
    padding: 0.55rem 0.4rem;
    border-radius: 6px;
    border: 1px solid var(--border, #ccc);
    background: var(--bg-elev, #fff);
    cursor: pointer;
    font: inherit;
    font-size: 0.85rem;
  }
  .ease-btn.again {
    background: #fde8e8;
    color: #a8160f;
  }
  .ease-btn.hard {
    background: #fef3c7;
    color: #8a5a00;
  }
  .ease-btn.good {
    background: #e6f6ec;
    color: #186a3b;
  }
  .ease-btn.easy {
    background: #e8f1ff;
    color: #1d3a8a;
  }
  .stats {
    display: flex;
    gap: 0.4rem;
    font-size: 0.8rem;
    color: var(--text-muted, #888);
    border-top: 1px solid var(--border, #eee);
    padding-top: 0.5rem;
  }
</style>
