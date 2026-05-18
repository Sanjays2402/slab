// src/lib/vim/keymap.ts
//
// Pure (state, key) -> (state', action) reducer.
//
// "Pure" means: NO DOM access, NO stores, NO side effects. This is so
// the panel adapters can wrap us with svelte stores OR test the
// behaviour directly without mounting any UI.
//
// The keymap is intentionally a tight subset of Vim — we ship the
// motions/ops that matter for a PDF reader / file browser / chat panel,
// not a full editor. Specifically:
//
//   Normal mode: hjkl, gg/G, Ctrl-d/Ctrl-u, n/N, /, :, i/I/a/A/o/O,
//                v/V, dd, yy, Enter, count prefixes.
//   Insert mode: passthrough; Esc returns to Normal.
//   Visual mode: hjkl extend; Esc/v returns to Normal.
//   Command mode: line buffer; Enter commits; Esc cancels.

import {
	type VimAction,
	type VimPending,
	type VimState,
	initialPending,
} from "./types";

export interface KeyEvent {
	key: string; // e.g. "j", "Enter", "Escape", "ArrowDown"
	ctrl?: boolean;
	meta?: boolean;
	alt?: boolean;
	shift?: boolean;
}

export interface DispatchResult {
	state: VimState;
	action: VimAction;
}

const NOOP: VimAction = { kind: "noop" };

function clone(p: VimPending): VimPending {
	return { ...p };
}

function reset(s: VimState): VimState {
	return { mode: s.mode, pending: { ...initialPending } };
}

function parseCount(s: string): number | undefined {
	if (!s) return undefined;
	const n = parseInt(s, 10);
	return Number.isFinite(n) && n > 0 ? n : undefined;
}

/**
 * Run one keystroke through the Vim machine.
 *
 * Returns the new state AND the action the adapter should perform.
 * `action.kind === "noop"` means the keystroke was consumed (e.g. a
 * count digit) but produced no externally-visible effect yet.
 */
export function dispatchKey(state: VimState, ev: KeyEvent): DispatchResult {
	switch (state.mode) {
		case "normal":
			return normal(state, ev);
		case "insert":
			return insert(state, ev);
		case "visual":
			return visual(state, ev);
		case "command":
			return command(state, ev);
	}
}

function normal(state: VimState, ev: KeyEvent): DispatchResult {
	const k = ev.key;
	const p = state.pending;

	// Escape clears any pending input.
	if (k === "Escape") {
		return { state: reset(state), action: NOOP };
	}

	// Ctrl-d / Ctrl-u — half-page scroll, NOT count-prefixable.
	if (ev.ctrl && (k === "d" || k === "u")) {
		return {
			state: reset(state),
			action: { kind: "scroll-half", direction: k === "d" ? "down" : "up" },
		};
	}
	if (ev.ctrl && (k === "f" || k === "b")) {
		return {
			state: reset(state),
			action: { kind: "scroll-full", direction: k === "f" ? "down" : "up" },
		};
	}

	// Count prefix: digits accumulate, except a leading "0" which is a
	// motion (move-to-line-start, we model as left-extreme via move-line).
	if (/^[1-9]$/.test(k) || (k === "0" && p.count.length > 0)) {
		return {
			state: { mode: state.mode, pending: { ...p, count: p.count + k } },
			action: NOOP,
		};
	}

	// Multi-char prefixes: "g" waits for second char.
	if (p.prefix === "g") {
		if (k === "g") {
			return {
				state: reset(state),
				action: { kind: "move-line", target: "first" },
			};
		}
		// Any other key cancels the prefix.
		return { state: reset(state), action: NOOP };
	}

	// "d" prefix → expect a second char for dd.
	if (p.prefix === "d") {
		if (k === "d") {
			return {
				state: reset(state),
				action: { kind: "delete-line", count: parseCount(p.count) },
			};
		}
		return { state: reset(state), action: NOOP };
	}

	// "y" prefix → expect yy.
	if (p.prefix === "y") {
		if (k === "y") {
			return {
				state: reset(state),
				action: { kind: "yank-line", count: parseCount(p.count) },
			};
		}
		return { state: reset(state), action: NOOP };
	}

	// Single-key motions.
	const count = parseCount(p.count);
	switch (k) {
		case "h":
			return motion(state, "left", count);
		case "j":
			return motion(state, "down", count);
		case "k":
			return motion(state, "up", count);
		case "l":
			return motion(state, "right", count);
		case "G":
			return {
				state: reset(state),
				action: { kind: "move-line", target: "last" },
			};
		case "g":
			return {
				state: { mode: state.mode, pending: { ...p, prefix: "g" } },
				action: NOOP,
			};
		case "d":
			return {
				state: { mode: state.mode, pending: { ...p, prefix: "d" } },
				action: NOOP,
			};
		case "y":
			return {
				state: { mode: state.mode, pending: { ...p, prefix: "y" } },
				action: NOOP,
			};
		case "n":
			return {
				state: reset(state),
				action: { kind: "search-next", backward: false },
			};
		case "N":
			return {
				state: reset(state),
				action: { kind: "search-next", backward: true },
			};
		case "/":
			return {
				state: {
					mode: "command",
					pending: { ...initialPending, searchBackward: false },
				},
				action: { kind: "search-start", backward: false },
			};
		case "?":
			return {
				state: {
					mode: "command",
					pending: { ...initialPending, searchBackward: true },
				},
				action: { kind: "search-start", backward: true },
			};
		case ":":
			return {
				state: { mode: "command", pending: { ...initialPending } },
				action: { kind: "enter-mode", to: "command" },
			};
		case "i":
		case "I":
		case "a":
		case "A":
		case "o":
		case "O":
			return {
				state: { mode: "insert", pending: { ...initialPending } },
				action: { kind: "enter-mode", to: "insert" },
			};
		case "v":
		case "V":
			return {
				state: { mode: "visual", pending: { ...initialPending } },
				action: { kind: "enter-mode", to: "visual" },
			};
		case "Enter":
			return { state: reset(state), action: { kind: "open-item" } };
		case "J":
		case "K":
			// Vim's J/K are line-join / man-lookup. We repurpose as
			// big-jump-down / big-jump-up (5 lines).
			return motion(state, k === "J" ? "down" : "up", (count ?? 1) * 5);
		case "{":
		case "[":
			return {
				state: reset(state),
				action: { kind: "page", direction: "prev", count },
			};
		case "}":
		case "]":
			return {
				state: reset(state),
				action: { kind: "page", direction: "next", count },
			};
	}

	// Unrecognised keystroke — clear any partial state.
	return { state: reset(state), action: NOOP };
}

