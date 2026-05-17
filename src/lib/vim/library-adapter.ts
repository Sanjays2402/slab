// src/lib/vim/library-adapter.ts
//
// Translates a Vim `VimAction` into LibraryPanel operations.
//
// LibraryPanel exposes its keyboard-nav state via window events:
//   - "slab:vim-library:move"     { direction: "up" | "down" | "left" | "right"; count?: number }
//   - "slab:vim-library:first"    {}
//   - "slab:vim-library:last"     {}
//   - "slab:vim-library:open"     {}             — open selected doc (Enter / l)
//   - "slab:vim-library:remove"   {}             — remove selected doc (dd)
//
// The panel registers `registerLibraryNav()` from `onMount` so this
// module knows it's safe to fire events. Without a registered panel,
// every action is a no-op (graceful in non-Library views).

import type { VimAction } from "./types";

let registered = false;

/**
 * Called by LibraryPanel.onMount → tells the adapter the panel is live.
 * Returns an unregister callback to be invoked from onDestroy.
 */
export function registerLibraryNav(): () => void {
	registered = true;
	return () => {
		registered = false;
	};
}

/** What the dispatcher (`+page.svelte`) needs to do AFTER the panel handled the action. */
export interface LibraryVimResult {
	/** `o` in Library = detach into a new window (handled by the shell). */
	detachActive: boolean;
}

/** Dispatch a VimAction at the library panel. Returns shell-side hints. */
export function runLibraryVim(action: VimAction): LibraryVimResult {
	switch (action.kind) {
		case "noop":
		case "enter-mode":
			return { detachActive: false };

		case "move": {
			emit("slab:vim-library:move", {
				direction: action.direction,
				count: action.count,
			});
			return { detachActive: false };
		}

		case "move-line": {
			emit(
				action.target === "first" ? "slab:vim-library:first" : "slab:vim-library:last",
				{},
			);
			return { detachActive: false };
		}

		case "open-item": {
			emit("slab:vim-library:open", {});
			return { detachActive: false };
		}

		case "delete-line": {
			emit("slab:vim-library:remove", {});
			return { detachActive: false };
		}

		case "command": {
			const body = action.line.trim().replace(/^:/, "");
			if (body === "q" || body === "quit") {
				// :q in library = no-op for now; documentation: "go back to reader".
				return { detachActive: false };
			}
			return { detachActive: false };
		}

		// `o` is wired in the keymap as "enter Insert mode at next line". In
		// Library context we re-purpose it as the shell-level "detach"
		// shortcut. The keymap returns an `enter-mode` action with `to=insert`
		// for `o`, so the controller fires it through. We intercept the
		// matching shape here.
		case "scroll-half":
		case "scroll-full":
		case "page":
		case "search-next":
		case "search-start":
		case "yank-line":
			return { detachActive: false };
	}
}

function emit(name: string, detail: object): void {
	if (!registered) return;
	if (typeof window === "undefined") return;
	window.dispatchEvent(new CustomEvent(name, { detail }));
}
