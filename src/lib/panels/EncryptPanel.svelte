<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { idle, basename, stripExt, type CmdResult, type Status } from "$lib/types";

  type Mode = "encrypt" | "decrypt";

  let mode = $state<Mode>("encrypt");
  let input = $state<string | null>(null);
  let password = $state("");
  let confirm = $state("");
  let showPw = $state(false);
  let status = $state<Status>(idle);

  async function pickInput() {
    const picked = await open({
      multiple: false,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof picked !== "string") return;
    input = picked;
    status = idle;
  }

  async function run() {
    if (!input) {
      status = { kind: "err", msg: "Pick a PDF first." };
      return;
    }
    if (!password) {
      status = { kind: "err", msg: "Password required." };
      return;
    }
    if (mode === "encrypt" && password !== confirm) {
      status = { kind: "err", msg: "Passwords don't match." };
      return;
    }

    const base = stripExt(basename(input));
    const suffix = mode === "encrypt" ? "locked" : "unlocked";
    const output = await save({
      defaultPath: `${base}-${suffix}.pdf`,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof output !== "string") return;

    status = {
      kind: "working",
      msg: mode === "encrypt" ? "Locking…" : "Unlocking…",
    };
    try {
      const cmd = mode === "encrypt" ? "slab_encrypt" : "slab_decrypt";
      const res = await invoke<CmdResult<null>>(cmd, { input, output, password });
      if (res.kind === "ok") {
        status = {
          kind: "ok",
          msg:
            mode === "encrypt"
              ? `Locked → ${basename(output)}`
              : `Unlocked → ${basename(output)}`,
        };
        password = "";
        confirm = "";
      } else {
        status = { kind: "err", msg: res.message };
      }
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }
</script>

<header class="content-header">
  <h1>Encrypt & Unlock</h1>
  <p class="subtitle">Add or remove a password. RC4-40 (universal compatibility).</p>
</header>

<section class="panel">
  <div class="tabs">
    <button class:tab-active={mode === "encrypt"} onclick={() => (mode = "encrypt")}>
      Lock
    </button>
    <button class:tab-active={mode === "decrypt"} onclick={() => (mode = "decrypt")}>
      Unlock
    </button>
  </div>

  {#if !input}
    <button class="dropzone" onclick={pickInput}>
      <span class="dz-icon">+</span>
      <span class="dz-title">Choose a PDF</span>
      <span class="dz-hint">
        {mode === "encrypt"
          ? "Pick the file you want to protect."
          : "Pick the locked file you want to open."}
      </span>
    </button>
  {:else}
    <div class="file-card">
      <div>
        <div class="file-name">{basename(input)}</div>
        <div class="file-meta">{mode === "encrypt" ? "Will be locked" : "Will be unlocked"}</div>
      </div>
      <button class="ghost" onclick={pickInput}>Change</button>
    </div>

    <label class="field">
      <span class="field-label">Password</span>
      <div class="row">
        <input
          type={showPw ? "text" : "password"}
          bind:value={password}
          placeholder="••••••••"
        />
        <button class="ghost" onclick={() => (showPw = !showPw)}>
          {showPw ? "Hide" : "Show"}
        </button>
      </div>
    </label>

    {#if mode === "encrypt"}
      <label class="field">
        <span class="field-label">Confirm</span>
        <input
          type={showPw ? "text" : "password"}
          bind:value={confirm}
          placeholder="••••••••"
        />
      </label>
      <div class="note">
        Forget the password and your PDF is gone — Slab can't recover it.
      </div>
    {/if}

    <div class="actions">
      <button class="primary" onclick={run} disabled={status.kind === "working"}>
        {status.kind === "working"
          ? status.msg
          : mode === "encrypt"
            ? "Lock PDF"
            : "Unlock PDF"}
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
  .note {
    font-size: 12px;
    color: var(--text-3);
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-left: 3px solid var(--accent);
    padding: 8px 12px;
    border-radius: var(--r-sm);
  }
</style>
