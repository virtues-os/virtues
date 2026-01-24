<script lang="ts">
	import { onMount } from 'svelte';
	import type { Snippet } from 'svelte';

	interface Props {
		title: string;
		open?: boolean;
		onClose: () => void;
		children: Snippet;
	}

	let { title, open = false, onClose, children }: Props = $props();

	let popoverEl: HTMLElement | null = $state(null);

	function handleClickOutside(e: MouseEvent) {
		if (popoverEl && !popoverEl.contains(e.target as Node)) {
			onClose();
		}
	}

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			onClose();
		}
	}

	onMount(() => {
		if (open) {
			document.addEventListener('click', handleClickOutside, true);
			document.addEventListener('keydown', handleKeydown);
		}

		return () => {
			document.removeEventListener('click', handleClickOutside, true);
			document.removeEventListener('keydown', handleKeydown);
		};
	});

	$effect(() => {
		if (open) {
			document.addEventListener('click', handleClickOutside, true);
			document.addEventListener('keydown', handleKeydown);
		} else {
			document.removeEventListener('click', handleClickOutside, true);
			document.removeEventListener('keydown', handleKeydown);
		}
	});
</script>

{#if open}
	<div
		class="popover"
		bind:this={popoverEl}
		role="menu"
		aria-label={title}
	>
		<div class="popover-header">{title}</div>
		<div class="popover-content">
			{@render children()}
		</div>
	</div>
{/if}

<style>
	@reference "../../../app.css";

	.popover {
		position: absolute;
		left: 100%;
		top: 0;
		margin-left: 8px;
		z-index: 1000;
		min-width: 200px;
		max-width: 280px;
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: 8px;
		box-shadow: 0 4px 12px rgba(0, 0, 0, 0.1);
		animation: popover-fade-in 150ms ease-out;
	}

	:global([data-theme="midnight-oil"]) .popover,
	:global([data-theme="narnia-nights"]) .popover,
	:global([data-theme="dumb-ox"]) .popover,
	:global([data-theme="chiaroscuro"]) .popover,
	:global([data-theme="stoa"]) .popover,
	:global([data-theme="lyceum"]) .popover,
	:global([data-theme="tabula-rasa"]) .popover,
	:global([data-theme="hemlock"]) .popover,
	:global([data-theme="shire"]) .popover {
		box-shadow: 0 4px 16px rgba(0, 0, 0, 0.3);
	}

	@keyframes popover-fade-in {
		from {
			opacity: 0;
			transform: translateX(-4px);
		}
		to {
			opacity: 1;
			transform: translateX(0);
		}
	}

	.popover-header {
		padding: 8px 12px;
		font-size: 11px;
		font-weight: 500;
		text-transform: uppercase;
		letter-spacing: 0.02em;
		color: var(--foreground-subtle);
		border-bottom: 1px solid var(--border-subtle);
	}

	.popover-content {
		padding: 6px;
		display: flex;
		flex-direction: column;
		gap: 2px;
	}
</style>
