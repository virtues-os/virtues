<script lang="ts">
	/**
	 * Popover Primitive
	 *
	 * A click-triggered floating element for displaying interactive content.
	 * Supports click-outside dismiss, Escape key, and smart positioning.
	 */
	import type { Snippet } from 'svelte';
	import type { Placement } from '../core/types';
	import FloatingContent from '../core/FloatingContent.svelte';
	import { useClickOutside } from '../hooks/useClickOutside.svelte';
	import { useEscapeKey } from '../hooks/useEscapeKey.svelte';

	interface Props {
		/** Whether the popover is open (bindable) */
		open?: boolean;
		/** Preferred placement relative to trigger */
		placement?: Placement;
		/** Show arrow pointer */
		arrow?: boolean;
		/** Close when clicking outside */
		closeOnClickOutside?: boolean;
		/** Close when pressing Escape */
		closeOnEscape?: boolean;
		/** Offset from anchor in pixels */
		offset?: number;
		/** Callback when popover closes */
		onClose?: () => void;
		/** Trigger element */
		trigger: Snippet<[{ open: boolean; toggle: () => void }]>;
		/** Popover content */
		children: Snippet<[{ close: () => void }]>;
	}

	let {
		open = $bindable(false),
		placement = 'bottom-start',
		arrow = false,
		closeOnClickOutside = true,
		closeOnEscape = true,
		offset = 8,
		onClose,
		trigger,
		children
	}: Props = $props();

	let triggerEl: HTMLElement | null = $state(null);
	let contentEl: HTMLElement | null = $state(null);

	function toggle() {
		open = !open;
		if (!open) {
			onClose?.();
		}
	}

	function close() {
		open = false;
		onClose?.();
	}

	// Click outside handling (wrap callback to capture current values)
	useClickOutside(
		() => [triggerEl, contentEl],
		() => close(),
		() => open && closeOnClickOutside
	);

	// Escape key handling (wrap callback to capture current values)
	useEscapeKey(() => close(), () => open && closeOnEscape);
</script>

<div bind:this={triggerEl} class="popover-trigger">
	{@render trigger({ open, toggle })}
</div>

{#if open && triggerEl}
	<FloatingContent
		anchor={triggerEl}
		options={{ placement, offset, arrow, flip: true, shift: true }}
		class="popover"
	>
		<div bind:this={contentEl} class="popover-content">
			{@render children({ close })}
		</div>
	</FloatingContent>
{/if}

<style>
	.popover-trigger {
		display: inline-block;
	}

	:global(.popover) {
		--z-floating: 200;
	}

	.popover-content {
		min-width: 150px;
	}
</style>
