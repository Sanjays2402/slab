// src/lib/vim/reader-adapter.ts
//
// Translates a Vim `VimAction` (panel-agnostic) into Reader-panel
// operations dispatched as window CustomEvents.
//
// We use window events instead of direct method calls because the
// ReaderPanel lives inside a per-tab stack and only the currently-active
// tab should react. Letting the panel subscribe to events keeps the
// active-tab gating localised inside the panel itself.
//
// Emitted events (all on `window`):
//   - "slab:vim-reader:page"           { direction: "next"|"prev"; count?: number }
//   - "slab:vim-reader:goto"           { page: number }
//   - "slab:vim-reader:scroll"         { kind: "line"|"half"|"full"; direction: "up"|"down"; count?: number }
//   - "slab:vim-reader:find-open"      {}
//   - "slab:vim-reader:find-next"      { backward: boolean }
//   - "slab:vim-reader:find-set"       { query: string }
//   - "slab:vim-reader:close-tab"      {}
//   - "slab:vim-reader:focus-find"     {}   // ":/" or "/" — focus search box
//
// The caller is also responsible for closing the active reader tab on
// `:q<CR>` because tab management lives in `+page.svelte`, not here.

import type { VimAction } from "./types";

/** Result of dispatching a Vim action — tells `+page.svelte` if it needs to act. */
export interface ReaderVimResult {
	/** True iff `:q<CR>` was issued (caller should close the active tab). */
	closeTab: boolean;
	/** True iff `:tabN<CR>` was issued (caller should set active tab to N). */
	gotoTab?: number;
}

/** Parse the trailing `<command>` form (`:foo<CR>`). */
function parseCommand(line: string): VimAction | { kind: "close" } | { kind: "tab"; n: number } | null {
	const trimmed = line.trim();
	if (trimmed === "" || trimmed === ":") return null;
	// Strip leading colon if present (state machine already does, but be defensive).
	const body = trimmed.startsWith(":") ? trimmed.slice(1) : trimmed;
	if (body === "q" || body === "quit") return { kind: "close" };
	if (/^\d+$/.test(body)) {
		return { kind: "command", line: body }; // bare number = goto page
	}
	if (/^tabn(?:ext)?$/.test(body)) return null; // not supported
	const tabMatch = body.match(/^tab\s+(\d+)$/);
	if (tabMatch) return { kind: "tab", n: parseInt(tabMatch[1], 10) };
	return null;
}

/**
 * Run a single VimAction against the Reader panel.
 * Returns metadata for callers that need to operate on shell state
 * (e.g. close the active tab on `:q`).
 */
export function runReaderVim(
	action: VimAction,
	pendingSearchQuery: string,
): ReaderVimResult {
	switch (action.kind) {
		case "noop":
		case "enter-mode":
			return { closeTab: false };

		case "move": {
			// j/k = line scroll. h/l = no-op in reader (no horizontal pages).
			if (action.direction === "down" || action.direction === "up") {
				emit("slab:vim-reader:scroll", {
					kind: "line",
					direction: action.direction,
					count: action.count,
				});
			}
			return { closeTab: false };
		}

		case "move-line": {
			emit("slab:vim-reader:goto", { page: action.target === "first" ? 1 : -1 });
			return { closeTab: false };
		}

		case "scroll-half": {
			emit("slab:vim-reader:scroll", { kind: "half", direction: action.direction });
			return { closeTab: false };
		}

		case "scroll-full": {
			emit("slab:vim-reader:scroll", { kind: "full", direction: action.direction });
			return { closeTab: false };
		}

		case "page": {
			emit("slab:vim-reader:page", {
				direction: action.direction,
				count: action.count,
			});
			return { closeTab: false };
		}

		case "search-start": {
			// "/" or "?" → open the find toolbar; the query is typed live.
			emit("slab:vim-reader:find-open", {});
			return { closeTab: false };
		}

		case "search-next": {
			// n / N or Enter after /query. If a pending search query exists
			// from the Vim command line, push it into the find toolbar first.
			if (pendingSearchQuery.length > 0) {
				emit("slab:vim-reader:find-set", { query: pendingSearchQuery });
			}
			emit("slab:vim-reader:find-next", { backward: !!action.backward });
			return { closeTab: false };
		}

		case "command": {
			const parsed = parseCommand(action.line);
			if (!parsed) return { closeTab: false };
			if ("kind" in parsed && parsed.kind === "close") {
				return { closeTab: true };
			}
			if ("kind" in parsed && parsed.kind === "tab") {
				return { closeTab: false, gotoTab: parsed.n };
			}
			// Bare number → jump to page.
			if (parsed.kind === "command") {
				const n = parseInt(parsed.line, 10);
				if (Number.isFinite(n) && n > 0) {
					emit("slab:vim-reader:goto", { page: n });
				}
			}
			return { closeTab: false };
		}

		case "delete-line":
		case "yank-line":
		case "open-item":
			// Not applicable in Reader.
			return { closeTab: false };
	}
}

function emit(name: string, detail: object): void {
	if (typeof window === "undefined") return;
	window.dispatchEvent(new CustomEvent(name, { detail }));
}
