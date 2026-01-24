<script lang="ts" generics="T extends Record<string, any>">
	import type { Snippet } from "svelte";
	import { fly } from "svelte/transition";
	import { cubicOut } from "svelte/easing";

	interface Props {
		items: T[];
		value: T | null;
		onValueChange: (value: T | null) => void;
		label: string;
		placeholder?: string;
		displayKey: keyof T;
		searchKey?: keyof T;
		disabled?: boolean;
		item?: Snippet<[T]>;
	}

	let {
		items,
		value = $bindable(null),
		onValueChange,
		label,
		placeholder = "Select...",
		displayKey,
		searchKey,
		disabled = false,
		item: itemSnippet,
	}: Props = $props();

	let inputValue = $state("");
	let open = $state(false);
	let highlightedIndex = $state(0);
	let inputElement: HTMLInputElement;
	let dropdownElement = $state<HTMLDivElement | null>(null);

	// Filter items based on search input
	let filteredItems = $derived(
		inputValue.length === 0
			? items
			: items.filter((item) => {
					const searchIn = searchKey ? item[searchKey] : item[displayKey];
					return String(searchIn)
						.toLowerCase()
						.includes(inputValue.toLowerCase());
				}),
	);

	// Keep input value in sync with selected value
	$effect(() => {
		if (value && !open) {
			inputValue = String(value[displayKey]);
		}
	});

	function handleFocus() {
		if (!disabled) {
			open = true;
			highlightedIndex = 0;
		}
	}

	function handleInput(e: Event) {
		const target = e.target as HTMLInputElement;
		inputValue = target.value;
		open = true;
		highlightedIndex = 0;
	}

	function selectItem(item: T) {
		value = item;
		inputValue = String(item[displayKey]);
		onValueChange(item);
		open = false;
		inputElement?.blur();
	}

	function handleKeydown(e: KeyboardEvent) {
		if (disabled) return;

		switch (e.key) {
			case "ArrowDown":
				e.preventDefault();
				if (!open) {
					open = true;
				} else {
					highlightedIndex = Math.min(
						highlightedIndex + 1,
						filteredItems.length - 1,
					);
				}
				break;
			case "ArrowUp":
				e.preventDefault();
				highlightedIndex = Math.max(highlightedIndex - 1, 0);
				break;
			case "Enter":
				e.preventDefault();
				if (open && filteredItems[highlightedIndex]) {
					selectItem(filteredItems[highlightedIndex]);
				}
				break;
			case "Escape":
				e.preventDefault();
				open = false;
				inputElement?.blur();
				break;
		}
	}

	function handleBlur(e: FocusEvent) {
		// Delay to allow click on dropdown item
		setTimeout(() => {
			if (
				!dropdownElement?.contains(document.activeElement) &&
				document.activeElement !== inputElement
			) {
				open = false;
				// Reset to selected value if no selection was made
				if (value) {
					inputValue = String(value[displayKey]);
				}
			}
		}, 200);
	}

	function handleClickOutside(e: MouseEvent) {
		if (
			!inputElement?.contains(e.target as Node) &&
			!dropdownElement?.contains(e.target as Node)
		) {
			open = false;
			if (value) {
				inputValue = String(value[displayKey]);
			}
		}
	}

	$effect(() => {
		if (open) {
			document.addEventListener("mousedown", handleClickOutside);
			return () => {
				document.removeEventListener("mousedown", handleClickOutside);
			};
		}
	});
</script>

<div class="space-y-2">
	<span class="block text-sm text-foreground-muted">
		{label}
	</span>
	<div class="relative">
		<input
			bind:this={inputElement}
			type="text"
			class="w-full px-4 py-2 pr-10 rounded border border-border bg-surface text-foreground focus:outline-none focus:border-border-strong disabled:opacity-50 disabled:cursor-not-allowed"
			{placeholder}
			{disabled}
			bind:value={inputValue}
			onfocus={handleFocus}
			oninput={handleInput}
			onkeydown={handleKeydown}
			onblur={handleBlur}
			aria-label={label}
			autocomplete="off"
		/>
		<div class="absolute right-3 top-1/2 -translate-y-1/2 pointer-events-none">
			<svg
				class="w-4 h-4 text-foreground-subtle"
				fill="none"
				stroke="currentColor"
				viewBox="0 0 24 24"
			>
				<path
					stroke-linecap="round"
					stroke-linejoin="round"
					stroke-width="2"
					d="M19 9l-7 7-7-7"
				/>
			</svg>
		</div>

		{#if open && filteredItems.length > 0}
			<div
				bind:this={dropdownElement}
				class="absolute z-50 w-full bg-surface border border-border shadow-lg rounded max-h-80 overflow-y-auto mt-2"
				transition:fly={{ y: -8, duration: 200, easing: cubicOut }}
			>
				{#each filteredItems as item, i}
					<button
						type="button"
						class="w-full px-4 py-3 text-left cursor-pointer hover:bg-surface-elevated transition-colors border-b border-border-subtle last:border-b-0"
						class:bg-surface-elevated={i === highlightedIndex}
						onmousedown={(e) => {
							e.preventDefault();
							selectItem(item);
						}}
						onmouseenter={() => {
							highlightedIndex = i;
						}}
					>
						{#if itemSnippet}
							{@render itemSnippet(item)}
						{:else}
							<span class="text-foreground">
								{item[displayKey]}
							</span>
						{/if}
					</button>
				{/each}
			</div>
		{:else if open && filteredItems.length === 0}
			<div
				bind:this={dropdownElement}
				class="absolute z-50 w-full bg-surface border border-border shadow-lg rounded mt-2"
				transition:fly={{ y: -8, duration: 200, easing: cubicOut }}
			>
				<div class="p-4 text-sm text-foreground-subtle text-center">
					No results found
				</div>
			</div>
		{/if}
	</div>
</div>
