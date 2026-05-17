<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { join, tempDir } from "@tauri-apps/api/path";
  import { basename, stripExt, type CmdResult } from "$lib/types";

  type Props = {
    /** Source PDF that pdf.js refused to open. */
    input: string;
    /** Called with the path to a temp plaintext copy once the password works. */
    onUnlock: (decryptedPath: string) => void;
    /** Called when the user dismisses the modal without unlocking. */
    onCancel: () => void;
  };

  let { input, onUnlock, onCancel }: Props = $props();

  let password = $state("");
  let showPw = $state(false);
  let working = $state(false);
  let err = $state<string | null>(null);
  let pwInput: HTMLInputElement | undefined = $state();

  // Focus the password field as soon as the modal mounts.
  $effect(() => {
    pwInput?.focus();
  });

  /// Compute a stable temp path for the decrypted PDF. Same shape as
  /// `polyglotTmpOutput` in ReaderPanel — lands in OS temp, never touches
  /// user folders, includes a timestamp so retries don't clobber state.
  async function decryptedTmpOutput(src: string): Promise<string> {
    const base = stripExt(basename(src)) || "slab-unlocked";
    const stamp = Date.now().toString(36);
    const safe = base.replace(/[^A-Za-z0-9._-]/g, "_");
    const dir = await tempDir();
    return await join(dir, `slab-unlocked-${safe}-${stamp}.pdf`);
  }

  /// Convert a `slab_decrypt` error string into something a human can act on.
  /// The lopdf message for wrong-password is "Invalid password" — we promote
  /// that to a UX-grade hint instead of leaking the raw library text.
  function friendlyDecryptError(raw: string): string {
    const low = raw.toLowerCase();
    if (low.includes("invalid password") || low.includes("wrong password")) {
      return "That password didn't work. Try again.";
    }
    if (low.includes("input does not exist") || low.includes("input missing")) {
      return "The file moved or was deleted. Pick it again.";
    }
    return raw;
  }

  async function submit() {
    if (working) return;
    if (!password) {
      err = "Enter the password.";
      return;
    }
    working = true;
    err = null;
    try {
      const output = await decryptedTmpOutput(input);
      const res = await invoke<CmdResult<null>>("slab_decrypt", {
        input,
        output,
        password,
      });
      if (res.kind === "ok") {
        password = "";
        onUnlock(output);
      } else {
        err = friendlyDecryptError(res.message);
      }
    } catch (e) {
      err = friendlyDecryptError(e instanceof Error ? e.message : String(e));
    } finally {
      working = false;
    }
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      onCancel();
    } else if (e.key === "Enter") {
      e.preventDefault();
      void submit();
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div
  class="backdrop"
  role="dialog"
  aria-modal="true"
  aria-labelledby="decrypt-title"
  onclick={(e) => { if (e.target === e.currentTarget) onCancel(); }}
  onkeydown={(e) => { if (e.key === "Escape") onCancel(); }}
  tabindex="-1"
>
  <div class="modal" role="document">
    <header>
      <div class="lock-icon" aria-hidden="true">🔒</div>
      <div>
        <h1 id="decrypt-title">Password required</h1>
        <p class="subtitle">
          <span class="file">{basename(input)}</span> is encrypted.
        </p>
      </div>
    </header>

    <label class="field">
      <span class="field-label">Password</span>
      <div class="row">
        <input
          bind:this={pwInput}
          type={showPw ? "text" : "password"}
          bind:value={password}
          placeholder="••••••••"
          autocomplete="current-password"
          disabled={working}
        />
        <button
          type="button"
          class="ghost"
          onclick={() => (showPw = !showPw)}
          tabindex="-1"
        >
          {showPw ? "Hide" : "Show"}
        </button>
      </div>
    </label>

    {#if err}
      <div class="status err">✕ {err}</div>
    {/if}

    <div class="actions">
      <button type="button" class="ghost" onclick={onCancel} disabled={working}>
        Cancel
      </button>
      <button
        type="button"
        class="primary"
        onclick={submit}
        disabled={working || !password}
      >
        {working ? "Unlocking…" : "Unlock"}
      </button>
    </div>

    <p class="hint">
      Slab will keep a temporary unlocked copy in your system temp folder while
      this document is open.
    </p>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 100;
    background: rgba(0, 0, 0, 0.55);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 24px;
    backdrop-filter: blur(2px);
  }
  .modal {
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: var(--r-md, 10px);
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.4);
    max-width: 420px;
    width: 100%;
    padding: 20px 22px 18px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  header {
    display: flex;
    gap: 12px;
    align-items: flex-start;
  }
  .lock-icon {
    font-size: 28px;
    line-height: 1;
    margin-top: 2px;
  }
  h1 {
    font-size: 16px;
    margin: 0;
    font-weight: 600;
  }
  .subtitle {
    font-size: 13px;
    color: var(--text-3);
    margin: 2px 0 0;
  }
  .file {
    color: var(--text-1);
    font-weight: 500;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .field-label {
    font-size: 12px;
    color: var(--text-3);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .row {
    display: flex;
    gap: 6px;
  }
  .row input {
    flex: 1;
    background: var(--bg-2);
    border: 1px solid var(--border);
    color: var(--text-1);
    padding: 8px 10px;
    border-radius: var(--r-sm, 6px);
    font: inherit;
  }
  .row input:focus {
    outline: 2px solid var(--accent);
    outline-offset: -1px;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 4px;
  }
  button.primary,
  button.ghost {
    font: inherit;
    padding: 8px 14px;
    border-radius: var(--r-sm, 6px);
    cursor: pointer;
    border: 1px solid var(--border);
  }
  button.primary {
    background: var(--accent);
    color: var(--accent-fg, white);
    border-color: var(--accent);
  }
  button.primary:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
  button.ghost {
    background: transparent;
    color: var(--text-2);
  }
  button.ghost:hover {
    background: var(--bg-2);
  }
  .status.err {
    color: var(--danger, #e54);
    font-size: 13px;
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-left: 3px solid var(--danger, #e54);
    padding: 6px 10px;
    border-radius: var(--r-sm, 6px);
  }
  .hint {
    font-size: 11px;
    color: var(--text-3);
    margin: 4px 0 0;
    line-height: 1.4;
  }
</style>
