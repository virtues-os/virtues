<script lang="ts">
	/**
	 * Tooltip Primitive
	 *
	 * A hover/focus-triggered floating element for displaying helpful text.
	 * Uses Floating UI for smart positioning with automatic flip/shift.
	 */
	import type { Snippet } from 'svelte';
	import type { Placement } from '../core/types';
	import FloatingContent from '../core/FloatingContent.svelte';

	interface Props {
		/** Text content to display in the tooltip */
		content: string;
		/** Delay before showing tooltip (ms) */
		delay?: number;
		/** Preferred placement relative to trigger */
		placement?: Placement;
		/** Show arrow pointer */
		arrow?: boolean;
		/** Disable the tooltip */
		disabled?: boolean;
		/** Trigger element(s) */
		children: Snippet;
	}

	let {
		content,
		delay = 200,
		placement = 'top',
		arrow = false,
		disabled = false,
		children
	}: Props = $props();

	let triggerEl: HTMLElement | null = $state(null);
	let isVisible = $state(false);
	let timeoutId: ReturnType<typeof setTimeout> | undefined;

	function show() {
		if (disabled) return;
		clearTimeout(timeoutId);
		timeoutId = setTimeout(() => {
			isVisible = true;
		}, delay);
	}

	function hide() {
		clearTimeout(timeoutId);
		isVisible = false;
	}

	// Cleanup on unmount
	$effect(() => {
		return () => {
			clearTimeout(timeoutId);
		};
	});
</script>

<div
	bind:this={triggerEl}
	class="tooltip-trigger"
	role="presentation"
	onmouseenter={show}
	onmouseleave={hide}
	onfocus={show}
	onblur={hide}
>
	{@render children()}
</div>

{#if isVisible && triggerEl && content}
	<FloatingContent anchor={triggerEl} options={{ placement, offset: 8, arrow }} class="tooltip">
		<span role="tooltip">{content}</span>
	</FloatingContent>
{/if}

<style>
	.tooltip-trigger {
		display: inline-block;
	}

	:global(.tooltip) {
		--z-floating: 300;
		padding: 6px 10px;
		font-size: 12px;
		pointer-events: none;
		max-width: 250px;
		text-align: center;
	}
</style>
