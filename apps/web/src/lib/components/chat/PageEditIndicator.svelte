<script lang="ts">
	import Icon from '$lib/components/Icon.svelte';
	import EntityPicker, { type EntityResult } from '$lib/components/EntityPicker.svelte';
	import { slide, fly } from 'svelte/transition';
	import { cubicOut } from 'svelte/easing';

	/**
	 * Resource item that can be edited
	 */
	interface EditableItem {
		type: 'page' | 'folder' | 'wiki_entry';
		id: string;
		title: string;
		icon?: string;
	}

	interface Props {
		/** List of resources the AI can edit */
		items?: EditableItem[];
		/** @deprecated Use items instead - Single bound page for backward compatibility */
		boundPage?: { id: string; title: string } | null;
		/** Called when a specific item is removed */
		onRemoveItem?: (type: string, id: string) => void;
		/** Called when entities are selected from picker */
		onSelectEntities?: (entities: EntityResult[]) => void;
		/** Whether to show the component (hide in chat mode) */
		visible?: boolean;
	}

	let { items = [], boundPage, onRemoveItem, onSelectEntities, visible = true }: Props = $props();

	let expanded = $state(false);
	let showPicker = $state(false);
	let containerEl: HTMLDivElement | null = $state(null);

	// Convert single boundPage to items array for backward compatibility
	const effectiveItems = $derived(() => {
		if (items && items.length > 0) {
			return items;
		}
		if (boundPage) {
			return [{ type: 'page' as const, id: boundPage.id, title: boundPage.title }];
		}
		return [];
	});

	function getIconForType(type: string): string {
		switch (type) {
			case 'page':
				return 'ri:file-text-line';
			case 'folder':
				return 'ri:folder-line';
			case 'wiki_entry':
				return 'ri:book-2-line';
			default:
				return 'ri:file-line';
		}
	}

	function handleToggle(event: MouseEvent) {
		event.stopPropagation();
		expanded = !expanded;
		if (!expanded) {
			showPicker = false;
		}
	}

	function handleRemove(event: MouseEvent, type: string, id: string) {
		event.stopPropagation();
		onRemoveItem?.(type, id);
	}

	function handleAddClick(event: MouseEvent) {
		event.stopPropagation();
		showPicker = true;
	}

	function handlePickerSelect(entity: EntityResult) {
		// Wrap single entity in array for compatibility with onSelectEntities
		onSelectEntities?.([entity]);
		// Don't close picker - keepOpen handles that
	}

	function handlePickerClose() {
		showPicker = false;
	}

	function handleClickOutside(event: MouseEvent) {
		if (!expanded) return;
		const target = event.target as HTMLElement;
		// Don't collapse if clicking inside the container or on picker backdrop/picker
		if (containerEl?.contains(target)) return;
		if (target.closest('.picker-backdrop') || target.closest('.entity-picker')) return;
		expanded = false;
		showPicker = false;
	}

	function handleKeydown(event: KeyboardEvent) {
		if (event.key === 'Escape') {
			if (showPicker) {
				showPicker = false;
			} else if (expanded) {
				expanded = false;
			}
		}
	}

	const itemCount = $derived(effectiveItems().length);
</script>

<svelte:window onclick={handleClickOutside} onkeydown={handleKeydown} />

