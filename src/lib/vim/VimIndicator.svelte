<!--
	VimIndicator.svelte
	===================

	Tiny pill that shows the current Vim mode + any pending input (count
	prefix, command line buffer, search query). Floats bottom-left of the
	main window, only visible when `vimEnabled` is true.

	Mirrors Vim's own statusline convention:
	  -- NORMAL --
	  -- INSERT --
	  -- VISUAL --
	  :foo
	  /foo
-->

<script lang="ts">
	import {
		vimCommandLine,
		vimEnabled,
		vimMode,
		vimSearchQuery,
		vimState,
	} from "$lib/vim/mode";

	$: enabled = $vimEnabled;
	$: mode = $vimMode;
	$: search = $vimSearchQuery;
	$: cmd = $vimCommandLine;
	$: count = $vimState.pending.count;
	$: prefix = $vimState.pending.prefix;
	$: backward = $vimState.pending.searchBackward;

	$: label = (() => {
		if (mode === "command") {
			if (search || backward) return (backward ? "?" : "/") + search;
			return ":" + cmd;
		}
		const base = `-- ${mode.toUpperCase()} --`;
		const pending = `${count}${prefix}`;
		return pending ? `${base} ${pending}` : base;
	})();
</script>

{#if enabled}
	<div class="vim-pill" role="status" aria-live="polite">
		<span class="vim-label" data-mode={mode}>{label}</span>
	</div>
{/if}

<style>
	.vim-pill {
		position: fixed;
		left: 16px;
		bottom: 14px;
		z-index: 90;
		padding: 4px 10px;
		font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
		font-size: 11px;
		background: rgba(0, 0, 0, 0.65);
		color: rgba(255, 255, 255, 0.92);
		border: 1px solid rgba(255, 255, 255, 0.18);
		border-radius: 6px;
		pointer-events: none;
		box-shadow: 0 2px 6px rgba(0, 0, 0, 0.25);
		user-select: none;
		letter-spacing: 0.5px;
	}

	.vim-label[data-mode="normal"] {
		color: #cdd6f4;
	}
	.vim-label[data-mode="insert"] {
		color: #a6e3a1;
	}
	.vim-label[data-mode="visual"] {
		color: #f9e2af;
	}
	.vim-label[data-mode="command"] {
		color: #89b4fa;
	}
</style>
