<script lang="ts">
	/**
	 * Dropdown Primitive
	 *
	 * A click-triggered floating menu anchored to a trigger element.
	 * Uses Floating UI for smart positioning with automatic flip/shift.
	 * Includes click-outside and escape key dismissal.
	 */
	import type { Snippet } from 'svelte';
	import type { Placement } from '../core/types';
	import FloatingContent from '../core/FloatingContent.svelte';
	import { useClickOutside } from '../hooks/useClickOutside.svelte';
	import { useEscapeKey } from '../hooks/useEscapeKey.svelte';

	interface Props {
		/** Controlled open state */
		open?: boolean;
		/** Callback when open state changes */
		onOpenChange?: (open: boolean) => void;
		/** Preferred placement relative to trigger */
		placement?: Placement;
		/** Disable the dropdown */
		disabled?: boolean;
		/** Additional class for the floating content */
		class?: string;
		/** Trigger element(s) - receives toggle function and open state */
		trigger: Snippet<[{ toggle: () => void; open: boolean }]>;
		/** Dropdown content - receives close function */
		children: Snippet<[{ close: () => void }]>;
	}

	let {
		open = $bindable(false),
		onOpenChange,
		placement = 'bottom-start',
		disabled = false,
		class: className = '',
		trigger,
		children
	}: Props = $props();

	let triggerEl: HTMLElement | null = $state(null);
	let contentEl: HTMLElement | null = $state(null);

	function toggle() {
		if (disabled) return;
		const newState = !open;
		open = newState;
		onOpenChange?.(newState);
	}

	function close() {
		open = false;
		onOpenChange?.(false);
	}

	// Use hooks for dismiss behavior (wrap callbacks to capture current values)
	useClickOutside(
		() => [triggerEl, contentEl],
		() => close(),
		() => open
	);
	useEscapeKey(() => close(), () => open);
</script>

<div bind:this={triggerEl} class="dropdown-trigger">
	{@render trigger({ toggle, open })}
</div>

{#if open && triggerEl}
	<FloatingContent
		anchor={triggerEl}
		options={{ placement, offset: 4, flip: true, shift: true, padding: 8 }}
		class="dropdown {className}"
	>
		<div bind:this={contentEl} role="menu">
			{@render children({ close })}
		</div>
	</FloatingContent>
{/if}

<style>
	.dropdown-trigger {
		display: inline-block;
	}

	:global(.dropdown) {
		--z-floating: 100;
		min-width: 160px;
		padding: 4px;
	}
</style>
