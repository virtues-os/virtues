<script lang="ts">
	import Icon from '$lib/components/Icon.svelte';

	// Selection action bar. A single "Add to chat" button that floats above the
	// selection — no textarea, no auto-focus, so the native selection stays live
	// and Cmd+C keeps copying the highlighted text. Clicking stages the quote as
	// a reference for the next message.
	let {
		rect,
		onAdd,
		onClose,
	}: {
		rect: { top: number; left: number; bottom: number; width: number };
		onAdd: () => void;
		onClose: () => void;
	} = $props();

	const WIDTH = 130;
	// Anchor above the selection so it never covers the highlighted text; flip
	// below only when there isn't room near the top of the viewport.
	const placeBelow = $derived(rect.top < 52);
	// Center the bar over the selection, clamped to the viewport.
	const left = $derived(
		Math.max(
			8,
			Math.min(
				rect.left + rect.width / 2 - WIDTH / 2,
				(typeof window !== 'undefined' ? window.innerWidth : 1200) - WIDTH - 8,
			),
		),
	);
	const top = $derived(placeBelow ? rect.bottom + 8 : rect.top - 8);

	function onKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			e.preventDefault();
			onClose();
		}
	}
</script>

<svelte:window onkeydown={onKeydown} />

<div
	class="vref-bar"
	class:below={placeBelow}
	style:top={`${top}px`}
	style:left={`${left}px`}
	style:width={`${WIDTH}px`}
	style:transform={placeBelow ? 'none' : 'translateY(-100%)'}
	role="toolbar"
	aria-label="Selection actions"
>
	<button
		type="button"
		class="vref-add"
		onclick={onAdd}
		aria-label="Add selection to chat"
	>
		<Icon icon="ri:chat-quote-line" width="14" />
		<span>Add to chat</span>
	</button>
</div>

<style>
	.vref-bar {
		position: fixed;
		z-index: var(--z-popover);
		display: flex;
		align-items: center;
		padding: 0.1875rem;
		border: 1px solid var(--color-border-subtle);
		border-radius: 0.75rem;
		background: var(--color-surface-overlay);
		box-shadow: 0 6px 22px color-mix(in srgb, var(--color-foreground) 10%, transparent);
		/* Never let the bar swallow the pointer's text-selection intent. */
		user-select: none;
	}

	.vref-add {
		flex: 1;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		gap: 0.375rem;
		height: 1.75rem;
		padding: 0 0.625rem;
		border-radius: 0.5625rem;
		font-size: 0.8125rem;
		font-weight: 500;
		color: var(--color-primary);
		background: color-mix(in srgb, var(--color-primary) 12%, transparent);
		transition: background-color 0.15s ease;
		white-space: nowrap;
	}

	.vref-add:hover {
		background: color-mix(in srgb, var(--color-primary) 20%, transparent);
	}
</style>
