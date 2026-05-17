// src/lib/vim/types.ts
//
// Public types for the Vim modal-keybinding subsystem.
//
// The state machine is intentionally minimal-but-extensible. Each panel
// adapter (Reader, Library, Beacon) consumes `VimAction` values and
// translates them into panel-specific operations.
//
// Design notes (Cake/cron 2026-05-17):
// - `Mode` is closed enum, NOT free-form string, so exhaustive switch()
//   matches stay safe.
// - `VimAction` is a discriminated union (`kind` field). Add new actions
//   by extending the union — TypeScript will flag every unhandled site.
// - `count` is optional everywhere; default = 1 at the adapter layer.

export type Mode = "normal" | "insert" | "visual" | "command";

export interface VimContext {
	/** The currently-focused panel id (reader, library, beacon, …). */
	panel: string;
	/** Whether the focused element is a text input (disables Normal). */
	editingText: boolean;
}

export type VimAction =
	| { kind: "noop" }
	| { kind: "enter-mode"; to: Mode }
	| { kind: "move"; direction: "up" | "down" | "left" | "right"; count?: number }
	| { kind: "move-line"; target: "first" | "last" }
	| { kind: "scroll-half"; direction: "up" | "down" }
	| { kind: "scroll-full"; direction: "up" | "down" }
	| { kind: "page"; direction: "next" | "prev"; count?: number }
	| { kind: "search-next"; backward?: boolean }
	| { kind: "search-start"; backward?: boolean }
	| { kind: "command"; line: string }
	| { kind: "delete-line"; count?: number }
	| { kind: "open-item" }
	| { kind: "yank-line"; count?: number };

/**
 * A pending key sequence. We hold partial input (e.g. "g" waiting for
 * "g" → "gg", or "3" waiting for a motion) here.
 */
export interface VimPending {
	count: string; // numeric prefix accumulator
	prefix: string; // multi-char op prefix (g, d, y, …)
	commandLine: string; // : … <CR>
	searchLine: string; // / … <CR> or ? … <CR>
	searchBackward: boolean;
}

export interface VimState {
	mode: Mode;
	pending: VimPending;
}

export const initialPending: VimPending = {
	count: "",
	prefix: "",
	commandLine: "",
	searchLine: "",
	searchBackward: false,
};

export const initialState: VimState = {
	mode: "normal",
	pending: { ...initialPending },
};
