<script lang="ts">
	/**
	 * EntityPicker - Unified entity search and selection component
	 *
	 * Used for:
	 * - Page @ mentions (single select, insert markdown link)
	 * - Chat @ mentions (single select, add context reference)
	 * - Edit allow list (multi select, add to permissions)
	 * - Sidebar "Add..." (single select, add entity/URL to workspace)
	 * - Page editor link button (single select, apply link mark)
	 *
	 * Supports both internal entity search and external URL detection.
	 * When the search input looks like a URL, a synthetic "Link" result
	 * appears at the top of results.
	 *
	 * Uses: GET /api/pages/search/entities?q={query}
	 * Uses the floating UI system for dismiss handling when positioned.
	 */

	import Icon from '$lib/components/Icon.svelte';
	import { onMount } from 'svelte';
	import { fade } from 'svelte/transition';
	import { FloatingContent, useClickOutside, useEscapeKey } from '$lib/floating';
	import type { VirtualAnchor } from '$lib/floating';

	/** Entity result from the API */
	export interface EntityResult {
		id: string;
		name: string;
		entity_type: string;
		icon: string;
		url: string;
		mime_type?: string;
	}

	interface FooterAction {
		label: string;
		icon: string;
		action: () => void;
		variant?: 'default' | 'destructive';
	}

	interface Props {
		/** Selection mode */
		mode?: 'single' | 'multi';
		/** Called when an entity is selected (single mode) */
		onSelect?: (entity: EntityResult) => void;
		/** Called when selection is confirmed (multi mode) */
		onSelectMultiple?: (entities: EntityResult[]) => void;
		/** Called when picker is closed */
		onClose: () => void;
		/** Filter to specific entity types */
		entityTypes?: string[];
		/** IDs to exclude from results (already selected) */
		excludeIds?: string[];
		/** Position for fixed positioning (optional) */
		position?: { x: number; y: number };
		/** Placeholder text */
		placeholder?: string;
		/** Keep picker open after single select (for adding multiple items) */
		keepOpen?: boolean;
		/** Pre-fill the search input (e.g. for editing an existing link) */
		initialQuery?: string;
		/** Optional action button in the footer (e.g. "Remove Link") */
		footerAction?: FooterAction;
	}

	let {
		mode = 'single',
		onSelect,
		onSelectMultiple,
		onClose,
		entityTypes,
		excludeIds = [],
		position,
		placeholder = 'Search entities...',
		keepOpen = false,
		initialQuery = '',
		footerAction
	}: Props = $props();

	// URL detection helpers
	function isUrlLike(text: string): boolean {
		return /^https?:\/\//i.test(text) || /^[a-zA-Z0-9]([a-zA-Z0-9-]*[a-zA-Z0-9])?(\.[a-zA-Z]{2,})/.test(text);
	}

	function normalizeUrl(text: string): string {
		return /^https?:\/\//i.test(text) ? text : `https://${text}`;
	}

	let query = $state(initialQuery);
	let results = $state<EntityResult[]>([]);
	let selectedIndex = $state(0);
	let selectedItems = $state<EntityResult[]>([]);
	let inputEl: HTMLInputElement | null = $state(null);
	let pickerEl: HTMLDivElement | null = $state(null);
	let isLoading = $state(false);

	// Convert position to virtual anchor for Floating UI (when position is provided)
	const virtualAnchor = $derived<VirtualAnchor | null>(
		position ? { x: position.x, y: position.y, width: 0, height: 0 } : null
	);

	// Use hooks for dismiss behavior (wrap callbacks to capture current values)
	useClickOutside(
		() => [pickerEl],
		() => onClose(),
		() => true
	);
	useEscapeKey(() => onClose(), () => true);

	// Fetch results when query changes
	$effect(() => {
		fetchResults(query);
	});

	async function fetchResults(q: string) {
		isLoading = true;
		try {
			const response = await fetch(`/api/pages/search/entities?q=${encodeURIComponent(q)}`);
			if (response.ok) {
				const data = await response.json();
				let items: EntityResult[] = data.results || [];

				// Filter by entity types if specified
				if (entityTypes && entityTypes.length > 0) {
					items = items.filter((item) => entityTypes.includes(item.entity_type));
				}

				// Exclude already selected items
				if (excludeIds.length > 0) {
					items = items.filter((item) => !excludeIds.includes(item.id));
				}

				results = items;
				selectedIndex = 0;
			}
		} catch (e) {
			console.error('EntityPicker fetch error:', e);
		} finally {
			isLoading = false;
		}
	}

	// Synthetic URL result when query looks like a URL
	const urlResult = $derived.by((): EntityResult | null => {
		const trimmed = query.trim();
		if (!trimmed || !isUrlLike(trimmed)) return null;
		return {
			id: `url:${normalizeUrl(trimmed)}`,
			name: trimmed,
			entity_type: 'url',
			icon: 'ri:link',
			url: normalizeUrl(trimmed),
		};
	});

	// Flat list for keyboard navigation (URL result prepended if present)
	const flatResults = $derived.by(() => {
		if (urlResult) return [urlResult, ...results];
		return results;
	});

	// Group results by entity type for display
	const groupedResults = $derived.by(() => {
		const groups: Record<string, EntityResult[]> = {};
		if (urlResult) {
			groups['url'] = [urlResult];
		}
		for (const item of results) {
			const type = item.entity_type;
			if (!groups[type]) groups[type] = [];
			groups[type].push(item);
		}
		return groups;
	});

	onMount(() => {
		inputEl?.focus();
	});

	function handleKeydown(e: KeyboardEvent) {
		// Only handle events if focus is within the picker or on its input
		const target = e.target as HTMLElement;
		const isWithinPicker = pickerEl?.contains(target) || target === inputEl;
		if (!isWithinPicker) return;

		// Escape is handled by useEscapeKey hook
		if (e.key === 'ArrowDown') {
			e.preventDefault();
			selectedIndex = Math.min(selectedIndex + 1, flatResults.length - 1);
		} else if (e.key === 'ArrowUp') {
			e.preventDefault();
			selectedIndex = Math.max(selectedIndex - 1, 0);
		} else if (e.key === 'Enter') {
			e.preventDefault();
			const item = flatResults[selectedIndex];
			if (item) {
				handleItemClick(item);
			}
		}
	}

	function handleItemClick(item: EntityResult) {
		if (mode === 'single') {
			onSelect?.(item);
			if (!keepOpen) {
				onClose();
			}
		} else {
			// Toggle selection in multi mode
			const idx = selectedItems.findIndex((i) => i.id === item.id);
			if (idx >= 0) {
				selectedItems = selectedItems.filter((i) => i.id !== item.id);
			} else {
				selectedItems = [...selectedItems, item];
			}
		}
	}

	function isSelected(item: EntityResult): boolean {
		return selectedItems.some((i) => i.id === item.id);
	}

	function handleConfirm() {
		if (selectedItems.length > 0) {
			onSelectMultiple?.(selectedItems);
		}
		onClose();
	}

	function getTypeLabel(type: string): string {
		const labels: Record<string, string> = {
			url: 'Link',
			page: 'Pages',
			person: 'People',
			place: 'Places',
			file: 'Files',
			org: 'Organizations'
		};
		return labels[type] || type;
	}
