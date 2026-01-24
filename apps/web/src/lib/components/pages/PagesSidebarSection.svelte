<!--
	PagesSidebarSection.svelte

	Navigation for user-authored pages in the sidebar.
	Uses a hierarchical tree view with folders and pages.
-->

<script lang="ts">
	import { onMount } from "svelte";
	import { workspaceStore } from "$lib/stores/workspace.svelte";
	import { pagesStore } from "$lib/stores/pages.svelte";
	import FileExplorerItem from "./FileExplorerItem.svelte";
	import "iconify-icon";

	interface Props {
		collapsed?: boolean;
		baseAnimationDelay?: number;
	}

	let { collapsed = false, baseAnimationDelay = 0 }: Props = $props();

	onMount(async () => {
		await pagesStore.init();
	});

	async function handleNewPage() {
		const page = await pagesStore.createNewPage();
		workspaceStore.openTabFromRoute(`/pages/${page.id}`, {
			label: page.title,
			preferEmptyPane: true,
		});
	}

	async function handleNewFolder() {
		await pagesStore.createNewFolder();
	}

	let isRootDragOver = $state(false);

	function onDragOver(e: DragEvent) {
		e.preventDefault();
		isRootDragOver = true;
		if (e.dataTransfer) e.dataTransfer.dropEffect = "move";
	}

	function onDragLeave() {
		isRootDragOver = false;
	}

	async function onDrop(e: DragEvent) {
		e.preventDefault();
		isRootDragOver = false;

		const data = e.dataTransfer?.getData("application/virtues-node");
		if (!data) return;

		const dragged = JSON.parse(data);
		if (dragged.type === "page") {
			await pagesStore.movePage(dragged.id, null);
		} else {
			await pagesStore.moveFolder(dragged.id, null);
		}
	}
</script>

<div class="pages-section">
	{#if !collapsed}
		<div class="section-actions">
			<button onclick={handleNewPage} title="New Page" class="action-btn">
				<iconify-icon icon="ri:add-line" width="14"></iconify-icon>
			</button>
			<button onclick={handleNewFolder} title="New Folder" class="action-btn">
				<iconify-icon icon="ri:folder-add-line" width="14"></iconify-icon>
			</button>
		</div>
	{/if}

	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div 
		class="explorer-tree" 
		class:drag-over={isRootDragOver}
		ondragover={onDragOver}
		ondragleave={onDragLeave}
		ondrop={onDrop}
	>
		{#if pagesStore.loading && pagesStore.tree.length === 0}
			<div class="loading-state">Loading...</div>
		{:else if pagesStore.tree.length === 0}
			<div class="empty-state">No pages yet</div>
		{:else}
			{#each pagesStore.tree as node}
				<FileExplorerItem {node} {collapsed} depth={0} />
			{/each}
		{/if}
	</div>
</div>

<style>
	@reference "../../../app.css";

	.pages-section {
		display: flex;
		flex-direction: column;
		gap: 4px;
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

	.explorer-tree {
		display: flex;
		flex-direction: column;
		min-height: 20px;
		border-radius: 6px;
		transition: background-color 150ms var(--ease-premium);
	}

	.explorer-tree.drag-over {
		background: var(--color-primary-subtle);
		outline: 2px dashed var(--color-primary);
	}

	.loading-state, .empty-state {
		padding: 8px 12px;
		font-size: 12px;
		color: var(--color-foreground-subtle);
		font-style: italic;
	}
</style>
