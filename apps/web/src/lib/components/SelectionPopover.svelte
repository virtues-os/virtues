<script lang="ts">
	import { tick } from 'svelte';
	import Icon from '$lib/components/Icon.svelte';

	// Per-selection comment bar (Track D). The reference itself is the highlight
	// in the text — so this is just the note field. Empty = quote, typed = comment.
	let {
		rect,
		onAdd,
		onCopy,
		onClose,
	}: {
		rect: { top: number; left: number; bottom: number; width: number };
		onAdd: (comment: string) => void;
		onCopy: () => void;
		onClose: () => void;
	} = $props();

	let comment = $state('');
	let fieldEl = $state<HTMLTextAreaElement | null>(null);
	let copied = $state(false);

	const WIDTH = 300;
	// Anchor above the selection so it never covers the highlighted text; flip
	// below only when there isn't room near the top of the viewport.
	const placeBelow = $derived(rect.top < 132);
	const left = $derived(
		Math.max(
			8,
			Math.min(
				rect.left,
				(typeof window !== 'undefined' ? window.innerWidth : 1200) - WIDTH - 8,
			),
		),
	);
	const top = $derived(placeBelow ? rect.bottom + 8 : rect.top - 8);

	$effect(() => {
		tick().then(() => fieldEl?.focus());
	});

	function grow() {
		if (!fieldEl) return;
		fieldEl.style.height = 'auto';
		fieldEl.style.height = `${Math.min(fieldEl.scrollHeight, 120)}px`;
	}

	function onKeydown(e: KeyboardEvent) {
		if (e.key === 'Enter' && !e.shiftKey) {
			e.preventDefault();
			onAdd(comment.trim());
		} else if (e.key === 'Escape') {
			e.preventDefault();
			onClose();
		}
	}

	function copy() {
		onCopy();
		copied = true;
		setTimeout(() => (copied = false), 1200);
	}
</script>

<div
	class="vref-bar"
	class:below={placeBelow}
	style:top={`${top}px`}
	style:left={`${left}px`}
	style:width={`${WIDTH}px`}
	style:transform={placeBelow ? 'none' : 'translateY(-100%)'}
	role="dialog"
	aria-label="Comment on selection"
>
	<textarea
		bind:this={fieldEl}
		bind:value={comment}
		oninput={grow}
		onkeydown={onKeydown}
		rows="1"
		placeholder="Add a comment…"
		class="vref-field"
	></textarea>
	<button type="button" class="vref-icon" onclick={copy} aria-label="Copy selection">
		<Icon icon={copied ? 'ri:check-line' : 'ri:file-copy-line'} width="14" />
	</button>
	<button
		type="button"
		class="vref-add"
		onclick={() => onAdd(comment.trim())}
		aria-label="Add reference (Enter)"
	>
		<Icon icon="ri:corner-down-left-line" width="14" />
	</button>
</div>

<style>
	.vref-bar {
		position: fixed;
		z-index: 60;
		display: flex;
		align-items: flex-end;
		gap: 0.25rem;
		padding: 0.3125rem 0.3125rem 0.3125rem 0.6875rem;
		border: 1px solid var(--color-border-subtle);
		border-radius: 0.75rem;
		background: var(--color-surface-overlay);
		box-shadow: 0 6px 22px color-mix(in srgb, var(--color-foreground) 10%, transparent);
	}

	.vref-field {
		flex: 1;
		min-width: 0;
		max-height: 120px;
		resize: none;
		border: none;
		outline: none;
		background: transparent;
		padding: 0.25rem 0;
		font-family: inherit;
		font-size: 0.8125rem;
		line-height: 1.35;
		color: var(--color-foreground);
	}

	.vref-field::placeholder {
		color: var(--color-foreground-subtle);
	}

	.vref-icon,
	.vref-add {
		flex-shrink: 0;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 1.625rem;
		height: 1.625rem;
		border-radius: 0.5rem;
		transition: background-color 0.15s ease;
	}

	.vref-icon {
		color: var(--color-foreground-subtle);
	}

	.vref-icon:hover {
		background: var(--color-surface-elevated);
		color: var(--color-foreground-muted);
	}

	.vref-add {
		color: var(--color-primary);
		background: color-mix(in srgb, var(--color-primary) 12%, transparent);
	}

	.vref-add:hover {
		background: color-mix(in srgb, var(--color-primary) 20%, transparent);
	}
</style>
