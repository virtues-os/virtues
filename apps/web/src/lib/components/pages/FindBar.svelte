<!--
	FindBar: find and replace in a block page (lib/document/find.ts). Enter
	goes to the next match and Shift-Enter to the previous one; Escape closes
	and leaves the selection on the match last gone to.

	One row: the field, where you are in the matches, previous and next, a
	chevron and close. The chevron opens the rest: match case, whole words
	and a regular expression as toggles, as in a code editor's search panel,
	and on a page that can be edited Replace (the current match, then the
	next) and Replace all (every match, one undo step).

	`docked`: the row stands in the page toolbar's place (DocumentEditor
	hangs it there), drawn as a field in the bar, and the rest drops under
	the bar as a floating layer. Otherwise the bar floats over the top right
	of the text and stays there as it scrolls, so opening it does not move
	the text.
-->
<script lang="ts">
	import { onMount } from "svelte";
	import type { Editor } from "@tiptap/core";
	import IconButton from "$lib/components/IconButton.svelte";
	import TextAction from "$lib/components/TextAction.svelte";
	import { inComposition } from "$lib/utils/ime";
	import {
		clearFind,
		findNext,
		findPrevious,
		findState,
		replaceAll,
		replaceCurrent,
		setFindQuery,
		type FindOptions,
	} from "$lib/document/find";

	interface Props {
		editor: Editor;
		/** The query the bar opens with: the selected text, or nothing. */
		initial?: string;
		/** Bumped by the host on every update, so the count follows edits. */
		revision: number;
		/** Stands in the page toolbar's place, as a field in the bar. */
		docked?: boolean;
		onClose: () => void;
	}

	let { editor, initial = "", revision, docked = false, onClose }: Props = $props();

	let query = $state("");
	let replacement = $state("");
	let options = $state<FindOptions>({ caseSensitive: false, regexp: false, wholeWord: false });
	/** The options and Replace, behind the chevron. */
	let more = $state(false);
	let input: HTMLInputElement;
	// Touch-sized buttons on touch: the row's 28px buttons sit 36px apart,
	// and their grown hit areas would overlap (IconButton).
	const touch = typeof window !== "undefined" && !!window.matchMedia?.("(pointer: coarse)").matches;

	const editable = $derived.by(() => {
		void revision;
		return editor.isEditable;
	});

	const status = $derived.by(() => {
		void revision;
		const s = findState(editor.state);
		if (!s.query) return "";
		if (s.invalid) return "Not a pattern";
		if (!s.matches.length) return "No matches";
		return `${s.current + 1} of ${s.matches.length}`;
	});

	onMount(() => {
		query = initial;
		if (initial) setFindQuery(editor.view, initial, options);
		input.focus();
		input.select();
	});

	function search() {
		setFindQuery(editor.view, query, options);
	}

	function toggle(key: keyof FindOptions) {
		options = { ...options, [key]: !options[key] };
		search();
	}

	function close() {
		clearFind(editor.view);
		onClose();
		editor.commands.focus();
	}

	function onKeydown(e: KeyboardEvent) {
		if (inComposition(e)) return;
		if (e.key === "Enter") {
			e.preventDefault();
			if (e.shiftKey) findPrevious(editor.view);
			else findNext(editor.view);
		} else if (e.key === "Escape") {
			e.preventDefault();
			close();
		} else if (e.key.toLowerCase() === "g" && (e.metaKey || e.ctrlKey)) {
			e.preventDefault();
			if (e.shiftKey) findPrevious(editor.view);
			else findNext(editor.view);
		}
	}

	/** Escape from anywhere in the bar (a toggle or the chevron last pressed) closes it, as from its fields. */
	function onBarKeydown(e: KeyboardEvent) {
		if (inComposition(e) || e.defaultPrevented) return;
		if (e.key === "Escape") {
			e.preventDefault();
			close();
		}
	}

	function onReplaceKeydown(e: KeyboardEvent) {
		if (inComposition(e)) return;
		if (e.key === "Enter") {
			e.preventDefault();
			if (e.metaKey || e.ctrlKey) replaceAll(editor.view, replacement);
			else replaceCurrent(editor.view, replacement);
		} else if (e.key === "Escape") {
			e.preventDefault();
			close();
		}
	}
</script>

