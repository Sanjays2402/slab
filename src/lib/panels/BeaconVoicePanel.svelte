<script lang="ts">
  // Beacon Voice Mode panel (v1.9.0 Slice 15).
  //
  // Workflow:
  //   1. On mount, probe `slab_beacon_voice_capabilities` so we know what
  //      engines this host has (macOS → say, Linux → espeak-ng, Windows
  //      → powershell).
  //   2. User picks an engine + voice + rate. Voice list is loaded
  //      lazily via `slab_beacon_voice_list_voices`.
  //   3. User clicks "Test voice" → fixed test phrase.
  //   4. "Apply" persists the choice into ~/.slab/config.toml so other
  //      Beacon panels (chat, summary, …) can opt-in to auto-speak.
  //   5. "Speak custom text" lets the user paste any text and hear it
  //      read aloud — useful for accessibility + spot-checking voices.
  //
  // Design parity with BeaconGlossaryPanel: same Status type, same
  // CmdResult discriminated union, same chip+button visual style.

  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { idle, type CmdResult, type Status } from "$lib/types";

  type Voice = {
    id: string;
    name: string;
    locale: string;
    gender: string;
  };

  type Capabilities = {
    available_engines: string[];
    recommended: string | null;
    stt: boolean;
  };

  // v1.9.1 — Voice Mode: Listen (STT) types.
  type SttEngineCapability = {
    id: string;
    installed: boolean;
    binary_path?: string;
  };
  type SttCapabilities = {
    engines: SttEngineCapability[];
    recorder_available: boolean;
  };

  type VoiceCfg = {
    engine?: string;
    voice?: string;
    rate_wpm?: number;
    auto_speak_replies?: boolean;
  };

  type BeaconCfg = {
    provider?: string;
    chat_model?: string;
    embed_model?: string;
    base_url?: string;
    api_key_env?: string;
    voice?: VoiceCfg;
  };

  let caps = $state<Capabilities | null>(null);
  let sttCaps = $state<SttCapabilities | null>(null);
  let engine = $state<string>(""); // "" = nothing chosen yet
  let voices = $state<Voice[]>([]);
  let voiceId = $state<string>("");
  let rateWpm = $state<number>(175);
  let autoSpeakReplies = $state(false);
  let testText = $state(
    "Slab Beacon voice test. The quick brown fox jumps over the lazy dog.",
  );
  let isSpeaking = $state(false);
  let status = $state<Status>(idle);

  onMount(async () => {
    try {
      caps = await invoke<Capabilities>("slab_beacon_voice_capabilities");
      // v1.9.1 — probe STT capabilities alongside TTS so we can show
      // a unified Listen section. Failure is non-fatal — the section
      // simply renders "(probing…)".
      try {
        sttCaps = await invoke<SttCapabilities>(
          "slab_beacon_voice_stt_capabilities",
        );
      } catch {
        sttCaps = null;
      }
      // Best-effort: load the persisted voice config so the form
      // pre-fills with whatever the user picked last time.
      try {
        const cfg = await invoke<BeaconCfg>("slab_beacon_config_read");
        if (cfg.voice) {
          if (cfg.voice.engine) engine = cfg.voice.engine;
          if (cfg.voice.voice) voiceId = cfg.voice.voice;
          if (cfg.voice.rate_wpm) rateWpm = cfg.voice.rate_wpm;
          if (cfg.voice.auto_speak_replies)
            autoSpeakReplies = cfg.voice.auto_speak_replies;
        }
      } catch {
        // No config yet — fall through to platform defaults.
      }
      // If we still don't have an engine picked, default to the
      // platform-recommended one (assuming it's installed).
      if (!engine && caps?.recommended) {
        if ((caps.available_engines ?? []).includes(caps.recommended)) {
          engine = caps.recommended;
        }
      }
      if (engine) await loadVoices();
    } catch (e) {
      status = {
        kind: "err",
        msg: `Couldn't probe voice capabilities: ${e}`,
      };
    }
  });

  async function loadVoices() {
    if (!engine) {
      voices = [];
      return;
    }
    status = { kind: "working", msg: "Loading voices…" };
    try {
      const res = await invoke<CmdResult<Voice[]>>(
        "slab_beacon_voice_list_voices",
        { engine },
      );
      if (res.kind === "ok") {
        voices = res.value;
        status = {
          kind: "ok",
          msg: `Loaded ${res.value.length} voice${res.value.length === 1 ? "" : "s"}.`,
        };
        // If the persisted voiceId isn't in the new list, clear it so
        // the picker shows "(engine default)" rather than a dead entry.
        if (voiceId && !voices.find((v) => v.id === voiceId)) {
          voiceId = "";
        }
      } else {
        voices = [];
        status = { kind: "err", msg: res.message };
      }
    } catch (e) {
      voices = [];
      status = { kind: "err", msg: `${e}` };
    }
  }

  async function onEngineChange() {
    voiceId = "";
    await loadVoices();
  }

  async function speakText(text: string) {
    if (!engine) {
      status = { kind: "err", msg: "Pick a TTS engine first." };
      return;
    }
    if (!text.trim()) {
      status = { kind: "err", msg: "Text is empty." };
      return;
    }
    status = { kind: "working", msg: "Speaking…" };
    try {
      const res = await invoke<CmdResult<number>>("slab_beacon_voice_speak", {
        engine,
        text,
        voice: voiceId || null,
        rateWpm: rateWpm || null,
      });
      if (res.kind === "ok") {
        isSpeaking = true;
        status = { kind: "ok", msg: `Speaking (pid ${res.value}).` };
      } else {
        status = { kind: "err", msg: res.message };
      }
    } catch (e) {
      status = { kind: "err", msg: `${e}` };
    }
  }

  async function stopSpeaking() {
    try {
      await invoke<CmdResult<boolean>>("slab_beacon_voice_stop");
      isSpeaking = false;
      status = { kind: "ok", msg: "Stopped." };
    } catch (e) {
      status = { kind: "err", msg: `${e}` };
    }
  }

  async function refreshSpeakingState() {
    try {
      const res = await invoke<CmdResult<boolean>>(
        "slab_beacon_voice_is_speaking",
      );
      if (res.kind === "ok") isSpeaking = res.value;
    } catch {
      // Best-effort — leave isSpeaking alone.
    }
  }

  async function applySettings() {
    status = { kind: "working", msg: "Saving voice settings…" };
    try {
      const cfg = await invoke<BeaconCfg>("slab_beacon_config_read");
      cfg.voice = {
        engine: engine || undefined,
        voice: voiceId || undefined,
        rate_wpm: rateWpm,
        auto_speak_replies: autoSpeakReplies,
      };
      await invoke<void>("slab_beacon_config_write", { config: cfg });
      status = { kind: "ok", msg: "Voice settings saved." };
    } catch (e) {
      status = { kind: "err", msg: `Save failed: ${e}` };
    }
  }

  // Group voices by locale for a nicer dropdown. "en-us" first, then
  // others alphabetically.
  let groupedVoices = $derived.by(() => {
    const groups = new Map<string, Voice[]>();
    for (const v of voices) {
      const key = v.locale || "other";
      if (!groups.has(key)) groups.set(key, []);
      groups.get(key)!.push(v);
    }
    const entries = Array.from(groups.entries());
    entries.sort((a, b) => {
      // Prioritise the user's likely native locale.
      if (a[0] === "en-us") return -1;
      if (b[0] === "en-us") return 1;
      return a[0].localeCompare(b[0]);
    });
    return entries;
  });

  let installHint = $derived.by(() => {
    if (!caps) return null;
    if (caps.available_engines.length > 0) return null;
    if (caps.recommended === "espeak-ng") {
      return "Install espeak-ng to enable voice mode (apt install espeak-ng / dnf install espeak-ng).";
    }
    if (caps.recommended === "powershell") {
      return "PowerShell speech synthesis isn't responding — voice mode unavailable.";
    }
    if (caps.recommended === "say") {
      return "macOS `say` not found — this shouldn't happen on a stock install.";
    }
    return "No TTS engine detected on this platform.";
  });