function motion(
	state: VimState,
	direction: "up" | "down" | "left" | "right",
	count: number | undefined,
): DispatchResult {
	return {
		state: { mode: state.mode, pending: { ...initialPending } },
		action: { kind: "move", direction, count },
	};
}

function insert(state: VimState, ev: KeyEvent): DispatchResult {
	if (ev.key === "Escape") {
		return {
			state: { mode: "normal", pending: { ...initialPending } },
			action: { kind: "enter-mode", to: "normal" },
		};
	}
	// Insert mode passes through to the underlying input — the controller
	// does NOT preventDefault, so the browser/svelte input gets the key.
	return { state, action: NOOP };
}

function visual(state: VimState, ev: KeyEvent): DispatchResult {
	if (ev.key === "Escape" || ev.key === "v" || ev.key === "V") {
		return {
			state: { mode: "normal", pending: { ...initialPending } },
			action: { kind: "enter-mode", to: "normal" },
		};
	}
	// Re-use Normal-mode motions but stay in Visual mode after.
	const result = normal({ ...state, mode: "normal" }, ev);
	return {
		state: { mode: "visual", pending: result.state.pending },
		action: result.action,
	};
}

function command(state: VimState, ev: KeyEvent): DispatchResult {
	const p = state.pending;
	if (ev.key === "Escape") {
		return {
			state: { mode: "normal", pending: { ...initialPending } },
			action: { kind: "enter-mode", to: "normal" },
		};
	}
	if (ev.key === "Enter") {
		const line = p.searchLine || p.commandLine;
		const wasSearch = p.searchLine.length > 0 || p.searchBackward;
		const action: VimAction = wasSearch
			? { kind: "search-next", backward: p.searchBackward }
			: { kind: "command", line };
		// Track the search/command in a result event the adapter can read.
		// For "search-next" we don't carry the query — adapters watch the
		// `searchLine` via a sibling store. (Simplification: adapter reads
		// state.pending.searchLine BEFORE we reset it. We expose this via
		// the returned state in normal/visual transition.)
		return {
			state: {
				mode: "normal",
				pending: { ...initialPending },
			},
			action: line ? action : { kind: "enter-mode", to: "normal" },
		};
	}
	if (ev.key === "Backspace") {
		if (p.searchLine.length > 0) {
			return {
				state: {
					mode: "command",
					pending: { ...p, searchLine: p.searchLine.slice(0, -1) },
				},
				action: NOOP,
			};
		}
		if (p.commandLine.length > 0) {
			return {
				state: {
					mode: "command",
					pending: { ...p, commandLine: p.commandLine.slice(0, -1) },
				},
				action: NOOP,
			};
		}
		// Backspace on empty line returns to Normal.
		return {
			state: { mode: "normal", pending: { ...initialPending } },
			action: { kind: "enter-mode", to: "normal" },
		};
	}
	// Single printable char appended to the active buffer.
	if (ev.key.length === 1 && !ev.ctrl && !ev.meta) {
		// If we entered via "/" or "?" the searchLine is the active buffer.
		// Otherwise it's the commandLine.
		const inSearch = p.searchBackward || hasSearchContext(p);
		const next: VimPending = inSearch
			? { ...p, searchLine: p.searchLine + ev.key }
			: { ...p, commandLine: p.commandLine + ev.key };
		return { state: { mode: "command", pending: next }, action: NOOP };
	}
	return { state, action: NOOP };
}

function hasSearchContext(p: VimPending): boolean {
	// We use a flag stored on entry — if searchLine has any chars OR the
	// backward flag was set, we're in a search.
	return p.searchLine.length > 0 || p.searchBackward;
}
