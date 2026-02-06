<script lang="ts">
	/**
	 * SidebarPopover
	 *
	 * A popover for sidebar navigation items that opens to the right.
	 * Uses the floating UI hooks for consistent dismiss behavior.
	 */
	import type { Snippet } from 'svelte';
	import { useClickOutside, useEscapeKey } from '$lib/floating';

	interface Props {
		title: string;
		open?: boolean;
		onClose: () => void;
		children: Snippet;
	}

	let { title, open = false, onClose, children }: Props = $props();

	let popoverEl: HTMLElement | null = $state(null);

	// Use hooks for dismiss behavior (wrap callbacks to capture current prop values)
	useClickOutside(
		() => [popoverEl],
		() => onClose(),
		() => open
	);
	useEscapeKey(() => onClose(), () => open);
</script>

{#if open}
	<div class="popover" bind:this={popoverEl} role="menu" aria-label={title}>
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

	:global([data-theme='baker-street']) .popover,
	:global([data-theme='narnia']) .popover,
	:global([data-theme='canterbury']) .popover,
	:global([data-theme='borghese']) .popover,
	:global([data-theme='gatsby']) .popover,
	:global([data-theme='lyceum']) .popover,
	:global([data-theme='asgard']) .popover,
	:global([data-theme='agora']) .popover,
	:global([data-theme='shire']) .popover,
	:global([data-theme='lothlorien']) .popover {
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
		gap: var(--sidebar-item-gap, 4px);
	}
</style>
