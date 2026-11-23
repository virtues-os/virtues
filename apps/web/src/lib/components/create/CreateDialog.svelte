<script lang="ts">
	/**
	 * The frame New task and New thread share, so both open the same way:
	 * where it will go, a large title, a description that grows as you
	 * write, a row of property chips, then one button. It opens over a
	 * veil, as ⌘K's search does.
	 *
	 * Return in the title moves to the description; ⌘Return creates from
	 * anywhere. Escape closes an open picker first, then the dialog. Closing
	 * keeps what you wrote: the dialog that owns the draft decides when to
	 * clear it.
	 */
	import type { Snippet } from 'svelte';
	import Modal from '$lib/components/Modal.svelte';
	import { Button, IconButton } from '$lib';
	import { isAppleKeyboard } from '$lib/utils/platform';

	let {
		open,
		onClose,
		label,
		crumb,
		lead,
		title = $bindable(''),
		titlePlaceholder,
		body = $bindable(''),
		bodyPlaceholder,
		chips,
		canCreate,
		saving = false,
		createLabel,
		onCreate,
	}: {
		open: boolean;
		onClose: () => void;
		/** The dialog's name for a screen reader: "New task". */
		label: string;
		/** Where the new item will go, above the title. */
		crumb: Snippet;
		/** A control before the title, such as a thread's glyph. */
		lead?: Snippet;
		title?: string;
		titlePlaceholder: string;
		body?: string;
		bodyPlaceholder: string;
		chips: Snippet;
		canCreate: boolean;
		saving?: boolean;
		createLabel: string;
		onCreate: () => void;
	} = $props();

	let titleEl = $state<HTMLInputElement | null>(null);
	let bodyEl = $state<HTMLTextAreaElement | null>(null);
	const createKeys = isAppleKeyboard ? '⌘Return' : 'Ctrl+Enter';

	$effect(() => {
		if (open) requestAnimationFrame(() => titleEl?.focus());
	});

	function onKeydown(e: KeyboardEvent) {
		if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
			e.preventDefault();
			if (canCreate && !saving) onCreate();
		}
	}
</script>

<Modal {open} {onClose} {label} width="xl" bare veil>
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="frame" onkeydown={onKeydown}>
		<div class="top">
			<div class="crumb">{@render crumb()}</div>
			<IconButton icon="mynaui:x" label="Close" onclick={onClose} />
		</div>

		<div class="title-row">
			{#if lead}{@render lead()}{/if}
			<input
				bind:this={titleEl}
				bind:value={title}
				class="title"
				placeholder={titlePlaceholder}
				aria-label="Title"
				onkeydown={(e) => {
					if (e.key === 'Enter' && !e.metaKey && !e.ctrlKey && !e.isComposing) {
						e.preventDefault();
						bodyEl?.focus();
					}
				}}
			/>
		</div>
		<textarea
			bind:this={bodyEl}
			bind:value={body}
			class="body"
			placeholder={bodyPlaceholder}
			aria-label={bodyPlaceholder}
			rows="2"
		></textarea>

		<div class="chips">{@render chips()}</div>

		<div class="foot">
			<span class="keys">{createKeys}</span>
			<Button loading={saving} disabled={!canCreate} onclick={onCreate}>{createLabel}</Button>
		</div>
	</div>
</Modal>

<style>
	.frame {
		display: flex;
		flex-direction: column;
		padding: 16px 24px 20px;
	}
	.top {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
		min-height: 32px;
		margin-right: -8px;
	}
	.crumb {
		display: flex;
		align-items: center;
		gap: 8px;
		min-width: 0;
		font-size: var(--text-small);
		color: var(--color-foreground-muted);
	}
	.title-row {
		display: flex;
		align-items: center;
		gap: 12px;
		margin-top: 16px;
	}
	.title {
		flex: 1 1 auto;
		min-width: 0;
		padding: 0;
		border: 0;
		background: transparent;
		font-family: var(--font-serif);
		font-weight: 400;
		font-size: var(--text-heading);
		line-height: 32px;
		color: var(--color-foreground);
		outline: none;
	}
	.body {
		width: 100%;
		min-height: 48px;
		margin-top: 8px;
		padding: 0;
		border: 0;
		background: transparent;
		font: inherit;
		font-size: var(--text-body);
		line-height: 24px;
		color: var(--color-foreground);
		resize: none;
		field-sizing: content;
		max-height: 240px;
		outline: none;
	}
	.title::placeholder,
	.body::placeholder {
		color: var(--color-foreground-subtle);
	}
	.chips {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 8px;
		margin-top: 16px;
	}
	.foot {
		display: flex;
		align-items: center;
		gap: 16px;
		margin-top: 24px;
	}
	.keys {
		margin-left: auto;
		font-size: var(--text-caption);
		color: var(--color-foreground-subtle);
	}
</style>