<div class="find-anchor" class:docked data-print="hide">
	<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
	<div class="find-bar" role="search" onkeydown={onBarKeydown}>
		<div class="find-row">
			<!-- First in the row, as a code editor's find puts it: the arrows
			     beside the count already mean previous and next. -->
			<IconButton
				size={touch ? "touch" : "md"}
				icon={more ? "mynaui:chevron-down" : "mynaui:chevron-right"}
				label={editable ? "Options and replace" : "Options"}
				expanded={more}
				onclick={() => (more = !more)}
			/>
			<input
				bind:this={input}
				bind:value={query}
				class="find-input"
				type="text"
				placeholder="Find in page"
				aria-label="Find in page"
				spellcheck="false"
				oninput={search}
				onkeydown={onKeydown}
			/>
			<span class="find-status" aria-live="polite">{status}</span>
			<IconButton size={touch ? "touch" : "md"} icon="mynaui:chevron-up" label="Previous match" shortcut="⇧⏎" onclick={() => findPrevious(editor.view)} />
			<IconButton size={touch ? "touch" : "md"} icon="mynaui:chevron-down" label="Next match" shortcut="⏎" onclick={() => findNext(editor.view)} />
			<IconButton size={touch ? "touch" : "md"} icon="mynaui:x" label="Close find" shortcut="Esc" onclick={close} />
		</div>
		{#if more}
			<div class="find-more">
				<div class="find-options">
					<IconButton size={touch ? "touch" : "md"} icon="mynaui:type-text" label="Match case" pressed={options.caseSensitive} onclick={() => toggle("caseSensitive")} />
					<IconButton size={touch ? "touch" : "md"} icon="mynaui:type-text" label="Whole words" pressed={options.wholeWord} onclick={() => toggle("wholeWord")} />
					<IconButton size={touch ? "touch" : "md"} icon="mynaui:asterisk-circle" label="Regular expression" pressed={options.regexp} onclick={() => toggle("regexp")} />
				</div>
				{#if editable}
					<div class="find-replace">
						<input
							bind:value={replacement}
							class="find-input"
							type="text"
							placeholder="Replace with"
							aria-label="Replace with"
							spellcheck="false"
							onkeydown={onReplaceKeydown}
						/>
						<TextAction onclick={() => replaceCurrent(editor.view, replacement)}>Replace</TextAction>
						<TextAction onclick={() => replaceAll(editor.view, replacement)}>Replace all</TextAction>
					</div>
				{/if}
			</div>
		{/if}
	</div>
</div>

<style>
	/* Floating, the default: sticky with no height, so it keeps the bar in
	   view while the page scrolls but takes no room, and the bar overlays
	   the text. */
	.find-anchor {
		position: sticky;
		top: 8px;
		z-index: var(--z-sticky);
		display: flex;
		justify-content: flex-end;
		height: 0;
	}

	.find-bar {
		position: relative;
		display: flex;
		flex-direction: column;
		gap: 4px;
		width: fit-content;
		max-width: 100%;
		height: fit-content;
		padding: 4px;
		border: 1px solid var(--color-border-subtle, var(--color-border));
		border-radius: var(--card-radius);
		background: var(--color-surface);
		color: var(--color-foreground-muted);
		box-shadow: var(--shadow-float);
	}

	.find-row {
		display: flex;
		align-items: center;
		gap: 4px;
	}

	.find-input {
		width: 200px;
		min-width: 0;
		margin-left: 4px;
		border: none;
		background: transparent;
		color: var(--color-foreground);
		font-size: 0.875rem;
		outline: none;
	}

	.find-status {
		min-width: 4.5em;
		font-size: 0.75rem;
		font-variant-numeric: tabular-nums;
		text-align: right;
		white-space: nowrap;
	}

	.find-more {
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 4px 0;
	}

	.find-options,
	.find-replace {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.find-options {
		gap: 4px;
	}

	.find-replace .find-input {
		flex: 1;
		height: 28px;
		padding: 0 8px;
		margin-left: 0;
		border-radius: var(--card-radius);
		background: var(--hover-bg);
	}

	/* Docked: the row is a field in the page toolbar's place, the toolbar's
	   height (28px controls), and the rest drops under the bar. */
	.find-anchor.docked {
		position: static;
		height: auto;
	}

	.docked .find-bar {
		padding: 0;
		border: none;
		background: transparent;
		box-shadow: none;
		width: 100%;
	}

	.docked .find-row {
		height: 28px;
		border-radius: var(--card-radius);
		background: var(--hover-bg);
	}

	.docked .find-input {
		flex: 1;
		width: auto;
	}

	.docked .find-more {
		position: absolute;
		top: calc(100% + 10px);
		left: 0;
		right: 0;
		padding: 8px;
		border: 1px solid var(--color-border-subtle, var(--color-border));
		border-radius: var(--card-radius);
		background: var(--color-surface);
		box-shadow: var(--shadow-float);
	}

	@media print {
		.find-anchor {
			display: none;
		}
	}
</style>
