// Thin wrapper around `slab_beacon_config_read` / `slab_beacon_config_write`.
//
// Lives separately from `theme.ts` because the [beacon] block has a much
// richer schema (provider, models, base URL, API-key env, voice subtree)
// and we want a typed read-modify-write helper that:
//
//   1. Never clobbers unrelated TOML sections ([ui], [keymap], [beacon.voice]).
//   2. Surfaces a Svelte store the SettingsPanel can subscribe to.
//   3. Ships a curated preset list so users can pick a sensible Ollama model
//      without typing it from memory.
//
// Added in v3.30.0 "Quill Smart Fill" Slice 3 to back the new
// "AI / Beacon" section in SettingsPanel — picking a chat model here
// takes effect on the next Smart Fill / Beacon-chat call because both
// routes ultimately call `ai::config::make_provider(&cfg.beacon)`.

import { invoke } from "@tauri-apps/api/core";
import { writable } from "svelte/store";
import type { CmdResult } from "$lib/types";

export type ProviderKind = "ollama" | "openai";

export interface VoiceCfg {
  engine?: string;
  voice?: string;
  rate_wpm?: number;
  auto_speak_replies?: boolean;
  stt_engine?: string;
  stt_model?: string;
  stt_language?: string;
  stt_trigger_word?: string;
  stt_send_on_trigger?: boolean;
}

export interface BeaconCfg {
  provider: ProviderKind;
  chat_model?: string | null;
  embed_model?: string | null;
  base_url?: string | null;
  api_key_env?: string | null;
  voice?: VoiceCfg;
}

export interface SlabCfg {
  beacon: BeaconCfg;
  // Pass-through for unknown sections so write-back is non-destructive.
  // The backend round-trips these via serde(default).
  [k: string]: unknown;
}

/** A handful of well-known Ollama chat models. The user can also type a
 *  custom name — the input is a combo box. Sizes are approximate
 *  on-disk footprints. Curated so the user gets one sensible default
 *  per use-case (fastest, smartest, forms-leaning, balanced). */
export const OLLAMA_CHAT_PRESETS: { id: string; label: string; hint: string }[] = [
  { id: "llama3.2:3b", label: "Llama 3.2 3B", hint: "fast · ~2 GB · default" },
  { id: "llama3.2:1b", label: "Llama 3.2 1B", hint: "fastest · ~1.3 GB" },
  { id: "llama3.1:8b", label: "Llama 3.1 8B", hint: "smarter · ~4.7 GB" },
  { id: "qwen2.5:7b", label: "Qwen 2.5 7B", hint: "great for forms · ~4.4 GB" },
  { id: "mistral:7b", label: "Mistral 7B", hint: "balanced · ~4.1 GB" },
];

/** OpenAI-compatible chat presets. Most folks point this at OpenAI proper
 *  or a self-hosted vLLM/LM-Studio. The hints are price + speed indicators. */
export const OPENAI_CHAT_PRESETS: { id: string; label: string; hint: string }[] = [
  { id: "gpt-4o-mini", label: "GPT-4o mini", hint: "cheap · fast · OpenAI default" },
  { id: "gpt-4o", label: "GPT-4o", hint: "smartest · pricier" },
  { id: "gpt-4.1-mini", label: "GPT-4.1 mini", hint: "latest mini-tier" },
];

const _store = writable<SlabCfg | null>(null);
/** Subscribe-only view of the most recently loaded `~/.slab/config.toml`. */
export const beaconCfg = { subscribe: _store.subscribe };

/** Read the full config from disk. Updates the store and returns the
 *  payload so callers can also use it imperatively. */
export async function loadBeaconCfg(): Promise<SlabCfg> {
  const res = await invoke<CmdResult<SlabCfg>>("slab_beacon_config_read");
  if (res.kind !== "ok") throw new Error(res.message);
  _store.set(res.value);
  return res.value;
}

/** Persist a partial Beacon update. We do a read-modify-write under the
 *  hood so unrelated sections ([ui], [keymap], [beacon.voice]) are never
 *  clobbered, even if the user has hand-edited the file outside Slab. */
export async function saveBeaconCfg(patch: Partial<BeaconCfg>): Promise<void> {
  const current = await loadBeaconCfg();
  const next: SlabCfg = {
    ...current,
    beacon: { ...current.beacon, ...patch },
  };
  const res = await invoke<CmdResult<null>>("slab_beacon_config_write", {
    config: next,
  });
  if (res.kind !== "ok") throw new Error(res.message);
  _store.set(next);
}
