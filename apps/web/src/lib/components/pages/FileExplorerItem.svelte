<script lang="ts">
	import { workspaceStore } from "$lib/stores/workspace.svelte";
	import { pagesStore } from "$lib/stores/pages.svelte";
	// Note: This component uses a legacy TreeNode type, not the new ExplorerTreeNode
	import "iconify-icon";
	import FileExplorerItem from "./FileExplorerItem.svelte";

	// Legacy TreeNode type for backwards compatibility
	// TODO: Migrate to ExplorerTreeNode
	interface TreeNode {
		id: string;
		type: 'folder' | 'page';
		name: string;
		children?: TreeNode[];
	}

	interface Props {
		node: TreeNode;
		depth?: number;
		collapsed?: boolean;
		animationDelay?: number;
	}

	let { node, depth = 0, collapsed = false, animationDelay = 0 }: Props = $props();

	let isExpanded = $state(false);
	let isRenaming = $state(false);
	let renameValue = $state("");
	let renameInput: HTMLInputElement | null = $state(null);

	let isDragOver = $state(false);

	function toggleExpand(e: MouseEvent) {
		e.stopPropagation();
		isExpanded = !isExpanded;
	}

	function handleClick(e: MouseEvent) {
		if (node.type === "folder") {
			toggleExpand(e);
		} else {
			const forceNew = e.metaKey || e.ctrlKey;
			workspaceStore.openTabFromRoute(`/pages/${node.id}`, {
				forceNew,
				label: node.title,
				preferEmptyPane: true,
			});
		}
	}

	async function handleRename(e: KeyboardEvent | FocusEvent) {
		if (e instanceof KeyboardEvent && e.key !== "Enter" && e.key !== "Escape") return;
		
		if (e instanceof KeyboardEvent && e.key === "Escape") {
			isRenaming = false;
			return;
		}

		const newName = renameValue.trim();
		if (newName && ((node.type === "folder" && newName !== node.name) || (node.type === "page" && newName !== node.title))) {
			if (node.type === "folder") {
				// TODO: Folders are now managed via explorer_nodes - use workspaceStore.renameNode
				console.warn('[FileExplorerItem] Folder rename not implemented in new system');
			} else {
				await pagesStore.renamePage(node.id, newName);
			}
		}
		isRenaming = false;
	}

	function startRename() {
		isRenaming = true;
		renameValue = node.type === "folder" ? node.name : node.title;
		setTimeout(() => renameInput?.focus(), 0);
	}

	// DnD Handlers
	function onDragStart(e: DragEvent) {
		if (isRenaming) {
			e.preventDefault();
			return;
		}
		e.dataTransfer?.setData("application/virtues-node", JSON.stringify({ id: node.id, type: node.type }));
		if (e.dataTransfer) e.dataTransfer.effectAllowed = "move";
	}

	function onDragOver(e: DragEvent) {
		if (node.type === "folder") {
			e.preventDefault();
			isDragOver = true;
			if (e.dataTransfer) e.dataTransfer.dropEffect = "move";
		}
	}

	function onDragLeave() {
		isDragOver = false;
	}

	async function onDrop(e: DragEvent) {
		if (node.type !== "folder") return;
		e.preventDefault();
		isDragOver = false;

		const data = e.dataTransfer?.getData("application/virtues-node");
		if (!data) return;

		const dragged = JSON.parse(data);
		if (dragged.id === node.id) return; // Can't drop on self

		if (dragged.type === "page") {
			await pagesStore.movePage(dragged.id, node.id);
		} else {
			await pagesStore.moveFolder(dragged.id, node.id);
		}
	}

	const icon = $derived(node.type === "folder" 
		? (isExpanded ? "ri:folder-open-line" : "ri:folder-line")
		: "ri:file-text-line"
	);

	const label = $derived(node.type === "folder" ? node.name : node.title);
	
	const isActive = $derived.by(() => {
		if (node.type === "folder") return false;
		const activeTabs = workspaceStore.getActiveTabsForSidebar();
		return activeTabs.some(t => t.route === `/pages/${node.id}`);
	});
</script>

<div class="node-wrapper" style="--depth: {depth}">
	<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
	<div
		class="node-item"
		class:active={isActive}
		class:drag-over={isDragOver}
		class:collapsed
		draggable="true"
		onclick={handleClick}
		ondragstart={onDragStart}
		ondragover={onDragOver}
		ondragleave={onDragLeave}
		ondrop={onDrop}
		title={label}
		role="treeitem"
		aria-selected={isActive}
		tabindex="0"
		onkeydown={(e) => e.key === 'Enter' && handleClick()}
	>
		{#if node.type === "folder"}
			<button class="chevron" onclick={toggleExpand} class:expanded={isExpanded} aria-label={isExpanded ? "Collapse folder" : "Expand folder"}>
				<iconify-icon icon="ri:arrow-right-s-line" width="14"></iconify-icon>
			</button>
		{:else}
			<div class="spacer"></div>
		{/if}

		<iconify-icon {icon} width="16" class="node-icon"></iconify-icon>

		{#if !collapsed}
			{#if isRenaming}
				<input
					bind:this={renameInput}
					bind:value={renameValue}
					onkeydown={handleRename}
					onblur={handleRename}
					class="rename-input"
				/>
			{:else}
				<span class="node-label">{label}</span>
			{/if}
		{/if}
	</div>

	{#if node.type === "folder" && isExpanded && !collapsed}
		<div class="children">
			{#each node.children as child}
				<FileExplorerItem node={child} depth={depth + 1} {collapsed} />
			{/each}
		</div>
	{/if}
</div>

<style>
	@reference "../../../app.css";

	.node-wrapper {
		display: flex;
		flex-direction: column;
	}

	.node-item {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 4px 8px;
		padding-left: calc(8px + (var(--depth) * 12px));
		border-radius: 6px;
		cursor: pointer;
		color: var(--color-foreground-muted);
		font-size: 13px;
		transition: all 150ms var(--ease-premium);
		user-select: none;
		height: 28px;
	}

	.node-item:hover {
		background: color-mix(in srgb, var(--color-foreground) 5%, transparent);
		color: var(--color-foreground);
	}

	.node-item.active {
		background: color-mix(in srgb, var(--color-foreground) 8%, transparent);
		color: var(--color-foreground);
		font-weight: 500;
	}

	.node-item.drag-over {
		background: var(--color-primary-subtle);
		outline: 2px solid var(--color-primary);
	}

	.chevron {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 16px;
		height: 16px;
		border: none;
		background: transparent;
		color: var(--color-foreground-subtle);
		cursor: pointer;
		padding: 0;
		transition: transform 150ms var(--ease-premium);
	}

	.chevron.expanded {
		transform: rotate(90deg);
	}

	.spacer {
		width: 16px;
	}

	.node-icon {
		flex-shrink: 0;
		color: var(--color-foreground-subtle);
	}

	.node-item:hover .node-icon,
	.node-item.active .node-icon {
		color: var(--color-foreground);
	}

	.node-label {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		flex: 1;
	}

	.rename-input {
		background: var(--surface-elevated);
		border: 1px solid var(--color-primary);
		border-radius: 4px;
		color: var(--color-foreground);
		font-size: 13px;
		padding: 0 4px;
		width: 100%;
		height: 20px;
		outline: none;
	}

	.children {
		display: flex;
		flex-direction: column;
	}
</style>
