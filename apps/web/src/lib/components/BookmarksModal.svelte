<script lang="ts">
	import 'iconify-icon';
	import { bookmarks, type Bookmark } from '$lib/stores/bookmarks.svelte';
	import { workspaceStore } from '$lib/stores/workspace.svelte';

	interface Props {
		open?: boolean;
		onClose: () => void;
	}

	let { open = false, onClose }: Props = $props();

	let searchQuery = $state('');
	let selectedIndex = $state(0);
	let inputEl: HTMLInputElement | null = $state(null);

	// Filter and group bookmarks
	const filteredBookmarks = $derived.by(() => {
		const query = searchQuery.toLowerCase().trim();
		let items = bookmarks.bookmarks;

		if (query) {
			items = items.filter((b) => b.label.toLowerCase().includes(query));
		}

		// Group by type
		const tabs = items.filter((b) => b.bookmark_type === 'tab');
		const entities = items.filter((b) => b.bookmark_type === 'entity');

		return { tabs, entities };
	});

	// Total results count for keyboard navigation
	const totalResults = $derived(filteredBookmarks.tabs.length + filteredBookmarks.entities.length);

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			e.preventDefault();
			onClose();
		} else if (e.key === 'ArrowDown') {
			e.preventDefault();
			selectedIndex = Math.min(selectedIndex + 1, totalResults - 1);
		} else if (e.key === 'ArrowUp') {
			e.preventDefault();
			selectedIndex = Math.max(selectedIndex - 1, 0);
		} else if (e.key === 'Enter') {
			e.preventDefault();
			navigateToSelected();
		}
	}

	function navigateToSelected() {
		const allItems = [...filteredBookmarks.tabs, ...filteredBookmarks.entities];
		const item = allItems[selectedIndex];
		if (item) {
			navigateToBookmark(item);
		}
	}

	function navigateToBookmark(bookmark: Bookmark) {
		if (bookmark.bookmark_type === 'tab' && bookmark.route) {
			workspaceStore.openTabFromRoute(bookmark.route, { label: bookmark.label });
		} else if (bookmark.bookmark_type === 'entity' && bookmark.entity_slug) {
			workspaceStore.openTabFromRoute(`/wiki/${bookmark.entity_slug}`, { label: bookmark.label });
		}
		onClose();
	}

	async function removeBookmark(e: MouseEvent, id: string) {
		e.stopPropagation();
		await bookmarks.removeBookmark(id);
	}

	function handleBackdropClick(e: MouseEvent) {
		if (e.target === e.currentTarget) {
			onClose();
		}
	}

	function getEntityIcon(entityType: string | null): string {
		switch (entityType) {
			case 'person':
				return 'ri:user-line';
			case 'place':
				return 'ri:map-pin-line';
			case 'organization':
				return 'ri:building-line';
			case 'thing':
				return 'ri:box-3-line';
			default:
				return 'ri:bookmark-line';
		}
	}

	// Focus input when modal opens
	$effect(() => {
		if (open && inputEl) {
			inputEl.focus();
			searchQuery = '';
			selectedIndex = 0;
		}
	});
</script>

