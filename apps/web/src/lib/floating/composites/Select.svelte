<script lang="ts" generics="T, V = T">
	/**
	 * Select Composite
	 *
	 * A dropdown select component with keyboard navigation and custom rendering.
	 * Built on the floating primitives with full keyboard accessibility.
	 */
	import type { Snippet } from 'svelte';
	import type { Placement } from '../core/types';
	import FloatingContent from '../core/FloatingContent.svelte';
	import { useClickOutside } from '../hooks/useClickOutside.svelte';
	import { useEscapeKey } from '../hooks/useEscapeKey.svelte';
	import { useKeyboardNav } from '../hooks/useKeyboardNav.svelte';

	interface Props {
		/** Currently selected value */
		value: V;
		/** List of items to select from */
		items: T[];
		/** Disable the select */
		disabled?: boolean;
		/** Preferred placement relative to trigger */
		placement?: Placement;
		/** Extract unique key from item for list rendering */
		getKey: (item: T) => string | number;
		/** Extract value from item (defaults to identity if T = V) */
		getValue?: (item: T) => V;
		/** Called when an item is selected */
		onSelect?: (item: T) => void;
		/** Additional class for the floating content */
		class?: string;
		/** Trigger snippet - receives (currentItem, disabled, open) */
		trigger: Snippet<[T, boolean, boolean]>;
		/** Item snippet - receives (item, isSelected, isHighlighted) */
		item: Snippet<[T, boolean, boolean]>;
	}

	let {
		value = $bindable(),
		items,
		disabled = false,
		placement = 'bottom-start',
		getKey,
		getValue,
		onSelect,
		class: className = '',
		trigger,
		item
	}: Props = $props();

	let open = $state(false);
	let triggerEl: HTMLElement | null = $state(null);
	let listEl: HTMLElement | null = $state(null);

	// If getValue is not provided, assume T and V are the same type
	function extractValue(listItem: T): V {
		return getValue ? getValue(listItem) : (listItem as unknown as V);
	}

	// Find the current item based on the value
	const currentItem = $derived(items.find((i) => extractValue(i) === value) ?? items[0]);

	// Initialize value if undefined
	$effect(() => {
		if (value === undefined && items.length > 0) {
			value = extractValue(items[0]);
		}
	});

	function toggle() {
		if (disabled) return;
		open = !open;
		if (open) {
			// Reset keyboard nav to current selection or -1
			const currentIndex = items.findIndex((i) => extractValue(i) === value);
			keyboard.selectedIndex = currentIndex >= 0 ? currentIndex : -1;
		}
	}

	function close() {
		open = false;
	}

	function selectItem(selectedItem: T) {
		value = extractValue(selectedItem);
		onSelect?.(selectedItem);
		close();
	}

	// Keyboard navigation
	const keyboard = useKeyboardNav({
		items: () => items,
		onSelect: selectItem,
		onEscape: close,
		enabled: () => open,
		loop: true
	});

	// Dismiss behavior (wrap callbacks to capture current values)
	useClickOutside(
		() => [triggerEl, listEl],
		() => close(),
		() => open
	);
	useEscapeKey(() => close(), () => open);

	// Scroll highlighted item into view
	$effect(() => {
		if (!open || keyboard.selectedIndex < 0 || !listEl) return;

		const items = listEl.querySelectorAll('[role="option"]');
		const highlightedItem = items[keyboard.selectedIndex] as HTMLElement | undefined;
		highlightedItem?.scrollIntoView({ block: 'nearest' });
	});
</script>

<div bind:this={triggerEl} class="select-trigger">
	<button
		type="button"
		onclick={toggle}
		{disabled}
		class="select-button"
		class:disabled
		aria-haspopup="listbox"
		aria-expanded={open}
	>
		{@render trigger(currentItem, disabled, open)}
	</button>
</div>

{#if open && triggerEl}
	<FloatingContent
		anchor={triggerEl}
		options={{ placement, offset: 4, flip: true, shift: true, padding: 8 }}
		class="select-dropdown {className}"
	>
		<div bind:this={listEl} role="listbox" class="select-list">
			{#each items as listItem, index (getKey(listItem))}
				{@const isSelected = extractValue(listItem) === value}
				{@const isHighlighted = keyboard.selectedIndex === index}
				<button
					type="button"
					role="option"
					aria-selected={isSelected}
					class="select-option"
					class:highlighted={isHighlighted}
					class:selected={isSelected}
					onclick={() => selectItem(listItem)}
					onmouseenter={() => {
						keyboard.selectedIndex = index;
					}}
				>
					{@render item(listItem, isSelected, isHighlighted)}
				</button>
			{/each}
		</div>
	</FloatingContent>
{/if}

<style>
	.select-trigger {
		display: inline-block;
		width: 100%;
	}

	.select-button {
		width: 100%;
		cursor: pointer;
		border: none;
		background: transparent;
		padding: 0;
		text-align: left;
	}

	.select-button.disabled {
		cursor: not-allowed;
		opacity: 0.5;
	}

	:global(.select-dropdown) {
		--z-floating: 100;
		min-width: 160px;
		max-height: 320px;
		overflow: hidden;
	}

	.select-list {
		max-height: 320px;
		overflow-y: auto;
		padding: 4px;
	}

	.select-option {
		width: 100%;
		border: none;
		background: transparent;
		padding: 0;
		text-align: left;
		cursor: pointer;
		border-radius: 6px;
	}

	.select-option.highlighted {
		background: color-mix(in srgb, var(--color-foreground) 8%, transparent);
	}

	.select-option.selected {
		background: color-mix(in srgb, var(--color-primary) 12%, transparent);
	}

	.select-option.highlighted.selected {
		background: color-mix(in srgb, var(--color-primary) 18%, transparent);
	}
</style>
