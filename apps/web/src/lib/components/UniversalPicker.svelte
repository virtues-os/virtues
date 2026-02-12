<script lang="ts" generics="T, V = T">
	/**
	 * UniversalPicker - Generic select dropdown component
	 *
	 * Uses the floating UI system for smart positioning and dismiss handling.
	 */
	import { fade } from "svelte/transition";
	import type { Snippet } from "svelte";
	import { FloatingContent, useClickOutside, useEscapeKey } from "$lib/floating";
	import type { Placement } from "$lib/floating";

	interface Props {
		value: V;
		items: T[];
		disabled?: boolean;
		width?: string;
		maxHeight?: string;
		position?: 'auto' | 'top' | 'bottom';
		getKey: (item: T) => string | number;
		getValue?: (item: T) => V;
		onSelect?: (item: T) => void;
	}

	let {
		value = $bindable(),
		items,
		disabled = false,
		width = 'w-64',
		maxHeight = 'max-h-80',
		position = 'auto',
		getKey,
		getValue,
		onSelect,
		trigger,
		item,
	}: Props & {
		trigger: Snippet<[T, boolean, boolean]>; // (currentItem, disabled, open)
		item: Snippet<[T, boolean]>; // (item, isSelected)
	} = $props();

	// If getValue is not provided, assume T and V are the same type
	function extractValue(item: T): V {
		return getValue ? getValue(item) : (item as unknown as V);
	}

	// Find the current item based on the value
	const currentItem = $derived(
		items.find(item => extractValue(item) === value) || items[0]
	);

	$effect(() => {
		if (value === undefined && items.length > 0) {
			value = extractValue(items[0]);
		}
	});

	let open = $state(false);
	let buttonElement = $state<HTMLButtonElement | null>(null);
	let dropdownElement = $state<HTMLDivElement | null>(null);

	// Convert position prop to placement for FloatingContent
	const placement = $derived<Placement>(
		position === 'top' ? 'top-start' : 'bottom-start'
	);

	// Use hooks for dismiss behavior (wrap callbacks to capture current values)
	useClickOutside(
		() => [buttonElement, dropdownElement],
		() => { open = false; },
		() => open
	);
	useEscapeKey(() => { open = false; }, () => open);

	function selectItem(selectedItem: T) {
		value = extractValue(selectedItem);
		open = false;
		onSelect?.(selectedItem);
	}

	function toggleDropdown() {
		if (!disabled) {
			open = !open;
		}
	}
</script>

<div class="relative w-full">
	<button
		bind:this={buttonElement}
		type="button"
		onclick={toggleDropdown}
		disabled={disabled}
		class="w-full flex cursor-pointer items-center gap-2 rounded text-sm transition-all duration-200"
		class:opacity-50={disabled}
		class:cursor-not-allowed={disabled}
	>
		{@render trigger(currentItem, disabled, open)}
	</button>

	{#if open && !disabled && buttonElement}
		<FloatingContent
			anchor={buttonElement}
			options={{ placement, offset: 8, flip: position === 'auto', shift: true, padding: 8 }}
			class="universal-select-dropdown"
		>
			<div
				bind:this={dropdownElement}
				class="{width} rounded-lg overflow-hidden"
				transition:fade={{ duration: 100 }}
			>
				<div class="{maxHeight} overflow-y-auto py-1">
					{#each items as listItem (getKey(listItem))}
						{@const isSelected = extractValue(listItem) === value}
						<button
							type="button"
							class="w-full text-left transition-all duration-150 px-1"
							onclick={() => selectItem(listItem)}
						>
							<div
								class="rounded-lg transition-colors"
								class:bg-primary-subtle={isSelected}
								class:hover:bg-surface-elevated={!isSelected}
							>
								{@render item(listItem, isSelected)}
							</div>
						</button>
					{/each}
				</div>
			</div>
		</FloatingContent>
	{/if}
</div>

<style>
	:global(.universal-select-dropdown) {
		--z-floating: 50;
	}
</style>
