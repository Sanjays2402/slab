// src/lib/vim/mode.ts
//
// Svelte store + thin glue around the pure keymap reducer.
//
// The store exposes:
//   - `vimState` (readable): current mode + pending input
//   - `vimMode` (derived): just the Mode string for UI indicators
//   - `vimSearchQuery` (derived): the live `/foo` buffer for echo
//   - `vimCommandLine` (derived): the live `:foo` buffer for echo
//   - `vimEnabled` (writable, persisted): global on/off toggle
//   - `dispatch(ev)` — feed a key event, returns the resulting action
//   - `resetVim()` — force back to Normal, clear pending
//
// Adapters call `dispatch` and act on the returned `VimAction`. They
// also subscribe to `vimMode` to set focus rings / panel cursors.

import { derived, writable, get } from "svelte/store";
import { dispatchKey, type KeyEvent } from "./keymap";
import {
	initialState,
	type Mode,
	type VimAction,
	type VimState,
} from "./types";

const STORAGE_KEY = "slab.vim.enabled";

function loadEnabled(): boolean {
	if (typeof localStorage === "undefined") return false;
	const v = localStorage.getItem(STORAGE_KEY);
	return v === "true";
}

function persistEnabled(v: boolean) {
	if (typeof localStorage === "undefined") return;
	localStorage.setItem(STORAGE_KEY, v ? "true" : "false");
}

export const vimEnabled = writable<boolean>(loadEnabled());
vimEnabled.subscribe(persistEnabled);

export const vimState = writable<VimState>(initialState);

export const vimMode = derived<typeof vimState, Mode>(
	vimState,
	($s) => $s.mode,
);

export const vimSearchQuery = derived<typeof vimState, string>(
	vimState,
	($s) => $s.pending.searchLine,
);

export const vimCommandLine = derived<typeof vimState, string>(
	vimState,
	($s) => $s.pending.commandLine,
);

/**
 * Feed a key event into the Vim machine. Returns the resulting
 * `VimAction` (`noop` if the key was absorbed but did nothing user-visible).
 *
 * Adapters should call this from a `keydown` handler and consult the
 * returned action to drive panel-specific behaviour. If the panel
 * should NOT swallow the event (Insert mode + Tauri text field),
 * adapters can inspect `get(vimMode)` first.
 */
export function dispatch(ev: KeyEvent): VimAction {
	const current = get(vimState);
	const { state, action } = dispatchKey(current, ev);
	vimState.set(state);
	return action;
}

/** Force the Vim machine back to Normal + clear pending. */
export function resetVim(): void {
	vimState.set(initialState);
}
