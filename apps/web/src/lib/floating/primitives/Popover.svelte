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
		/** Drawn in the app's glass, for a popover that opens over the
		 *  painting. Menus over pages stay solid. */
		glass?: boolean;
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
		children,
		glass = false
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
		class={glass ? 'popover lq glass' : 'popover'}
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
		--z-floating: calc(var(--z-overlay) + 100);
	}

	/* FloatingContent's own surface would win over .lq, so the glass is
	   restated, as ChatInput does for the composer. */
	:global(.floating-content.popover.glass) {
		border-color: transparent;
		background: var(--lq-sheen), var(--lq-fill);
		/* design-ok: glass is lifted off the painting */
		box-shadow: var(--lq-rim), var(--lq-lift);
	}

	.popover-content {
		min-width: 150px;
	}
</style>
