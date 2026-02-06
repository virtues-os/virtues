<script lang="ts">
	/**
	 * LinkPopover - URL input popover for adding links to selected text
	 *
	 * Uses FloatingContent with virtual anchor for smart positioning.
	 */

	import Icon from '$lib/components/Icon.svelte';
	import { onMount } from 'svelte';
	import { fade } from 'svelte/transition';
	import { FloatingContent, useClickOutside, useEscapeKey } from '$lib/floating';

	interface Props {
		/** Position for floating positioning (selection coordinates) */
		position: { x: number; y: number };
		/** Initial URL value (for editing existing links) */
		initialUrl?: string;
		/** Called when link is submitted */
		onSubmit: (url: string) => void;
		/** Called when link should be removed */
		onRemove?: () => void;
		/** Called when popover should close */
		onClose: () => void;
	}

	let { position, initialUrl = '', onSubmit, onRemove, onClose }: Props = $props();

	let inputEl: HTMLInputElement | null = $state(null);
	let popoverEl: HTMLDivElement | null = $state(null);
	let url = $state(initialUrl);

	// Create virtual anchor from position
	const virtualAnchor = $derived({
		x: position.x,
		y: position.y,
		width: 0,
		height: 0
	});

	// Use hooks for dismiss behavior (wrap callbacks to capture current prop values)
	useClickOutside(
		() => [popoverEl],
		() => onClose(),
		() => true
	);
	useEscapeKey(() => onClose(), () => true);

	function handleSubmit(e: Event) {
		e.preventDefault();
		const trimmedUrl = url.trim();
		if (trimmedUrl) {
			// Add https:// if no protocol specified
			const finalUrl = /^https?:\/\//i.test(trimmedUrl) ? trimmedUrl : `https://${trimmedUrl}`;
			onSubmit(finalUrl);
		}
		onClose();
	}

	// Prevent mousedown from stealing focus from input
	function handleMouseDown(e: MouseEvent) {
		// Allow clicks on input and buttons
		if (
			(e.target as HTMLElement).tagName === 'INPUT' ||
			(e.target as HTMLElement).tagName === 'BUTTON'
		) {
			return;
		}
		e.preventDefault();
	}

	onMount(() => {
		// Focus input on mount
		inputEl?.focus();
		inputEl?.select();
	});
</script>

<FloatingContent
	anchor={virtualAnchor}
	options={{ placement: 'top', offset: 8, flip: true, shift: true, padding: 8 }}
	class="link-popover"
>
	<div
		bind:this={popoverEl}
		transition:fade={{ duration: 100 }}
		onmousedown={handleMouseDown}
		role="dialog"
		aria-label="Add link"
		tabindex="-1"
	>
		<form onsubmit={handleSubmit} class="link-form">
			<Icon icon="ri:link" width="14" class="link-icon" />
			<input
				bind:this={inputEl}
				type="text"
				bind:value={url}
				placeholder="Paste or type a URL..."
				class="link-input"
			/>
			{#if url.trim()}
				<button type="submit" class="link-btn apply" title="Apply link">
					<Icon icon="ri:check-line" width="16" />
				</button>
			{/if}
			{#if initialUrl && onRemove}
				<button type="button" class="link-btn remove" onclick={onRemove} title="Remove link">
					<Icon icon="ri:link-unlink" width="16" />
				</button>
			{/if}
		</form>
	</div>
</FloatingContent>

<style>
	:global(.link-popover) {
		--z-floating: 103;
		width: 280px;
		padding: 6px;
	}

	.link-form {
		display: flex;
		align-items: center;
		gap: 6px;
	}

	:global(.link-icon) {
		color: var(--color-foreground-muted);
		flex-shrink: 0;
	}

	.link-input {
		flex: 1;
		border: none;
		background: transparent;
		color: var(--color-foreground);
		font-size: 13px;
		padding: 6px 4px;
		outline: none;
	}

	.link-input::placeholder {
		color: var(--color-foreground-muted);
	}

	.link-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 28px;
		height: 28px;
		border: none;
		background: transparent;
		border-radius: 6px;
		cursor: pointer;
		color: var(--color-foreground-muted);
		transition: all 0.1s ease;
		flex-shrink: 0;
	}

	.link-btn:hover {
		background: var(--color-surface-elevated);
		color: var(--color-foreground);
	}

	.link-btn.apply {
		color: var(--color-success);
	}

	.link-btn.apply:hover {
		background: var(--color-success-subtle);
	}

	.link-btn.remove {
		color: var(--color-error);
	}

	.link-btn.remove:hover {
		background: var(--color-error-subtle);
	}
</style>
