<!--
	FindBar: find and replace in a block page (lib/document/find.ts). Enter
	goes to the next match and Shift-Enter to the previous one; Escape closes
	and leaves the selection on the match last gone to. Match case, a regular
	expression and whole words are toggles, as in the CodeMirror editor's
	search panel. On a page that can be edited, Replace replaces the current
	match and goes to the next, and Replace all replaces every match as one
	undo step.
-->
<script lang="ts">
	import { onMount } from "svelte";
	import type { Editor } from "@tiptap/core";
	import Icon from "$lib/components/Icon.svelte";
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
		onClose: () => void;
	}

	let { editor, initial = "", revision, onClose }: Props = $props();

	let query = $state("");
	let replacement = $state("");
	let options = $state<FindOptions>({ caseSensitive: false, regexp: false, wholeWord: false });
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

<div class="find-bar" role="search" data-print="hide">
	<div class="find-row">
		<Icon icon="ri:search-line" width="14" />
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
		<IconButton size={touch ? "touch" : "md"} icon="ri:font-size" label="Match case" pressed={options.caseSensitive} onclick={() => toggle("caseSensitive")} />
		<IconButton size={touch ? "touch" : "md"} icon="ri:text" label="Whole words" pressed={options.wholeWord} onclick={() => toggle("wholeWord")} />
		<IconButton size={touch ? "touch" : "md"} icon="ri:asterisk" label="Regular expression" pressed={options.regexp} onclick={() => toggle("regexp")} />
		<IconButton size={touch ? "touch" : "md"} icon="ri:arrow-up-s-line" label="Previous match" onclick={() => findPrevious(editor.view)} />
		<IconButton size={touch ? "touch" : "md"} icon="ri:arrow-down-s-line" label="Next match" onclick={() => findNext(editor.view)} />
		<IconButton size={touch ? "touch" : "md"} icon="ri:close-line" label="Close find" onclick={close} />
	</div>
	{#if editable}
		<div class="find-row">
			<Icon icon="ri:find-replace-line" width="14" />
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

<style>
	.find-bar {
		position: sticky;
		top: 8px;
		z-index: var(--z-sticky);
		display: flex;
		flex-direction: column;
		gap: 4px;
		width: fit-content;
		max-width: 100%;
		margin: 0 0 8px auto;
		padding: 4px 4px 4px 12px;
		border: 1px solid var(--color-border-subtle, var(--color-border));
		border-radius: 12px;
		background: var(--color-surface);
		color: var(--color-foreground-muted);
	}

	.find-row {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 8px;
	}

	.find-input {
		width: 180px;
		min-width: 0;
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

	@media print {
		.find-bar {
			display: none;
		}
	}
</style>
