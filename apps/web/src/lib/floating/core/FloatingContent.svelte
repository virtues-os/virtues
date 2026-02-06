<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { Anchor, FloatingOptions } from './types';
	import { useFloating } from '../hooks/useFloating.svelte';

	interface Props {
		anchor: Anchor | null;
		options?: FloatingOptions;
		class?: string;
		children: Snippet;
	}

	let { anchor, options = {}, class: className = '', children }: Props = $props();

	let floatingEl: HTMLElement | null = $state(null);
	let arrowEl: HTMLElement | null = $state(null);

	// Capture options.arrow at component creation (options are stable after mount)
	const showArrow = options.arrow;

	const floating = useFloating(
		() => anchor,
		() => floatingEl,
		() => (showArrow ? arrowEl : null),
		options
	);

	// Arrow positioning based on placement
	const arrowStyle = $derived.by(() => {
		if (floating.state.arrowX == null && floating.state.arrowY == null) return '';

		const side = floating.state.placement.split('-')[0];
		const staticSide: Record<string, string> = {
			top: 'bottom',
			right: 'left',
			bottom: 'top',
			left: 'right'
		};

		return `
			left: ${floating.state.arrowX != null ? `${floating.state.arrowX}px` : ''};
			top: ${floating.state.arrowY != null ? `${floating.state.arrowY}px` : ''};
			${staticSide[side] ?? 'bottom'}: -4px;
		`;
	});
</script>

<div
	bind:this={floatingEl}
	class="floating-content {className}"
	style={floating.style}
	data-placement={floating.state.placement}
>
	{@render children()}

	{#if showArrow}
		<div bind:this={arrowEl} class="floating-arrow" style={arrowStyle}></div>
	{/if}
</div>

<style>
	.floating-content {
		z-index: var(--z-floating, 100);
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 8px;
		box-shadow: 0 4px 16px rgba(0, 0, 0, 0.15);
		animation: floating-enter 150ms ease-out;
	}

	.floating-arrow {
		position: absolute;
		width: 8px;
		height: 8px;
		background: inherit;
		border: inherit;
		border-right: none;
		border-bottom: none;
		transform: rotate(45deg);
		z-index: -1;
	}

	/* Arrow direction based on placement */
	:global([data-placement^='top']) .floating-arrow {
		transform: rotate(-135deg);
	}

	:global([data-placement^='bottom']) .floating-arrow {
		transform: rotate(45deg);
	}

	:global([data-placement^='left']) .floating-arrow {
		transform: rotate(135deg);
	}

	:global([data-placement^='right']) .floating-arrow {
		transform: rotate(-45deg);
	}

	@keyframes floating-enter {
		from {
			opacity: 0;
			transform: scale(0.97);
		}
		to {
			opacity: 1;
			transform: scale(1);
		}
	}
</style>
