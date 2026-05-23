<script lang="ts">
  // v3.4.0 "Discovery" Slice 5 — Legal stamp panel.
  //
  // Four canonical legal presets (CONFIDENTIAL / ATTORNEY EYES ONLY /
  // PRIVILEGED & CONFIDENTIAL / DRAFT) + a Custom text option, applied as
  // a rotated semi-transparent diagonal stamp on every page (or a chosen
  // subset). Live preview re-renders on every keystroke / slider drag.
  import { invoke } from "@tauri-apps/api/core";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { idle, basename, stripExt, type CmdResult, type Status } from "$lib/types";

  type PresetKind = "confidential" | "attorney-eyes-only" | "privileged" | "draft" | "custom";

  interface Preset {
    kind: PresetKind;
    label: string;
    text: string;
    color: [number, number, number]; // 0..1 floats, matches the Rust default_color()
  }

  const PRESETS: Preset[] = [
    {
      kind: "confidential",
      label: "Confidential",
      text: "CONFIDENTIAL",
      color: [0.78, 0.1, 0.1],
    },
    {
      kind: "attorney-eyes-only",
      label: "Attorney Eyes Only",
      text: "ATTORNEY EYES ONLY",
      color: [0.55, 0.05, 0.05],
    },
    {
      kind: "privileged",
      label: "Privileged",
      text: "PRIVILEGED & CONFIDENTIAL",
      color: [0.1, 0.18, 0.55],
    },
    { kind: "draft", label: "Draft", text: "DRAFT", color: [0.45, 0.45, 0.45] },
    { kind: "custom", label: "Custom…", text: "CONFIDENTIAL", color: [0.55, 0.05, 0.05] },
  ];

  interface LegalStampReport {
    pages_stamped: number;
    text: string;
  }

  let input = $state<string | null>(null);
  let presetKind = $state<PresetKind>("confidential");
  let customText = $state("CONFIDENTIAL");
  let opacity = $state(35); // % so the slider feels natural
  let fontSize = $state(64);
  let rotationDeg = $state(45);
  let pagesText = $state(""); // blank = all
  let status = $state<Status>(idle);

  const currentPreset = $derived(PRESETS.find((p) => p.kind === presetKind) ?? PRESETS[0]);
  const stampText = $derived(
    presetKind === "custom" ? (customText.trim() || "CONFIDENTIAL") : currentPreset.text,
  );
  const stampColor = $derived(currentPreset.color);
  const cssStampFill = $derived(rgbCss(stampColor, opacity / 100));

  function rgbCss(c: [number, number, number], a: number): string {
    const r = Math.round(c[0] * 255);
    const g = Math.round(c[1] * 255);
    const b = Math.round(c[2] * 255);
    return `rgba(${r}, ${g}, ${b}, ${a.toFixed(3)})`;
  }

  function parsePages(s: string): number[] {
    const out: number[] = [];
    for (const part of s.split(",")) {
      const p = part.trim();
      if (!p) continue;
      if (p.includes("-")) {
        const [a, b] = p.split("-").map((x) => parseInt(x.trim(), 10));
        if (Number.isFinite(a) && Number.isFinite(b)) {
          for (let i = Math.min(a, b); i <= Math.max(a, b); i++) out.push(i);
        }
      } else {
        const n = parseInt(p, 10);
        if (Number.isFinite(n)) out.push(n);
      }
    }
    return out;
  }

  async function pickInput() {
    const picked = await open({
      multiple: false,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof picked !== "string") return;
    input = picked;
    status = idle;
  }

  function selectPreset(k: PresetKind) {
    presetKind = k;
    status = idle;
  }

  async function apply() {
    if (!input) {
      status = { kind: "err", msg: "Pick a PDF first." };
      return;
    }
    const base = stripExt(basename(input));
    const output = await save({
      defaultPath: `${base}-stamped.pdf`,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof output !== "string") return;

    // The Rust side uses a tagged enum: { kind: "...", text?: "..." }.
    const preset =
      presetKind === "custom"
        ? { kind: "custom", text: stampText }
        : { kind: presetKind };

    status = { kind: "working", msg: "Stamping…" };
    try {
      const res = await invoke<CmdResult<LegalStampReport>>("slab_legal_stamp_apply", {
        input,
        output,
        opts: {
          preset,
          opacity: opacity / 100,
          font_size: fontSize,
          rotation_deg: rotationDeg,
          color: null, // let Rust pick the preset default for parity with the preview
          pages: parsePages(pagesText),
        },
      });
      if (res.kind === "ok") {
        status = {
          kind: "ok",
          msg: `Stamped ${res.value.pages_stamped} pages with "${res.value.text}" → ${basename(output)}`,
        };
      } else {
        status = { kind: "err", msg: res.message };
      }
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }

  // Preview geometry — US letter at half scale, fit in a 220x285 box.
  const PREVIEW_W = 612;
  const PREVIEW_H = 792;
</script>

<header class="content-header">
  <h1>Legal stamp</h1>
  <p class="subtitle">
    Diagonal stamps for litigation discovery. CONFIDENTIAL, ATTORNEY EYES ONLY,
    PRIVILEGED, DRAFT — or your own text. One click, every page.
  </p>
</header>

<section class="panel">
  <!-- Preset chips -->
  <div class="presets" role="radiogroup" aria-label="Stamp preset">
    {#each PRESETS as p}
      <button
        class={presetKind === p.kind ? "chip active" : "chip"}
        style:--chip-tint={rgbCss(p.color, 1.0)}
        onclick={() => selectPreset(p.kind)}
        type="button"
        aria-pressed={presetKind === p.kind}
      >
        <span class="chip-dot" style:background-color={rgbCss(p.color, 1.0)}></span>
        {p.label}
      </button>
    {/each}
  </div>

  {#if presetKind === "custom"}
    <label class="field grow custom-text">
      <span class="field-label">Custom text (will be applied verbatim — uppercase recommended)</span>
      <input
        type="text"
        bind:value={customText}
        placeholder="CONFIDENTIAL — DO NOT DISTRIBUTE"
        class="mono"
        maxlength="80"
      />
    </label>
  {/if}

  <!-- Sliders -->
  <div class="grid">
    <label class="field">
      <span class="field-label">Opacity: {opacity}%</span>
      <input type="range" min="5" max="100" step="5" bind:value={opacity} />
    </label>
    <label class="field">
      <span class="field-label">Font size: {fontSize}pt</span>
      <input type="range" min="24" max="120" step="2" bind:value={fontSize} />
    </label>
    <label class="field">
      <span class="field-label">Rotation: {rotationDeg}°</span>
      <input type="range" min="0" max="90" step="5" bind:value={rotationDeg} />
    </label>
    <label class="field">
      <span class="field-label">Pages (blank = all)</span>
      <input type="text" bind:value={pagesText} placeholder="1,3,5-9" class="mono" />
    </label>
  </div>

  <!-- Live preview -->
  <div class="preview-wrap" aria-label="Live preview of stamped page">
    <svg
      class="preview"
      viewBox={`0 0 ${PREVIEW_W} ${PREVIEW_H}`}
      preserveAspectRatio="xMidYMid meet"
      role="img"
    >
      <rect
        x="0"
        y="0"
        width={PREVIEW_W}
        height={PREVIEW_H}
        fill="white"
        stroke="rgba(0,0,0,0.15)"
        stroke-width="2"
      />
      <!-- faint body text -->
      {#each Array.from({ length: 22 }) as _, i}
        <rect
          x="72"
          y={108 + i * 28}
          width={i === 21 ? 380 : 468}
          height="6"
          fill="rgba(0,0,0,0.08)"
          rx="1"
        />
      {/each}
      <!-- the rotated stamp -->
      <g transform={`translate(${PREVIEW_W / 2} ${PREVIEW_H / 2}) rotate(${-rotationDeg})`}>
        <text
          x="0"
          y="0"
          font-family="Helvetica, Arial, sans-serif"
          font-size={fontSize * 2}
          text-anchor="middle"
          dominant-baseline="middle"
          font-weight="700"
          fill={cssStampFill}
        >
          {stampText}
        </text>
      </g>
    </svg>
    <div class="preview-caption">
      Preview · <span class="mono">{stampText}</span> · {opacity}% · {rotationDeg}°
    </div>
  </div>

  <!-- File pick / apply -->
  {#if !input}
    <button class="dropzone" onclick={pickInput} type="button">
      <span class="dz-icon">+</span>
      <span class="dz-title">Choose a PDF</span>
      <span class="dz-hint">Vector stamp, every page (or a subset).</span>
    </button>
  {:else}
    <div class="file-card">
      <div>
        <div class="file-name">{basename(input)}</div>
        <div class="file-meta">Ready · will stamp "{stampText}"</div>
      </div>
      <button class="ghost" onclick={pickInput} type="button">Change</button>
    </div>
    <div class="actions">
      <button
        class="primary"
        onclick={apply}
        disabled={status.kind === "working"}
        type="button"
      >
        {status.kind === "working" ? status.msg : "Apply stamp"}
      </button>
    </div>
  {/if}

  {#if status.kind === "ok"}
    <div class="status ok">✓ {status.msg}</div>
  {:else if status.kind === "err"}
    <div class="status err">✕ {status.msg}</div>
  {/if}
</section>

<style>
  .presets {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-bottom: 14px;
  }
  .chip {
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: 999px;
    padding: 6px 14px 6px 10px;
    font-size: 13px;
    color: var(--text-2);
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    transition: all 0.12s ease;
  }
  .chip:hover {
    background: var(--bg-2);
    color: var(--text-1);
  }
  .chip.active {
    background: var(--chip-tint, var(--accent));
    color: white;
    border-color: var(--chip-tint, var(--accent));
  }
  .chip.active .chip-dot {
    background: white !important;
    box-shadow: 0 0 0 2px rgba(255, 255, 255, 0.3);
  }
  .chip-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    display: inline-block;
  }

  .custom-text {
    margin-bottom: 14px;
  }
  .mono {
    font-family: ui-monospace, SFMono-Regular, monospace;
  }

  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px 16px;
    margin-bottom: 14px;
  }

  .preview-wrap {
    background: linear-gradient(180deg, var(--bg-2), var(--bg-1));
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    padding: 16px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    margin-bottom: 16px;
  }
  .preview {
    width: 220px;
    height: 285px;
    filter: drop-shadow(0 4px 16px rgba(0, 0, 0, 0.18));
  }
  .preview-caption {
    font-size: 11px;
    color: var(--text-3);
  }

  @media (max-width: 720px) {
    .grid {
      grid-template-columns: 1fr;
    }
  }
</style>
