<script lang="ts">
	/**
	 * The chat composer's editor, for a task's notes and for a reply to a
	 * ready request: the same keys and the same prose as a chat message.
	 * Return commits (saves the notes, sends the reply), Shift+Return breaks
	 * the line, Escape leaves the field. Notes also save when the field loses
	 * focus.
	 */
	import { onMount, untrack } from "svelte";
	import { createComposer, type Composer } from "$lib/document/composer";
	import { mobileLayout } from "$lib/stores/mobileLayout.svelte";

	let {
		value = "",
		placeholder = "",
		label,
		commitOnBlur = false,
		autofocus = false,
		sendLabel,
		onCommit,
	}: {
		/** The markdown to open with. */
		value?: string;
		placeholder?: string;
		/** The field's accessible name. */
		label: string;
		/** Save on leaving the field as well as on Return. */
		commitOnBlur?: boolean;
		autofocus?: boolean;
		/** Draw a button that commits, labeled this. A phone's Return breaks the line, so a reply needs one. */
		sendLabel?: string;
		/** Called with the field's markdown. Resolve true to clear the field. */
		onCommit: (markdown: string) => Promise<boolean | void> | boolean | void;
	} = $props();

	let element: HTMLDivElement | undefined = $state();
	let composer: Composer | undefined;
	// The field owns its text once open; a later `value` does not replace it.
	let draft = $state(untrack(() => value));

	async function commit() {
		const clear = await onCommit(draft);
		if (clear) {
			composer?.setMarkdown("");
			draft = "";
		}
	}

	onMount(() => {
		if (!element) return;
		composer = createComposer({
			element,
			markdown: value,
			placeholder,
			label,
			isMobile: () => mobileLayout.isMobile,
			onSubmit: () => {
				void commit();
				if (commitOnBlur) composer?.editor.commands.blur();
			},
			onChange: (markdown) => (draft = markdown),
			onFocusChange: (focused) => {
				if (!focused && commitOnBlur) void commit();
			},
			onHeight: () => {},
			onEscape: () => {
				composer?.editor.commands.blur();
				return true;
			},
		});
		if (autofocus) composer.focus();
		return () => composer?.destroy();
	});
</script>

<div class="task-composer" bind:this={element}></div>
{#if sendLabel}
	<div class="send-row">
		<button type="button" class="send" disabled={!draft.trim()} onclick={() => void commit()}>{sendLabel}</button>
	</div>
{/if}

<style>
	.task-composer {
		min-height: 1.5rem;
		color: var(--color-foreground);
	}
	.send-row {
		display: flex;
		justify-content: flex-end;
		margin-top: 0.25rem;
	}
	.send {
		background: none;
		border: none;
		padding: 0.125rem 0;
		font: inherit;
		font-size: 0.8125rem;
		font-weight: 500;
		color: var(--color-foreground);
		cursor: pointer;
	}
	.send:disabled {
		color: var(--color-foreground-subtle);
		cursor: default;
	}
	.task-composer :global(.composer-field) {
		outline: none;
	}
	.task-composer :global(.composer-field p.is-editor-empty:first-child::before) {
		content: attr(data-placeholder);
		float: left;
		height: 0;
		pointer-events: none;
		color: var(--color-foreground-subtle);
	}
</style>
