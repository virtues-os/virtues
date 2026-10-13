<!--
	CodeBlockNode: the header of a code block. The language picker writes
	`language` (the highlighting follows it; `lib/document/code.ts`), and
	Copy copies the code. A language the picker does not offer is typed
	after its Other…, as a fence's info string is edited on a markdown page:
	Enter or leaving the field writes it, Escape leaves the language as it
	was, and either key gives the keyboard back to the picker. The code
	itself is the editor's, drawn below this.
-->
<script lang="ts">
	import { tick } from "svelte";
	import IconButton from "$lib/components/IconButton.svelte";
	import { LANGUAGE_CHOICES, OTHER_LANGUAGE, languageLabel, typedLanguage } from "$lib/document/languages";
	import { inComposition } from "$lib/utils/ime";
	import type { ViewState } from "$lib/document/nodeviews.svelte";

	let { view }: { view: ViewState } = $props();

	const language = $derived(typeof view.attrs.language === "string" ? view.attrs.language : "");
	/** The offer, with the block's own language when it is not one of them. */
	const choices = $derived(
		LANGUAGE_CHOICES.some(([value]) => value === language)
			? LANGUAGE_CHOICES
			: [...LANGUAGE_CHOICES, [language, languageLabel(language)] as [string, string]],
	);

	let copied = $state(false);

	/** A language is being typed, after Other… was chosen. */
	let typing = $state(false);
	let typed = $state("");
	let picker = $state<HTMLSelectElement | null>(null);

	function choose(e: Event) {
		const select = e.currentTarget as HTMLSelectElement;
		if (select.value === OTHER_LANGUAGE) {
			select.value = language;
			typed = language;
			typing = true;
			return;
		}
		view.setAttrs({ language: select.value || null });
	}

	/** Write the language typed, once. */
	function commit() {
		if (!typing) return;
		typing = false;
		const lang = typedLanguage(typed);
		if (lang !== language) view.setAttrs({ language: lang || null });
	}

	/**
	 * The field closed from the keyboard goes back to the picker it opened
	 * from: the field that had the focus is gone, and the focus would fall
	 * to the page's body, a Tab from the top of the app away from the code.
	 */
	async function backToPicker() {
		await tick();
		picker?.focus();
	}

	function onTypedKey(e: KeyboardEvent) {
		if (inComposition(e)) return;
		if (e.key === "Enter") {
			e.preventDefault();
			commit();
			void backToPicker();
		} else if (e.key === "Escape") {
			e.preventDefault();
			typing = false;
			void backToPicker();
		}
	}

	/** The field takes the keyboard as it opens, its text selected to type over. */
	function takeKeyboard(input: HTMLInputElement) {
		input.focus();
		input.select();
	}

	async function copy() {
		try {
			await navigator.clipboard.writeText(view.text());
			copied = true;
			setTimeout(() => (copied = false), 1200);
		} catch {
			copied = false;
		}
	}
</script>

<div class="doc-code-head">
	{#if view.editable && typing}
		<input
			class="doc-code-language"
			type="text"
			bind:value={typed}
			use:takeKeyboard
			onkeydown={onTypedKey}
			onblur={commit}
			aria-label="Language"
			placeholder="Language"
			spellcheck="false"
			autocapitalize="off"
		/>
	{:else if view.editable}
		<select bind:this={picker} class="doc-code-language" value={language} onchange={choose} aria-label="Language">
			{#each choices as [value, label] (value)}
				<option {value}>{label}</option>
			{/each}
			<option value={OTHER_LANGUAGE}>Other…</option>
		</select>
	{:else}
		<span class="doc-code-language">{languageLabel(language) || "Plain text"}</span>
	{/if}
	<IconButton icon={copied ? "ri:check-line" : "ri:file-copy-line"} label="Copy code" size="sm" onclick={copy} />
</div>

<style>
	.doc-code-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
		padding: 4px 8px;
	}

	.doc-code-language {
		padding: 0 4px;
		border: none;
		border-radius: 6px;
		background: transparent;
		color: var(--color-foreground-muted);
		font-size: 0.75rem;
		cursor: pointer;
	}

	input.doc-code-language {
		width: 12ch;
		cursor: text;
	}

	/* A thumb's 44pt, inside the picker and the field (design-grammar §6). */
	@media (pointer: coarse) {
		select.doc-code-language {
			min-height: 44px;
		}

		input.doc-code-language {
			min-height: 44px;
		}
	}

	.doc-code-language:hover {
		color: var(--color-foreground);
	}

	@media print {
		.doc-code-head {
			display: none;
		}
	}
</style>
