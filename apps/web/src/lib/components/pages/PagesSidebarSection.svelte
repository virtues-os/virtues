<!--
	PagesSidebarSection.svelte

	Quick access to pinned and recent pages in the sidebar.
	Shows pinned items first, then recent items to fill remaining slots.
-->

<script lang="ts">
	import { onMount } from "svelte";
	import { spaceStore } from "$lib/stores/space.svelte";
	import { pagesStore } from "$lib/stores/pages.svelte";
	import Icon from "$lib/components/Icon.svelte";

	interface Props {
		collapsed?: boolean;
		baseAnimationDelay?: number;
	}

	let { collapsed = false, baseAnimationDelay = 0 }: Props = $props();

	// Get quick access items (pinned + recent)
	const sidebarItems = $derived(pagesStore.getSidebarItems(8));
	const loading = $derived(pagesStore.pagesLoading);

	onMount(async () => {
		// Load pages so getSidebarItems has data to work with
		await pagesStore.loadPages();
	});

	async function handleNewPage() {
		const page = await pagesStore.createNewPage();
		pagesStore.addPage(page);
		pagesStore.markAsRecent(page.id);
		spaceStore.openTabFromRoute(`/page/${page.id}`, {
			label: page.title,
			preferEmptyPane: true,
		});
	}

	function handlePageClick(pageId: string, title: string, e: MouseEvent) {
		pagesStore.markAsRecent(pageId);
		const forceNew = e.metaKey || e.ctrlKey;
		spaceStore.openTabFromRoute(`/page/${pageId}`, {
			forceNew,
			label: title,
			preferEmptyPane: true,
		});
	}

	function handleViewAll(e: MouseEvent) {
		const forceNew = e.metaKey || e.ctrlKey;
		spaceStore.openTabFromRoute('/page', {
			forceNew,
			label: 'All Pages',
			preferEmptyPane: true,
		});
	}

	function handleContextMenu(e: MouseEvent, pageId: string) {
		e.preventDefault();
		// Toggle pin on right-click for now (could add proper context menu later)
		pagesStore.togglePin(pageId);
	}

	function getPageIcon(page: { icon: string | null }): string {
		return page.icon || 'ri:file-text-line';
	}
</script>

<div class="pages-section">
	{#if !collapsed}
		<div class="section-actions">
			<button onclick={handleNewPage} title="New Page" class="action-btn">
				<Icon icon="ri:add-line" width="14"/>
			</button>
		</div>
	{/if}

	<div class="pages-list">
		{#if loading && sidebarItems.length === 0}
			<div class="loading-state">Loading...</div>
		{:else if sidebarItems.length === 0}
			<div class="empty-state">No pages yet</div>
		{:else}
			{#each sidebarItems as page (page.id)}
				{@const isPinned = pagesStore.isPinned(page.id)}
				<button
					class="page-item"
					class:collapsed
					onclick={(e) => handlePageClick(page.id, page.title, e)}
					oncontextmenu={(e) => handleContextMenu(e, page.id)}
					title={collapsed ? page.title : (isPinned ? 'Pinned • Right-click to unpin' : 'Right-click to pin')}
				>
					<Icon
						icon={isPinned ? 'ri:pushpin-fill' : getPageIcon(page)}
						width="14"
						class="page-icon {isPinned ? 'pinned' : ''}"
					/>
					{#if !collapsed}
						<span class="page-title">{page.title}</span>
					{/if}
				</button>
			{/each}
		{/if}
	</div>

	{#if !collapsed}
		<button class="view-all-link" onclick={handleViewAll}>
			<Icon icon="ri:arrow-right-s-line" width="14"/>
			<span>View All Pages</span>
		</button>
	{/if}
</div>

<style>
	@reference "../../../app.css";

	.pages-section {
		display: flex;
		flex-direction: column;
		gap: 2px;
		min-height: 40px;
	}

	.section-actions {
		display: flex;
		justify-content: flex-end;
		gap: 4px;
		padding: 0 8px;
		margin-bottom: 4px;
	}

	.action-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 24px;
		height: 24px;
		border-radius: 4px;
		border: none;
		background: transparent;
		color: var(--color-foreground-muted);
		cursor: pointer;
		transition: all 150ms var(--ease-premium);
	}

	.action-btn:hover {
		background: color-mix(in srgb, var(--color-foreground) 7%, transparent);
		color: var(--color-foreground);
	}

	.pages-list {
		display: flex;
		flex-direction: column;
		gap: 1px;
	}

	.page-item {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 6px 12px;
		border: none;
		background: transparent;
		color: var(--color-foreground-muted);
		font-size: 13px;
		text-align: left;
		cursor: pointer;
		border-radius: 6px;
		transition: all 150ms var(--ease-premium);
		width: 100%;
	}

	.page-item:hover {
		background: color-mix(in srgb, var(--color-foreground) 5%, transparent);
		color: var(--color-foreground);
	}

	.page-item.collapsed {
		justify-content: center;
		padding: 6px;
	}

	.page-item :global(.page-icon) {
		flex-shrink: 0;
		opacity: 0.6;
	}

	.page-item :global(.page-icon.pinned) {
		color: var(--color-primary);
		opacity: 1;
	}

	.page-item:hover :global(.page-icon) {
		opacity: 1;
	}

	.page-title {
		flex: 1;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.view-all-link {
		display: flex;
		align-items: center;
		gap: 4px;
		padding: 6px 12px;
		margin-top: 4px;
		border: none;
		background: transparent;
		color: var(--color-foreground-subtle);
		font-size: 12px;
		cursor: pointer;
		border-radius: 6px;
		transition: all 150ms var(--ease-premium);
	}

	.view-all-link:hover {
		background: color-mix(in srgb, var(--color-foreground) 5%, transparent);
		color: var(--color-foreground-muted);
	}

	.loading-state, .empty-state {
		padding: 8px 12px;
		font-size: 12px;
		color: var(--color-foreground-subtle);
		font-style: italic;
	}
</style>
