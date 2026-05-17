<!--
	VimController.svelte
	====================

	Reusable wrapper component that funnels keydown events through the
	Vim modal-keybinding machine and forwards the resulting `VimAction`
	to a parent-supplied callback.

	Usage (from a panel):

	    <VimController panel="reader" on:action={handleAction}>
	        <ReaderPanel ... />
	    </VimController>

	The controller:
	  - Listens to keydown on the wrapped subtree (capture phase) so we
	    intercept BEFORE the underlying input does.
	  - Skips when `vimEnabled === false`.
	  - Skips when the target is a real text input AND we're not in Normal
	    mode (i.e. the user is typing into Beacon chat — let it through).
	  - In Normal/Visual/Command modes, `preventDefault` on every absorbed
	    key so we don't double-handle.
	  - In Insert mode, never preventDefault (passthrough).

	The controller is intentionally panel-agnostic: it doesn't know what
	"move down" means for Reader vs Library — that's the parent's job.
-->

<script lang="ts">
	import { createEventDispatcher, onMount } from "svelte";
	import { dispatch as dispatchVim, vimEnabled, vimMode } from "$lib/vim/mode";
	import type { VimAction } from "$lib/vim/types";

	export let panel: string;
	/** When true, the controller is completely inactive even if vimEnabled is on. */
	export let disabled = false;

	const dispatchEvent = createEventDispatcher<{ action: VimAction }>();

	let wrapper: HTMLDivElement | undefined;
	let enabled = false;
	let mode: string = "normal";

	$: enabled = $vimEnabled && !disabled;
	$: mode = $vimMode;

	function shouldPassthrough(target: EventTarget | null): boolean {
		if (!(target instanceof HTMLElement)) return false;
		const tag = target.tagName;
		if (tag === "INPUT" || tag === "TEXTAREA") return true;
		if (target.isContentEditable) return true;
		return false;
	}

	function onKeyDown(ev: KeyboardEvent) {
		if (!enabled) return;

		// Modifier-only key presses: ignore.
		if (
			ev.key === "Shift" ||
			ev.key === "Control" ||
			ev.key === "Meta" ||
			ev.key === "Alt"
		) {
			return;
		}

		// Cmd/Ctrl shortcuts (Cmd-P, Cmd-O, …) belong to the app, not us.
		if (ev.metaKey || (ev.ctrlKey && ev.key !== "d" && ev.key !== "u" && ev.key !== "f" && ev.key !== "b")) {
			return;
		}

		// In Insert mode, typing into a real input field should NOT be
		// intercepted — Esc still works because that's not a printable char.
		const passthrough = shouldPassthrough(ev.target);
		if (mode === "insert" && passthrough && ev.key !== "Escape") {
			return;
		}

		// In Normal mode, if focus is in a real text input, let the user
		// type — they probably clicked into Beacon chat. Esc still gets
		// processed to bail out cleanly.
		if (mode === "normal" && passthrough && ev.key !== "Escape") {
			return;
		}

		const action = dispatchVim({
			key: ev.key,
			ctrl: ev.ctrlKey,
			meta: ev.metaKey,
			alt: ev.altKey,
			shift: ev.shiftKey,
		});

		// Consume the keystroke. Insert-mode keys never reach here (early
		// return above) so this is safe.
		ev.preventDefault();
		ev.stopPropagation();

		if (action.kind !== "noop") {
			dispatchEvent("action", action);
		}
	}

	onMount(() => {
		// Capture phase = we win over `<input>`'s own handlers when in
		// Normal mode. Without capture, the input would consume `j` etc
		// before we see it.
		const el = wrapper;
		if (!el) return;
		el.addEventListener("keydown", onKeyDown, { capture: true });
		return () => {
			el.removeEventListener("keydown", onKeyDown, { capture: true });
		};
	});
</script>

<!-- tabindex=-1 means the wrapper itself can receive focus programmatically
     (needed for panels with no natural focus target) without breaking the
     real Tab order. -->
<div bind:this={wrapper} class="vim-controller" data-panel={panel} data-vim-mode={mode} tabindex="-1">
	<slot />
</div>

<style>
	.vim-controller {
		display: contents;
	}
</style>
