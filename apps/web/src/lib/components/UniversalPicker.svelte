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
		/** Show a filter box at the top of the dropdown. Needs `getSearchText`. */
		searchable?: boolean;
		/** The text a search query matches against, per item. */
		getSearchText?: (item: T) => string;
		searchPlaceholder?: string;
		/** Optional section label per item; a header renders when it changes
		 *  between consecutive items. Items must already be grouped in order. */
		getGroup?: (item: T) => string;
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
		searchable = false,
		getSearchText,
		searchPlaceholder = 'Search…',
		getGroup,
		trigger,
		item,
	}: Props & {
		trigger: Snippet<[T, boolean, boolean]>; // (currentItem, disabled, open)
		item: Snippet<[T, boolean]>; // (item, isSelected)
	} = $props();

	let query = $state('');

	// Filtered, then flattened into header/item rows so section labels can be
	// interleaved without the caller injecting fake items.
	const visibleItems = $derived.by(() => {
		const q = query.trim().toLowerCase();
		if (!q || !getSearchText) return items;
		return items.filter((it) => getSearchText(it).toLowerCase().includes(q));
	});

	type Row = { header: string } | { item: T };
	const rows = $derived.by<Row[]>(() => {
		const out: Row[] = [];
		let lastGroup: string | undefined;
		for (const it of visibleItems) {
			if (getGroup) {
				const g = getGroup(it);
				if (g !== lastGroup) {
					// Empty label = an ungrouped row (e.g. a "default" sentinel);
					// track it so the next real group still emits its header.
					if (g) out.push({ header: g });
					lastGroup = g;
				}
			}
			out.push({ item: it });
		}
		return out;
	});

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
			if (!open) query = '';
		}
	}

	// Clear the filter whenever the menu closes (click-outside / escape / select),
	// so it reopens showing the full list rather than a stale query.
	$effect(() => {
		if (!open) query = '';
	});
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
				{#if searchable}
					<div class="sticky top-0 z-10 bg-surface border-b border-border p-1">
						<!-- svelte-ignore a11y_autofocus -->
						<input
							type="text"
							bind:value={query}
							placeholder={searchPlaceholder}
							autofocus
							class="w-full px-2 py-1.5 bg-background border border-border rounded-md text-sm outline-none focus:border-border-strong"
						/>
					</div>
				{/if}
				<div class="{maxHeight} overflow-y-auto py-1">
					{#each rows as row, i (('header' in row) ? `h:${row.header}` : getKey(row.item))}
						{#if 'header' in row}
							<div class="px-3 pt-2 pb-1 text-xs font-medium text-foreground-subtle {i === 0 ? '' : 'border-t border-border mt-1'}">
								{row.header}
							</div>
						{:else}
							{@const listItem = row.item}
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
						{/if}
					{/each}
					{#if rows.length === 0}
						<div class="px-3 py-4 text-center text-sm text-foreground-muted">
							No matches
						</div>
					{/if}
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
