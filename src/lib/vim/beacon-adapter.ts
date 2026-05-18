// src/lib/vim/beacon-adapter.ts
//
// Translates a Vim `VimAction` into BeaconChatPanel operations.
//
// Beacon is mostly an input-driven panel (typing prompts), so the Vim
// integration is intentionally focused on the few orthogonal modes that
// matter for chat:
//
//   - Normal mode `j` / `k`         → scroll chat history by line
//   - Normal mode `<C-d>` / `<C-u>` → scroll by half page
//   - Normal mode `<C-f>` / `<C-b>` → scroll by full page
//   - Normal mode `gg` / `G`        → top / bottom of history
//   - Normal mode `i` / `a` / `o`   → enter Insert (focus textarea)
//   - Insert mode  `<Esc>`           → exit Insert (blur textarea)
//   - Normal mode `dd`              → reset current chat (new conversation)
//   - Normal mode `:q<CR>`          → also reset chat
//
// Emitted window events (camelCase namespace per panel):
//   - "slab:vim-beacon:scroll"        { kind: "line"|"half"|"full"; direction: "up"|"down"; count?: number }
//   - "slab:vim-beacon:scroll-edge"   { target: "first" | "last" }
//   - "slab:vim-beacon:focus-input"   {}
//   - "slab:vim-beacon:blur-input"    {}
//   - "slab:vim-beacon:reset-chat"    {}
//
// Like the Library adapter, emission gates on `registerBeaconNav()` so
// the events are silent no-ops when BeaconChatPanel is not mounted.

import type { VimAction } from "./types";

let registered = false;

/**
 * Called by BeaconChatPanel.onMount — tells the adapter the panel is live.
 * Returns an unregister callback to be invoked from onDestroy.
 */
export function registerBeaconNav(): () => void {
	registered = true;
	return () => {
		registered = false;
	};
}

/** Shell-side hints after dispatching the Vim action. (None used today.) */
export type BeaconVimResult = Record<string, never>;

/** Dispatch a single VimAction at the Beacon panel. */
export function runBeaconVim(action: VimAction): BeaconVimResult {
	switch (action.kind) {
		case "noop":
			return {};

		case "enter-mode": {
			if (action.to === "insert") {
				emit("slab:vim-beacon:focus-input", {});
			} else if (action.to === "normal") {
				// Coming out of Insert (Esc) — blur the textarea so the next
				// Normal-mode keystroke isn't passed through to it.
				emit("slab:vim-beacon:blur-input", {});
			}
			return {};
		}

		case "move": {
			// j/k = line scroll. h/l are no-ops in chat.
			if (action.direction === "down" || action.direction === "up") {
				emit("slab:vim-beacon:scroll", {
					kind: "line",
					direction: action.direction,
					count: action.count,
				});
			}
			return {};
		}

		case "move-line": {
			emit("slab:vim-beacon:scroll-edge", { target: action.target });
			return {};
		}

		case "scroll-half": {
			emit("slab:vim-beacon:scroll", {
				kind: "half",
				direction: action.direction,
			});
			return {};
		}

		case "scroll-full": {
			emit("slab:vim-beacon:scroll", {
				kind: "full",
				direction: action.direction,
			});
			return {};
		}

		case "delete-line": {
			// `dd` in Beacon resets the current conversation. Equivalent
			// to clicking the "New chat" button.
			emit("slab:vim-beacon:reset-chat", {});
			return {};
		}

		case "command": {
			const body = action.line.trim().replace(/^:/, "");
			if (body === "q" || body === "quit") {
				// `:q` in chat = back-to-empty, same as dd. Documented in
				// the keybinding overlay (Cmd-/) so users discover it.
				emit("slab:vim-beacon:reset-chat", {});
			}
			return {};
		}

		// Not applicable in Beacon:
		case "page":
		case "search-next":
		case "search-start":
		case "open-item":
		case "yank-line":
			return {};
	}
}

function emit(name: string, detail: object): void {
	if (!registered) return;
	if (typeof window === "undefined") return;
	window.dispatchEvent(new CustomEvent(name, { detail }));
}