{#if visible}
<div class="edit-pill" class:expanded class:has-items={itemCount > 0} bind:this={containerEl}>
	<!-- Toggle button - always present, style changes based on expanded state -->
	<button
		class="edit-toggle"
		onclick={(e) => handleToggle(e)}
		type="button"
		title={expanded ? 'Collapse' : (itemCount > 0 ? `AI can edit ${itemCount} item${itemCount === 1 ? '' : 's'}` : 'Add items AI can edit')}
	>
		<Icon icon="ri:edit-line" width="14" />
		{#if !expanded && itemCount > 0}
			<span class="inline-count">{itemCount}</span>
		{/if}
	</button>

	<!-- Expandable content - slides in -->
	{#if expanded}
		<div class="pill-content" transition:slide={{ axis: 'x', duration: 200, easing: cubicOut }}>
			<div class="chips-scroll">
				{#each effectiveItems() as item, i (item.id)}
					<button
						type="button"
						class="edit-chip"
						onclick={(e) => handleRemove(e, item.type, item.id)}
						title="Click to remove"
						in:fly={{ x: -8, duration: 150, delay: 50 + i * 30 }}
					>
						<Icon icon={item.icon || getIconForType(item.type)} width="12" />
						<span class="chip-title">{item.title}</span>
						<Icon icon="ri:close-line" width="10" class="remove-icon" />
					</button>
				{/each}
			</div>
			<button
				type="button"
				class="add-btn"
				onclick={(e) => handleAddClick(e)}
				title="Add editable items"
				in:fly={{ x: -8, duration: 150, delay: 50 + effectiveItems().length * 30 }}
			>
				<Icon icon="ri:add-line" width="14" />
			</button>
		</div>
	{/if}

	{#if showPicker}
		<EntityPicker
			mode="single"
			keepOpen={true}
			excludeIds={effectiveItems().map(i => i.id)}
			placeholder="Search pages to allow editing..."
			onSelect={handlePickerSelect}
			onClose={handlePickerClose}
		/>
	{/if}
</div>
{/if}

<style>
	.edit-pill {
		position: relative;
		display: flex;
		align-items: center;
		border-radius: 100px;
		transition: background-color 150ms ease, border-color 150ms ease;
		border: 1px solid transparent;
	}

	.edit-pill.expanded {
		background: var(--color-surface-elevated);
		border-color: var(--color-border);
	}

	/* Single toggle button - circular when icon-only */
	.edit-toggle {
		display: flex;
		align-items: center;
		gap: 6px;
		height: 28px;
		padding: 0 7px;
		background: var(--color-surface-elevated);
		border: none;
		border-radius: 100px;
		color: var(--color-foreground-muted);
		font-size: 12px;
		font-weight: 500;
		cursor: pointer;
		transition: all 150ms ease;
		flex-shrink: 0;
	}

	.edit-toggle:hover {
		background: var(--color-border);
		color: var(--color-foreground);
	}

	/* Keep muted style even with items - no special highlight */

	/* When expanded, toggle becomes minimal - pill handles the background */
	.edit-pill.expanded .edit-toggle {
		background: transparent;
		padding: 0 6px;
	}

	.edit-pill.expanded .edit-toggle:hover {
		background: var(--color-surface);
	}

	.inline-count {
		font-size: 11px;
		font-weight: 500;
		color: inherit;
	}

	.pill-content {
		display: flex;
		align-items: center;
		gap: 4px;
		padding-right: 4px;
		overflow: hidden;
	}

	/* Scrollable chips container with fade */
	.chips-scroll {
		display: flex;
		align-items: center;
		gap: 4px;
		max-width: 240px;
		overflow-x: auto;
		scrollbar-width: none; /* Firefox */
		-ms-overflow-style: none; /* IE/Edge */
		/* Fade effect on right edge when overflowing */
		mask-image: linear-gradient(to right, black 85%, transparent 100%);
		-webkit-mask-image: linear-gradient(to right, black 85%, transparent 100%);
		padding-right: 8px; /* Extra space for fade */
	}

	.chips-scroll::-webkit-scrollbar {
		display: none; /* Chrome/Safari */
	}

	.edit-chip {
		display: flex;
		align-items: center;
		gap: 4px;
		padding: 3px 6px;
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 100px;
		color: var(--color-foreground);
		font-size: 11px;
		cursor: pointer;
		transition: all 0.15s ease;
		max-width: 100px;
		white-space: nowrap;
		flex-shrink: 0;
	}

	.edit-chip:hover {
		border-color: var(--color-error);
		background: color-mix(in srgb, var(--color-error) 15%, transparent);
	}

	.edit-chip:hover :global(.remove-icon) {
		color: var(--color-error);
	}

	.edit-chip :global(svg) {
		flex-shrink: 0;
		color: var(--color-foreground-muted);
	}

	.chip-title {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.edit-chip :global(.remove-icon) {
		opacity: 0.5;
		transition: all 0.15s ease;
	}

	.edit-chip:hover :global(.remove-icon) {
		opacity: 1;
	}

	.add-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 22px;
		height: 22px;
		background: none;
		border: none;
		border-radius: 50%;
		color: var(--color-foreground-muted);
		cursor: pointer;
		transition: all 0.15s ease;
		flex-shrink: 0;
	}

	.add-btn:hover {
		background: var(--color-primary-subtle);
		color: var(--color-primary);
	}
</style>