{#if open}
	<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
	<div class="modal-backdrop" onclick={handleBackdropClick}>
		<div class="modal" role="dialog" aria-modal="true" aria-label="Bookmarks">
			<!-- Search Input -->
			<div class="search-input-container">
				<iconify-icon icon="ri:bookmark-line" width="18" class="search-icon"></iconify-icon>
				<input
					bind:this={inputEl}
					bind:value={searchQuery}
					onkeydown={handleKeydown}
					type="text"
					placeholder="Search bookmarks..."
					class="search-input"
				/>
				<kbd class="escape-hint">Esc</kbd>
			</div>

			<!-- Results -->
			<div class="results">
				{#if filteredBookmarks.tabs.length > 0}
					<div class="result-group">
						<span class="group-label">Pages</span>
						{#each filteredBookmarks.tabs as bookmark, i}
							<div
								class="result-item"
								class:selected={selectedIndex === i}
								role="button"
								tabindex="0"
								onclick={() => navigateToBookmark(bookmark)}
								onkeydown={(e) => e.key === 'Enter' && navigateToBookmark(bookmark)}
								onmouseenter={() => (selectedIndex = i)}
							>
								<iconify-icon
									icon={bookmark.icon || 'ri:file-line'}
									width="16"
									class="result-icon"
								></iconify-icon>
								<span class="result-label">{bookmark.label}</span>
								<button
									class="remove-btn"
									onclick={(e) => removeBookmark(e, bookmark.id)}
									aria-label="Remove bookmark"
								>
									<iconify-icon icon="ri:close-line" width="14"></iconify-icon>
								</button>
							</div>
						{/each}
					</div>
				{/if}

				{#if filteredBookmarks.entities.length > 0}
					<div class="result-group">
						<span class="group-label">Wiki Entities</span>
						{#each filteredBookmarks.entities as bookmark, i}
							{@const index = filteredBookmarks.tabs.length + i}
							<div
								class="result-item"
								class:selected={selectedIndex === index}
								role="button"
								tabindex="0"
								onclick={() => navigateToBookmark(bookmark)}
								onkeydown={(e) => e.key === 'Enter' && navigateToBookmark(bookmark)}
								onmouseenter={() => (selectedIndex = index)}
							>
								<iconify-icon
									icon={bookmark.icon || getEntityIcon(bookmark.entity_type)}
									width="16"
									class="result-icon"
								></iconify-icon>
								<span class="result-label">{bookmark.label}</span>
								<span class="entity-type">{bookmark.entity_type}</span>
								<button
									class="remove-btn"
									onclick={(e) => removeBookmark(e, bookmark.id)}
									aria-label="Remove bookmark"
								>
									<iconify-icon icon="ri:close-line" width="14"></iconify-icon>
								</button>
							</div>
						{/each}
					</div>
				{/if}

				{#if totalResults === 0}
					<div class="no-results">
						{#if searchQuery}
							<span>No bookmarks found for "{searchQuery}"</span>
						{:else}
							<iconify-icon icon="ri:bookmark-line" width="32" class="empty-icon"></iconify-icon>
							<span>No bookmarks yet</span>
							<span class="hint">Right-click a tab to bookmark it</span>
						{/if}
					</div>
				{/if}
			</div>
		</div>
	</div>
{/if}

<style>
	@reference "../../app.css";

	.modal-backdrop {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.4);
		display: flex;
		align-items: flex-start;
		justify-content: center;
		padding-top: 15vh;
		z-index: 9999;
		animation: backdrop-fade-in 150ms ease-out;
	}

	@keyframes backdrop-fade-in {
		from {
			opacity: 0;
		}
		to {
			opacity: 1;
		}
	}

	.modal {
		width: 100%;
		max-width: 520px;
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: 12px;
		box-shadow: 0 16px 48px rgba(0, 0, 0, 0.2);
		overflow: hidden;
		animation: modal-slide-in 150ms ease-out;
	}

	@keyframes modal-slide-in {
		from {
			opacity: 0;
			transform: translateY(-8px) scale(0.98);
		}
		to {
			opacity: 1;
			transform: translateY(0) scale(1);
		}
	}

	.search-input-container {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 14px 16px;
		border-bottom: 1px solid var(--border);
	}

	.search-icon {
		color: var(--foreground-muted);
		flex-shrink: 0;
	}

	.search-input {
		flex: 1;
		border: none;
		background: transparent;
		font-size: 15px;
		color: var(--foreground);
		outline: none;
	}

	.search-input::placeholder {
		color: var(--foreground-subtle);
	}

	.escape-hint {
		font-family: var(--font-mono);
		font-size: 10px;
		padding: 3px 6px;
		background: var(--surface-elevated);
		border-radius: 4px;
		color: var(--foreground-subtle);
	}

	.results {
		max-height: 400px;
		overflow-y: auto;
		padding: 8px;
	}

	.result-group {
		margin-bottom: 8px;
	}

	.group-label {
		display: block;
		font-size: 11px;
		font-weight: 500;
		text-transform: uppercase;
		letter-spacing: 0.02em;
		color: var(--foreground-subtle);
		padding: 6px 8px;
	}

	.result-item {
		display: flex;
		align-items: center;
		gap: 10px;
		width: 100%;
		padding: 10px 12px;
		border-radius: 8px;
		cursor: pointer;
		background: transparent;
		border: none;
		text-align: left;
		color: var(--foreground);
		transition: background-color 80ms ease-out;
	}

	.result-item:hover,
	.result-item.selected {
		background: var(--surface-overlay);
	}

	.result-item.selected {
		background: var(--primary-subtle);
	}

	.result-icon {
		color: var(--foreground-muted);
		flex-shrink: 0;
	}

	.result-label {
		flex: 1;
		font-size: 14px;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.entity-type {
		font-size: 11px;
		padding: 2px 6px;
		background: var(--surface-elevated);
		border-radius: 4px;
		color: var(--foreground-subtle);
		text-transform: capitalize;
	}

	.remove-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 20px;
		height: 20px;
		padding: 0;
		border: none;
		background: transparent;
		border-radius: 4px;
		color: var(--foreground-subtle);
		cursor: pointer;
		opacity: 0;
		transition:
			opacity 100ms ease-out,
			background-color 100ms ease-out;
	}

	.result-item:hover .remove-btn,
	.result-item.selected .remove-btn {
		opacity: 1;
	}

	.remove-btn:hover {
		background: var(--surface-elevated);
		color: var(--foreground);
	}

	.no-results {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 8px;
		padding: 32px 16px;
		text-align: center;
		color: var(--foreground-subtle);
		font-size: 14px;
	}

	.empty-icon {
		color: var(--foreground-subtle);
		opacity: 0.5;
	}

	.hint {
		font-size: 12px;
		color: var(--foreground-subtle);
		opacity: 0.7;
	}
</style>