</script>

<div class="voice-panel">
  <header>
    <h2>🔊 Voice Mode</h2>
    <p class="subtitle">
      Hear Beacon answers out loud, or read any text. Runs entirely on
      your machine — no API calls.
    </p>
  </header>

  {#if installHint}
    <div class="install-hint">
      <span>⚠️</span>
      <p>{installHint}</p>
    </div>
  {/if}

  <fieldset>
    <legend>Engine</legend>
    <select
      bind:value={engine}
      onchange={onEngineChange}
      disabled={!caps || caps.available_engines.length === 0}
    >
      <option value="">(none)</option>
      {#each caps?.available_engines ?? [] as e}
        <option value={e}>{e}</option>
      {/each}
    </select>
    {#if caps?.recommended && engine !== caps.recommended}
      <p class="hint">
        Recommended for your platform: <code>{caps.recommended}</code>
      </p>
    {/if}
  </fieldset>

  <fieldset disabled={!engine}>
    <legend>Voice</legend>
    <select bind:value={voiceId}>
      <option value="">(engine default)</option>
      {#each groupedVoices as [locale, vs]}
        <optgroup label={locale}>
          {#each vs as v}
            <option value={v.id}
              >{v.name}{v.gender ? ` (${v.gender})` : ""}</option
            >
          {/each}
        </optgroup>
      {/each}
    </select>
    {#if voices.length === 0 && engine}
      <p class="hint">No voices loaded yet.</p>
    {/if}
  </fieldset>

  <fieldset disabled={!engine}>
    <legend>Rate ({rateWpm} wpm)</legend>
    <input
      type="range"
      min="80"
      max="320"
      step="5"
      bind:value={rateWpm}
      aria-label="Words per minute"
    />
    <div class="rate-ticks">
      <span>slow</span><span>natural</span><span>fast</span>
    </div>
  </fieldset>

  <fieldset disabled={!engine}>
    <legend>Auto-speak chat replies</legend>
    <label class="checkbox-row">
      <input type="checkbox" bind:checked={autoSpeakReplies} />
      <span
        >Speak Beacon's chat replies aloud when they finish streaming.</span
      >
    </label>
  </fieldset>

  <div class="action-row">
    <button
      type="button"
      class="primary"
      disabled={!engine}
      onclick={() => speakText(testText)}
    >
      ▶︎ Test voice
    </button>
    <button
      type="button"
      disabled={!isSpeaking}
      onclick={() => {
        stopSpeaking();
      }}
    >
      ◼︎ Stop
    </button>
    <button
      type="button"
      disabled={!engine}
      onclick={() => {
        applySettings();
      }}
    >
      Save settings
    </button>
    <button
      type="button"
      class="ghost"
      onclick={() => {
        refreshSpeakingState();
      }}
      aria-label="Refresh speaking state"
    >
      ↻
    </button>
  </div>

  <fieldset disabled={!engine}>
    <legend>Custom text</legend>
    <textarea
      bind:value={testText}
      rows="4"
      placeholder="Type or paste anything to hear it read aloud…"
    ></textarea>
  </fieldset>

  <!-- v1.9.1 — Voice Mode: Listen (STT). Lives below the speak controls
       so the visual flow is "out → in" — settings for talking back to
       the user, then settings for hearing the user. -->
  <fieldset class="listen-fieldset">
    <legend>🎙 Listen (STT)</legend>
    {#if sttCaps === null}
      <p class="hint">Probing speech-to-text…</p>
    {:else}
      {#if sttCaps.engines[0]?.installed && sttCaps.recorder_available}
        <p class="hint">
          A microphone button will appear in Beacon Chat. Click to dictate
          your question; Slab transcribes it on-device with whisper.cpp.
          <strong>Audio bytes never leave this machine</strong> and the
          WAV file is deleted immediately after transcription.
        </p>
      {/if}
      <div class="listen-status">
        <div>
          <span class="ls-label">Engine:</span>
          <code>{sttCaps.engines[0]?.id ?? "(none)"}</code>
          {#if sttCaps.engines[0]?.installed}
            <span class="badge ok">installed</span>
          {:else}
            <span class="badge missing">not installed</span>
          {/if}
        </div>
        <div>
          <span class="ls-label">Recorder:</span>
          {#if sttCaps.recorder_available}
            <span class="badge ok">available</span>
          {:else}
            <span class="badge missing">missing</span>
          {/if}
        </div>
      </div>
      {#if !sttCaps.engines[0]?.installed}
        <p class="install-hint stt-hint">
          Install whisper.cpp to enable dictation:<br />
          <code>brew install whisper-cpp</code> (macOS) ·
          <code>apt install whisper-cpp</code> (Debian/Ubuntu)
        </p>
      {:else if !sttCaps.recorder_available}
        <p class="install-hint stt-hint">
          Install a microphone recorder:
          <code>brew install sox</code> (macOS) ·
          <code>apt install alsa-utils</code> (Linux)
        </p>
      {/if}
    {/if}
  </fieldset>

  {#if status.kind !== "idle"}
    <div class="status status-{status.kind}">
      {#if status.kind === "working"}⏳{:else if status.kind === "ok"}✓{:else}✗{/if}
      <span>{status.msg}</span>
    </div>
  {/if}

  <footer class="caps-info">
    <strong>This host:</strong>
    {caps
      ? caps.available_engines.length
        ? caps.available_engines.join(", ")
        : "(no engines detected)"
      : "(probing…)"}
    {#if sttCaps?.engines[0]?.installed && sttCaps.recorder_available}
      <span class="muted">· STT ready (whisper-cpp)</span>
    {:else if sttCaps}
      <span class="muted">· STT unavailable (install whisper-cpp + recorder)</span>
    {/if}
  </footer>
</div>

<style>
  .voice-panel {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 16px;
    height: 100%;
    overflow-y: auto;
    color: var(--text);
    font-family:
      -apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif;
    font-size: 13px;
  }
  header h2 {
    margin: 0 0 4px;
    font-size: 16px;
    font-weight: 600;
  }
  .subtitle {
    margin: 0;
    color: var(--text-muted, #888);
    font-size: 12px;
  }
  .install-hint {
    display: flex;
    gap: 10px;
    align-items: flex-start;
    padding: 10px 12px;
    background: var(--surface-warning, #fff8e1);
    border: 1px solid var(--border-warning, #ffd54f);
    border-radius: 6px;
  }
  .install-hint p {
    margin: 0;
    line-height: 1.4;
  }
  fieldset {
    margin: 0;
    padding: 10px 12px 12px;
    border: 1px solid var(--border, #2a2a2a);
    border-radius: 6px;
    background: var(--surface, #1a1a1a);
  }
  fieldset[disabled] {
    opacity: 0.5;
  }
  legend {
    padding: 0 6px;
    font-weight: 500;
    font-size: 12px;
    color: var(--text-muted, #888);
  }
  select,
  textarea,
  input[type="range"] {
    width: 100%;
    box-sizing: border-box;
    font-size: 13px;
    color: inherit;
    background: var(--surface-input, #111);
    border: 1px solid var(--border, #333);
    border-radius: 4px;
    padding: 6px 8px;
    font-family: inherit;
  }
  input[type="range"] {
    padding: 0;
    height: 26px;
  }
  textarea {
    resize: vertical;
    line-height: 1.5;
    min-height: 80px;
  }
  .rate-ticks {
    display: flex;
    justify-content: space-between;
    font-size: 11px;
    color: var(--text-muted, #888);
    margin-top: 4px;
  }
  .checkbox-row {
    display: flex;
    align-items: center;
    gap: 8px;
    cursor: pointer;
  }
  .checkbox-row input[type="checkbox"] {
    width: auto;
    margin: 0;
  }
  .action-row {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
    align-items: center;
  }
  .action-row button {
    padding: 6px 12px;
    font-size: 13px;
    border-radius: 4px;
    border: 1px solid var(--border, #333);
    background: var(--surface, #1a1a1a);
    color: inherit;
    cursor: pointer;
    font-family: inherit;
  }
  .action-row button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .action-row button.primary {
    background: var(--accent, #ff7a00);
    color: #fff;
    border-color: transparent;
  }
  .action-row button.primary:hover:not(:disabled) {
    filter: brightness(1.1);
  }
  .action-row button.ghost {
    border: 1px dashed var(--border, #444);
    background: transparent;
    padding: 6px 10px;
  }
  .hint {
    margin: 6px 0 0;
    font-size: 11px;
    color: var(--text-muted, #888);
  }
  .hint code {
    font-family:
      "SF Mono", Menlo, Consolas, monospace;
    background: rgba(255, 255, 255, 0.05);
    padding: 1px 5px;
    border-radius: 3px;
  }
  /* v1.9.1 — Listen (STT) status box. */
  .listen-status {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-top: 8px;
    font-size: 12px;
  }
  .listen-status code {
    font-family:
      "SF Mono", Menlo, Consolas, monospace;
    background: rgba(255, 255, 255, 0.05);
    padding: 1px 5px;
    border-radius: 3px;
  }
  .ls-label {
    color: var(--text-muted, #888);
    margin-right: 4px;
  }
  .badge {
    display: inline-block;
    margin-left: 6px;
    padding: 1px 6px;
    border-radius: 10px;
    font-size: 10px;
    font-weight: 500;
    text-transform: uppercase;
    letter-spacing: 0.4px;
  }
  .badge.ok {
    background: rgba(76, 175, 80, 0.18);
    color: #66bb6a;
  }
  .badge.missing {
    background: rgba(244, 67, 54, 0.18);
    color: #ef5350;
  }
  .stt-hint {
    margin-top: 10px;
    font-size: 12px;
  }
  .stt-hint code {
    font-family:
      "SF Mono", Menlo, Consolas, monospace;
    background: rgba(0, 0, 0, 0.25);
    padding: 1px 5px;
    border-radius: 3px;
    font-size: 11px;
  }
  .status {
    display: flex;
    gap: 6px;
    align-items: center;
    padding: 8px 12px;
    border-radius: 6px;
    font-size: 12px;
  }
  .status-ok {
    background: rgba(76, 175, 80, 0.12);
    color: #66bb6a;
  }
  .status-err {
    background: rgba(244, 67, 54, 0.12);
    color: #ef5350;
  }
  .status-working {
    background: rgba(255, 152, 0, 0.12);
    color: #ffa726;
  }
  .caps-info {
    margin-top: auto;
    padding: 10px 12px;
    background: var(--surface, #1a1a1a);
    border: 1px solid var(--border, #2a2a2a);
    border-radius: 6px;
    font-size: 11px;
    color: var(--text-muted, #888);
  }
  .muted {
    opacity: 0.6;
  }
</style>
