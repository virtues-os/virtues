<script lang="ts" generics="T, V = T">
	import { fly } from "svelte/transition";
	import { cubicOut } from "svelte/easing";
	import type { Snippet } from "svelte";

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
	let dropdownPosition = $state<'top' | 'bottom'>('bottom');
	let buttonElement = $state<HTMLButtonElement | null>(null);
	let dropdownElement = $state<HTMLDivElement | null>(null);

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

	function handleClickOutside(e: MouseEvent) {
		if (
			!buttonElement?.contains(e.target as Node) &&
			!dropdownElement?.contains(e.target as Node)
		) {
			open = false;
		}
	}

	function calculateDropdownPosition() {
		// If position is forced, use it directly
		if (position !== 'auto') {
			dropdownPosition = position;
			return;
		}

		if (!buttonElement) return;

		const rect = buttonElement.getBoundingClientRect();
		const viewportHeight = window.innerHeight;

		// Calculate available space above and below
		const spaceBelow = viewportHeight - rect.bottom;
		const spaceAbove = rect.top;

		// Assume dropdown height is around 320px (max-h-80 = 20rem = 320px) or less
		// Add some buffer for padding and borders
		const estimatedDropdownHeight = 340;

		// Prefer bottom unless there's clearly not enough space
		if (spaceBelow < estimatedDropdownHeight && spaceAbove > spaceBelow) {
			dropdownPosition = 'top';
		} else {
			dropdownPosition = 'bottom';
		}
	}

	$effect(() => {
		if (open) {
			calculateDropdownPosition();
			document.addEventListener("mousedown", handleClickOutside);
			return () => {
				document.removeEventListener("mousedown", handleClickOutside);
			};
		}
	});
</script>

<div class="relative">
	<button
		bind:this={buttonElement}
		type="button"
		onclick={toggleDropdown}
		disabled={disabled}
		class="flex cursor-pointer items-center gap-2 rounded text-sm transition-all duration-200"
		class:opacity-50={disabled}
		class:cursor-not-allowed={disabled}
	>
		{@render trigger(currentItem, disabled, open)}
	</button>

	{#if open && !disabled}
		<div
			bind:this={dropdownElement}
			class="absolute z-50 left-0 {width} bg-surface border border-border shadow-xl rounded-xl overflow-hidden backdrop-blur-sm"
			class:top-full={dropdownPosition === 'bottom'}
			class:mt-2={dropdownPosition === 'bottom'}
			class:bottom-full={dropdownPosition === 'top'}
			class:mb-2={dropdownPosition === 'top'}
			transition:fly={{
				y: dropdownPosition === 'bottom' ? -10 : 10,
				duration: 200,
				easing: cubicOut
			}}
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
	{/if}
</div>
