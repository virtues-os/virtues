<script lang="ts">
	import "iconify-icon";
	import { workspaceStore, getRouteFromEntityId, getEntityTypeFromId } from "$lib/stores/workspace.svelte";
	import type { ExplorerTreeNode, ViewEntity } from "$lib/api/client";
	import SidebarNavItem from "./SidebarNavItem.svelte";

	interface Props {
		node: ExplorerTreeNode;
		depth?: number;
		collapsed?: boolean;
	}

	let { node, depth = 0, collapsed = false }: Props = $props();

	// Local expanded state (simpler than store-based for now)
	let isExpanded = $state(false);

	// Resolved view entities (if this is a view node)
	let viewEntities = $state<ViewEntity[]>([]);
	let viewLoading = $state(false);
	let viewLoadAttempted = $state(false);  // Prevent infinite retry loop

	// Load view entities when expanded (only once)
	$effect(() => {
		if (node.node_type === "view" && isExpanded && !viewLoadAttempted) {
			loadViewEntities();
		}
	});

	async function loadViewEntities() {
		if (viewLoading || viewLoadAttempted) return;  // Guard against multiple calls
		
		viewLoading = true;
		viewLoadAttempted = true;  // Mark as attempted regardless of outcome
		
		try {
			viewEntities = await workspaceStore.resolveViewNode(node);
		} catch (e) {
			console.error("[ExplorerNode] Failed to load view entities:", e);
			// Don't retry - viewLoadAttempted stays true
		} finally {
			viewLoading = false;
		}
	}

	function toggleExpanded() {
		isExpanded = !isExpanded;
	}

	function handleNodeClick(e: MouseEvent) {
		e.preventDefault();
		e.stopPropagation();
		
		try {
			if (node.node_type === "shortcut" && node.entity_id) {
				// Check if this is a route link (entity_id starts with "route:")
				if (node.entity_id.startsWith("route:")) {
					const route = node.entity_id.slice(6); // Remove "route:" prefix
					workspaceStore.openTabFromRoute(route, { label: node.name || route });
				} else {
					// Open the entity in a tab
					const name = node.name || node.entity_id;
					workspaceStore.openEntityTab(node.entity_id, name);
				}
			} else if (node.node_type === "folder" || node.node_type === "view") {
				// Toggle expansion
				toggleExpanded();
			}
		} catch (err) {
			console.error("[ExplorerNode] Click handler error:", err);
		}
	}

	// Get appropriate icon
	const nodeIcon = $derived.by(() => {
		if (node.icon) return node.icon;
		if (node.node_type === "folder") return isExpanded ? "ri:folder-open-line" : "ri:folder-line";
		if (node.node_type === "view") return "ri:filter-line";
		if (node.node_type === "shortcut" && node.entity_id) {
			return getEntityTypeFromId(node.entity_id).icon;
		}
		return "ri:file-line";
	});

	// Get display name
	const displayName = $derived(node.name || node.entity_id || "Untitled");

	// Indentation based on depth
	const indent = $derived(depth * 12);
</script>

<div class="explorer-node" class:collapsed style="--indent: {indent}px">
	{#if collapsed}
		<!-- Collapsed mode: show nothing (handled by parent) -->
	{:else}
		<!-- Node header -->
		<button
			class="node-header"
			class:expandable={node.node_type !== "shortcut"}
			class:expanded={isExpanded}
			onclick={handleNodeClick}
			style="padding-left: calc(10px + var(--indent))"
		>
			<iconify-icon icon={nodeIcon} width="14" class="node-icon"></iconify-icon>
			<span class="node-name">{displayName}</span>

			{#if node.node_type === "view" && viewLoading}
				<iconify-icon
					icon="ri:loader-4-line"
					width="12"
					class="loading-spinner"
				></iconify-icon>
			{/if}

			{#if node.node_type !== "shortcut"}
				<svg
					class="chevron"
					class:expanded={isExpanded}
					width="10"
					height="10"
					viewBox="0 0 12 12"
					fill="none"
				>
					<path
						d="M4.5 3L7.5 6L4.5 9"
						stroke="currentColor"
						stroke-width="1.5"
						stroke-linecap="round"
						stroke-linejoin="round"
					/>
				</svg>
			{/if}
		</button>

		<!-- Children (for folders) -->
		{#if node.node_type === "folder" && node.children && node.children.length > 0}
			<div class="node-children" class:expanded={isExpanded}>
				<div class="children-inner">
					{#each node.children as child}
						<svelte:self
							node={child}
							depth={depth + 1}
							{collapsed}
						/>
					{/each}
				</div>
			</div>
		{/if}

		<!-- View entities (for views) -->
		{#if node.node_type === "view"}
			<div class="node-children" class:expanded={isExpanded}>
				<div class="children-inner">
					{#if viewLoading}
						<div class="view-loading" style="padding-left: calc(10px + {indent + 12}px)">
							Loading...
						</div>
					{:else if viewEntities.length === 0 && isExpanded}
						<div class="view-empty" style="padding-left: calc(10px + {indent + 12}px)">
							No items
						</div>
					{:else}
					{#each viewEntities as entity}
						<SidebarNavItem
							item={{
								id: entity.id,
								type: "link",
								label: entity.name,
								href: getRouteFromEntityId(entity.id),
								icon: entity.icon,
								pagespace: entity.id,
							}}
							{collapsed}
							indent={depth + 1}
						/>
					{/each}
					{/if}
				</div>
			</div>
		{/if}
	{/if}
</div>

<style>
	@reference "../../../app.css";

	:root {
		--ease-premium: cubic-bezier(0.2, 0, 0, 1);
	}

	@keyframes spin {
		from {
			transform: rotate(0deg);
		}
		to {
			transform: rotate(360deg);
		}
	}

	.explorer-node {
		display: contents;
	}

	.explorer-node.collapsed {
		display: none;
	}

	.node-header {
		display: flex;
		align-items: center;
		gap: 6px;
		width: 100%;
		padding: 5px 10px;
		border-radius: 6px;
		background: transparent;
		color: var(--color-foreground-muted);
		font-size: 13px;
		text-align: left;
		cursor: pointer;
		border: none;
		transition:
			background-color 150ms ease-out,
			color 150ms ease-out;
	}

	.node-header:hover {
		background: color-mix(in srgb, var(--color-foreground) 7%, transparent);
		color: var(--color-foreground);
	}

	.node-header:active {
		background: color-mix(in srgb, var(--color-foreground) 10%, transparent);
	}

	.chevron {
		flex-shrink: 0;
		margin-left: auto;
		color: var(--color-foreground-subtle);
		opacity: 0.5;
		transition: transform 150ms ease-out, opacity 150ms ease-out;
	}

	.node-header:hover .chevron {
		opacity: 1;
	}

	.chevron.expanded {
		transform: rotate(90deg);
		opacity: 0.8;
	}

	.node-icon {
		flex-shrink: 0;
		color: var(--color-foreground-subtle);
	}

	.node-header:hover .node-icon {
		color: var(--color-foreground-muted);
	}

	.node-name {
		flex: 1;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.loading-spinner {
		color: var(--color-foreground-subtle);
		animation: spin 1s linear infinite;
	}

	.node-children {
		display: grid;
		grid-template-rows: 0fr;
		transition: grid-template-rows 150ms ease-out;
		overflow: hidden;
	}

	.node-children.expanded {
		grid-template-rows: 1fr;
	}

	.children-inner {
		min-height: 0;
		overflow: hidden;
	}

	.view-loading,
	.view-empty {
		padding: 6px 10px;
		font-size: 12px;
		color: var(--color-foreground-subtle);
	}
</style>