</script>

<svelte:window on:keydown={handleKeydown} />

{#snippet pickerContent()}
	<!-- Search input -->
	<div class="search-row">
		<Icon icon="ri:search-line" width="14" />
		<input
			bind:this={inputEl}
			bind:value={query}
			type="text"
			{placeholder}
			class="search-input"
		/>
		{#if isLoading}
			<Icon icon="ri:loader-4-line" width="14" />
		{/if}
	</div>

	<!-- Results -->
	<div class="results">
		{#if flatResults.length === 0}
			<div class="empty">
				{query ? 'No results found' : 'Type to search...'}
			</div>
		{:else}
			{#each Object.entries(groupedResults) as [type, items]}
				<div class="type-group">
					<div class="type-header">{getTypeLabel(type)}</div>
					{#each items as item}
						{@const globalIndex = flatResults.indexOf(item)}
						<button
							class="result-item"
							class:selected={globalIndex === selectedIndex}
							class:checked={mode === 'multi' && isSelected(item)}
							onclick={() => handleItemClick(item)}
							onmouseenter={() => (selectedIndex = globalIndex)}
							type="button"
						>
							{#if mode === 'multi'}
								<div class="checkbox" class:checked={isSelected(item)}>
									{#if isSelected(item)}
										<Icon icon="ri:check-line" width="12" />
									{/if}
								</div>
							{/if}
							<Icon icon={item.icon} width="14" />
							<span class="item-name">{item.name}</span>
						</button>
					{/each}
				</div>
			{/each}
		{/if}
	</div>

	<!-- Footer -->
	<div class="footer">
		{#if mode === 'multi' && selectedItems.length > 0}
			<span class="selection-count">{selectedItems.length} selected</span>
			<button type="button" class="confirm-btn" onclick={handleConfirm}>
				Add selected
			</button>
		{:else}
			<span class="hint"><kbd>↑↓</kbd> navigate</span>
			<span class="hint"><kbd>↵</kbd> select</span>
			<span class="hint"><kbd>esc</kbd> close</span>
			{#if footerAction}
				<button
					type="button"
					class="footer-action-btn"
					class:destructive={footerAction.variant === 'destructive'}
					onclick={footerAction.action}
				>
					<Icon icon={footerAction.icon} width="14" />
					{footerAction.label}
				</button>
			{/if}
		{/if}
	</div>
{/snippet}

{#if virtualAnchor}
	<!-- Fixed positioning mode (when position prop is provided) -->
	<FloatingContent
		anchor={virtualAnchor}
		options={{ placement: 'bottom-start', offset: 4, flip: true, shift: true, padding: 8 }}
		class="entity-picker-container"
	>
		<div
			bind:this={pickerEl}
			class="entity-picker floating"
			transition:fade={{ duration: 100 }}
		>
			{@render pickerContent()}
		</div>
	</FloatingContent>
{:else}
	<!-- Absolute positioning mode (default, relative to parent) -->
	<div
		bind:this={pickerEl}
		class="entity-picker"
		transition:fade={{ duration: 100 }}
	>
		{@render pickerContent()}
	</div>
{/if}

<style>
	/* FloatingContent wrapper styles */
	:global(.entity-picker-container) {
		--z-floating: 101;
		padding: 0;
		background: transparent;
		border: none;
		box-shadow: none;
	}

	.entity-picker {
		position: absolute;
		bottom: 100%;
		left: 0;
		margin-bottom: 8px;
		width: 300px;
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 8px;
		box-shadow: 0 4px 20px rgba(0, 0, 0, 0.15);
		z-index: 101;
		overflow: hidden;
	}

	.entity-picker.floating {
		position: static;
		margin-bottom: 0;
	}

	.search-row {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 10px 12px;
		border-bottom: 1px solid var(--color-border);
		color: var(--color-foreground-muted);
	}

	.search-input {
		flex: 1;
		border: none;
		background: none;
		outline: none;
		font-size: 13px;
		color: var(--color-foreground);
	}

	.search-input::placeholder {
		color: var(--color-foreground-subtle);
	}

	.results {
		max-height: 280px;
		overflow-y: auto;
	}

	.empty {
		padding: 20px;
		text-align: center;
		color: var(--color-foreground-muted);
		font-size: 13px;
	}

	.type-group {
		border-bottom: 1px solid var(--color-border);
	}

	.type-group:last-child {
		border-bottom: none;
	}

	.type-header {
		padding: 6px 12px;
		font-size: 11px;
		font-weight: 500;
		color: var(--color-foreground-muted);
		background: var(--color-surface-elevated);
		text-transform: uppercase;
		letter-spacing: 0.5px;
	}

	.result-item {
		display: flex;
		align-items: center;
		gap: 8px;
		width: 100%;
		padding: 8px 12px;
		border: none;
		background: none;
		text-align: left;
		cursor: pointer;
		color: var(--color-foreground);
		font-size: 13px;
		transition: background-color 0.1s;
	}

	.result-item:hover,
	.result-item.selected {
		background: var(--color-primary-subtle);
	}

	.result-item.checked {
		background: var(--color-primary-subtle);
	}

	.result-item :global(svg) {
		color: var(--color-foreground-muted);
		flex-shrink: 0;
	}

	.checkbox {
		width: 16px;
		height: 16px;
		border: 1px solid var(--color-border);
		border-radius: 3px;
		display: flex;
		align-items: center;
		justify-content: center;
		background: var(--color-surface);
		flex-shrink: 0;
	}

	.checkbox.checked {
		background: var(--color-primary);
		border-color: var(--color-primary);
		color: white;
	}

	.item-name {
		flex: 1;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.footer {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 8px 12px;
		border-top: 1px solid var(--color-border);
		background: var(--color-surface-elevated);
	}

	.hint {
		font-size: 11px;
		color: var(--color-foreground-subtle);
	}

	.hint kbd {
		display: inline-block;
		padding: 1px 4px;
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 3px;
		font-family: inherit;
		font-size: 10px;
	}

	.selection-count {
		flex: 1;
		font-size: 12px;
		color: var(--color-foreground-muted);
	}

	.confirm-btn {
		padding: 6px 12px;
		font-size: 12px;
		background: var(--color-primary);
		border: none;
		border-radius: 4px;
		color: white;
		cursor: pointer;
		transition: opacity 0.15s ease;
	}

	.confirm-btn:hover {
		opacity: 0.9;
	}

	.footer-action-btn {
		display: flex;
		align-items: center;
		gap: 4px;
		margin-left: auto;
		padding: 4px 8px;
		font-size: 11px;
		background: none;
		border: none;
		border-radius: 4px;
		color: var(--color-foreground-muted);
		cursor: pointer;
		transition: all 0.15s ease;
	}

	.footer-action-btn:hover {
		background: var(--color-surface);
		color: var(--color-foreground);
	}

	.footer-action-btn.destructive {
		color: var(--color-error);
	}

	.footer-action-btn.destructive:hover {
		background: color-mix(in srgb, var(--color-error) 10%, transparent);
	}
</style>
