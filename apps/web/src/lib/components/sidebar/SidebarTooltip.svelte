<script lang="ts">
	import { onMount } from 'svelte';
	import type { Snippet } from 'svelte';

	interface Props {
		content: string;
		delay?: number;
		disabled?: boolean;
		children: Snippet;
	}

	let { content, delay = 300, disabled = false, children }: Props = $props();

	let isVisible = $state(false);
	let timeoutId: ReturnType<typeof setTimeout> | null = null;
	let triggerEl: HTMLElement | null = $state(null);
	let tooltipEl: HTMLElement | null = $state(null);

	function handleMouseEnter() {
		if (disabled) return;
		timeoutId = setTimeout(() => {
			isVisible = true;
		}, delay);
	}

	function handleMouseLeave() {
		if (timeoutId) {
			clearTimeout(timeoutId);
			timeoutId = null;
		}
		isVisible = false;
	}

	onMount(() => {
		return () => {
			if (timeoutId) {
				clearTimeout(timeoutId);
			}
		};
	});
</script>

<div
	class="tooltip-trigger"
	bind:this={triggerEl}
	onmouseenter={handleMouseEnter}
	onmouseleave={handleMouseLeave}
	role="presentation"
>
	{@render children()}

	{#if isVisible && content}
		<div
			class="tooltip"
			bind:this={tooltipEl}
			role="tooltip"
		>
			<span class="tooltip-content">{content}</span>
		</div>
	{/if}
</div>

<style>
	@reference "../../../app.css";

	.tooltip-trigger {
		position: relative;
		display: inline-flex;
	}

	.tooltip {
		position: absolute;
		left: 100%;
		top: 50%;
		transform: translateY(-50%);
		margin-left: 8px;
		z-index: 1000;
		pointer-events: none;
		animation: tooltip-fade-in 150ms ease-out;
	}

	@keyframes tooltip-fade-in {
		from {
			opacity: 0;
			transform: translateY(-50%) translateX(-4px);
		}
		to {
			opacity: 1;
			transform: translateY(-50%) translateX(0);
		}
	}

	.tooltip-content {
		display: block;
		padding: 6px 10px;
		font-size: 12px;
		font-weight: 500;
		color: var(--foreground);
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: 6px;
		box-shadow: 0 4px 12px rgba(0, 0, 0, 0.1);
		white-space: nowrap;
	}

	:global([data-theme="midnight-oil"]) .tooltip-content,
	:global([data-theme="narnia-nights"]) .tooltip-content,
	:global([data-theme="dumb-ox"]) .tooltip-content,
	:global([data-theme="chiaroscuro"]) .tooltip-content,
	:global([data-theme="stoa"]) .tooltip-content,
	:global([data-theme="lyceum"]) .tooltip-content,
	:global([data-theme="tabula-rasa"]) .tooltip-content,
	:global([data-theme="hemlock"]) .tooltip-content,
	:global([data-theme="shire"]) .tooltip-content {
		box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
	}
</style>
