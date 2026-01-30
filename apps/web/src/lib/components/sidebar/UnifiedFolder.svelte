<script lang="ts">
	import { slide } from "svelte/transition";
	import Sortable from "sortablejs";
	import type { SortableEvent, MoveEvent } from "sortablejs";
	import { reorder } from "$lib/utils/useSortable.svelte";
	import Icon from "$lib/components/Icon.svelte";
	import { spaceStore } from "$lib/stores/space.svelte";
	import { persistFolderReorder, type SidebarDndItem } from "$lib/stores/dndManager.svelte";
	import { pagesStore } from "$lib/stores/pages.svelte";
	import { contextMenu } from "$lib/stores/contextMenu.svelte";
	import { updateView, deleteView, addViewItem, removeViewItem } from "$lib/api/client";
	import type { ViewSummary, ViewEntity } from "$lib/api/client";
	import type { ContextMenuItem } from "$lib/stores/contextMenu.svelte";
	import SidebarNavItem from "./SidebarNavItem.svelte";
	// Self-import for recursive sub-folder rendering (replaces deprecated <svelte:self>)
	import UnifiedFolder from "./UnifiedFolder.svelte";

	const ANIMATION_DURATION_MS = 150;

	interface Props {
		view: ViewSummary;
		collapsed?: boolean;
		/** Nesting depth (0 = top-level, 1 = sub-folder). Max depth is 1. */
		depth?: number;
		/** When true, auto-focus rename input on mount */
		autoFocusRename?: boolean;
		/** Callback when rename focus is consumed */
		onRenameFocusConsumed?: () => void;
	}

	let {
		view,
		collapsed = false,
		depth = 0,
		autoFocusRename = false,
		onRenameFocusConsumed
	}: Props = $props();

	// Build class list based on depth for indentation and sub-folder styling
	const depthClasses = $derived.by(() => {
		const classes: string[] = [];
		if (depth === 1) classes.push('sidebar-interactive--indent-1', 'sidebar-interactive--sub-folder');
		else if (depth >= 2) classes.push('sidebar-interactive--indent-2', 'sidebar-interactive--sub-folder');
		return classes.join(' ');
	});

	// Local expanded state
	let isExpanded = $state(false);

	// Rename state
	let isRenaming = $state(false);
	let renameValue = $state("");
	let renameInputEl: HTMLInputElement | null = $state(null);

	// Visual states for drag feedback (set by CSS via parent's onMove)
	let isExpandPending = $state(false);
	let isDragOver = $state(false);

	// DnD zone items - transformed from viewEntities for SortableJS
	interface FolderDndItem extends SidebarDndItem {
		entity: ViewEntity;
		sourceSpaceId?: string; // Track source for cross-zone drops
		sourceViewId?: string; // Track source folder for folder-to-folder drops
		sourceIsSmartView?: boolean; // True if dragged from a smart view (copy semantics)
	}
	let dndItems = $state<FolderDndItem[]>([]);

	// SortableJS instance ref
	let sortableInstance: Sortable | null = null;
	let folderListEl = $state<HTMLElement | null>(null);
	let folderEl = $state<HTMLElement | null>(null);

	// Track original item URLs for detecting new drops
	let originalItemUrls = $state<Set<string>>(new Set());


	// Auto-focus rename when requested (for newly created views)
	$effect(() => {
		if (autoFocusRename && !isRenaming) {
			isRenaming = true;
			renameValue = view.name;
			onRenameFocusConsumed?.();
		}
	});

	// Focus input when entering rename mode
	$effect(() => {
		if (isRenaming && renameInputEl) {
			renameInputEl.focus();
			renameInputEl.select();
		}
	});

	// Resolved view entities
	let viewEntities = $state<ViewEntity[]>([]);
	let viewLoading = $state(false);
	let lastCacheVersion = $state(-1);

	// Load view entities when expanded or when cache is invalidated
	$effect(() => {
		const currentVersion = spaceStore.viewCacheVersion;
		if (isExpanded && (lastCacheVersion !== currentVersion)) {
			// Force refresh if cache was invalidated (version changed from known value)
			const forceRefresh = lastCacheVersion !== -1;
			loadViewEntities(currentVersion, forceRefresh);
		}
	});

	async function loadViewEntities(cacheVersion: number, forceRefresh = false) {
		// If already loading, wait for current load to complete then trigger another if needed
		if (viewLoading) {
			// Queue a reload after current finishes
			setTimeout(() => loadViewEntities(cacheVersion, true), 100);
			return;
		}

		viewLoading = true;
		lastCacheVersion = cacheVersion;

		try {
			viewEntities = await spaceStore.resolveView(view.id, forceRefresh);
			// Transform to DnD items - use entity URL as stable ID
			// Mark items from smart views so destinations know to use copy semantics
			const isSmartView = view.view_type === "smart";
			dndItems = viewEntities.map((entity) => ({
				id: getHrefForEntity(entity),
				url: getHrefForEntity(entity),
				label: entity.name,
				icon: entity.icon,
				entity,
				sourceViewId: view.id, // Track source folder for folder-to-folder drops
				sourceIsSmartView: isSmartView // Smart view items use copy semantics when dragged out
			}));
			// Track original items for detecting new drops
			originalItemUrls = new Set(dndItems.map(item => item.url));
		} catch (e) {
			console.error("[UnifiedFolder] Failed to load view entities:", e);
		} finally {
			viewLoading = false;
		}
	}

	function toggleExpanded() {
		isExpanded = !isExpanded;
	}

	function handleClick(e: MouseEvent) {
		e.preventDefault();
		e.stopPropagation();
		if (!isRenaming) {
			toggleExpanded();
		}
	}

	function handleContextMenu(e: MouseEvent) {
		e.preventDefault();
		e.stopPropagation();

		const items: ContextMenuItem[] = [];

		// Creation options (non-system folders only)
		if (!view.is_system) {
			items.push({
				id: "new-chat",
				label: "New Chat",
				icon: "ri:chat-1-line",
				action: handleNewChat,
			});
			items.push({
				id: "new-page",
				label: "New Page",
				icon: "ri:file-text-line",
				action: handleNewPage,
			});
		}

		// Rename (non-system only)
		if (!view.is_system) {
			items.push({
				id: "rename",
				label: "Rename",
				icon: "ri:edit-line",
				action: startRename,
				dividerBefore: true,
			});
		}

		// Delete (non-system only)
		if (!view.is_system) {
			items.push({
				id: "delete",
				label: "Delete",
				icon: "ri:delete-bin-line",
				variant: "destructive",
				action: handleDelete,
			});
		}

		if (items.length > 0) {
			contextMenu.show({ x: e.clientX, y: e.clientY }, items);
		}
	}

	function startRename() {
		if (view.is_system) return;
		isRenaming = true;
		renameValue = view.name;
	}

	async function handleRenameSubmit() {
		const trimmed = renameValue.trim();
		if (!trimmed || trimmed === view.name) {
			handleRenameCancel();
			return;
		}

		try {
			await updateView(view.id, { name: trimmed });
			await spaceStore.refreshViews();
		} catch (e) {
			console.error("[UnifiedFolder] Failed to rename view:", e);
		}

		isRenaming = false;
		renameValue = "";
	}

	function handleRenameCancel() {
		isRenaming = false;
		renameValue = "";
	}

	function handleRenameKeydown(e: KeyboardEvent) {
		if (e.key === "Enter") {
			e.preventDefault();
			handleRenameSubmit();
		} else if (e.key === "Escape") {
			e.preventDefault();
			handleRenameCancel();
		}
	}

	async function handleDelete() {
		if (view.is_system) return;

		try {
			await deleteView(view.id);
			await spaceStore.refreshViews();
		} catch (e) {
			console.error("[UnifiedFolder] Failed to delete view:", e);
		}
	}

	// Create a new chat and add it to this folder
	async function handleNewChat() {
		// Open new chat tab
		spaceStore.openTabFromRoute("/", {
			label: "New Chat",
			forceNew: true,
			preferEmptyPane: true,
		});
		// Note: We don't add to folder immediately since the chat doesn't have an ID yet
		// User can add it later via context menu once the chat has been created
	}

	// Create a new page and add it to this folder
	async function handleNewPage() {
		try {
			const page = await pagesStore.createNewPage();
			const pageRoute = `/page/${page.id}`;

			// Add the new page to this folder
			await addViewItem(view.id, pageRoute);
			spaceStore.invalidateViewCache();

			// Open the page in a tab
			spaceStore.openTabFromRoute(pageRoute, {
				label: page.title,
				forceNew: true,
				preferEmptyPane: true,
			});
		} catch (e) {
			console.error("[UnifiedFolder] Failed to create page:", e);
		}
	}

	// ============================================================================
	// SortableJS Integration
	// ============================================================================

	// Initialize SortableJS when folder is expanded
	$effect(() => {
		if (isExpanded && folderListEl && !sortableInstance) {
			sortableInstance = initSortable(folderListEl);
		}

		// Cleanup when collapsed or destroyed
		return () => {
			if (sortableInstance) {
				sortableInstance.destroy();
				sortableInstance = null;
			}
		};
	});

	// Clean up any stuck DnD visual state (ghost elements, classes, etc.)
	// Uses a small delay to let SortableJS finish its own cleanup first
	function cleanupStuckDndState() {
		requestAnimationFrame(() => {
			// Remove stuck classes from all elements - don't remove elements, just classes
			document.querySelectorAll('.sidebar-ghost').forEach(el => {
				el.classList.remove('sidebar-ghost');
			});
			document.querySelectorAll('.sidebar-chosen, .sidebar-dragging').forEach(el => {
				el.classList.remove('sidebar-chosen', 'sidebar-dragging');
			});
			document.querySelectorAll('.expand-pending').forEach(el => {
				el.classList.remove('expand-pending');
			});
			// Only remove sortable-fallback elements (these are definitely SortableJS artifacts)
			document.querySelectorAll('.sortable-fallback').forEach(el => {
				el.remove();
			});
		});
	}

	function initSortable(el: HTMLElement): Sortable {
		const isSmartView = view.view_type === 'smart';

		return Sortable.create(el, {
			group: {
				name: 'sidebar',
				pull: isSmartView ? 'clone' : true, // Copy from smart view, move from manual
				put: !isSmartView // Cannot drop INTO smart views
			},
			animation: ANIMATION_DURATION_MS,
			fallbackOnBody: true,
			swapThreshold: 0.65,
			emptyInsertThreshold: 20, // Allow drops into empty folders
			filter: '.sidebar-empty', // Don't allow dragging the empty message
			ghostClass: 'sidebar-ghost',
			chosenClass: 'sidebar-chosen',
			dragClass: 'sidebar-dragging',

			// Block folders being dropped into folders + freeze reordering over collapsed folders
			onMove(evt: MoveEvent) {
				const draggedIsFolder = evt.dragged.hasAttribute('data-is-folder');
				if (draggedIsFolder && depth >= 1) {
					return false; // Block folder nesting
				}
				// Also freeze reordering when hovering over collapsed sub-folders (for hover-to-expand)
				const related = evt.related as HTMLElement | null;
				const folderEl = related?.closest('[data-folder-id]') as HTMLElement | null
					|| related?.querySelector('[data-folder-id]') as HTMLElement | null;
				if (folderEl) {
					const isFolderExpanded = folderEl.classList.contains('expanded');
					const isSmartView = folderEl.classList.contains('smart-view');
					if (!isFolderExpanded && !isSmartView) {
						return false; // Freeze to allow hover-to-expand
					}
				}
				return true;
			},

			// Handle items ADDED from another list (cross-zone drops TO this folder)
			async onAdd(evt: SortableEvent) {
				try {
					// Remove the DOM element SortableJS added - we'll reload from API
					evt.item.remove();
					await handleCrossZoneDrop(evt);
					// Reload folder contents to show the new item
					await loadViewEntities(spaceStore.viewCacheVersion, true);
				} catch (error) {
					console.error("[UnifiedFolder] Error in onAdd:", error);
					// On error, invalidate cache and reload to reset state
					spaceStore.invalidateViewCache();
					await loadViewEntities(spaceStore.viewCacheVersion, true);
				} finally {
					// Always cleanup stuck visual state
					cleanupStuckDndState();
				}
			},

			// Handle reorder within this folder only
			async onEnd(evt: SortableEvent) {
				try {
					// Only handle same-zone reorders here
					if (evt.from === evt.to) {
						await handleDragEnd(evt);
					}
					// Cross-zone drops are handled by onAdd
				} catch (error) {
					console.error("[UnifiedFolder] Error in onEnd:", error);
				} finally {
					// Always cleanup stuck visual state
					cleanupStuckDndState();
				}
			}
		});
	}

	async function handleDragEnd(evt: SortableEvent) {
		// Block drops into smart views
		if (view.view_type === 'smart' && evt.from !== evt.to) {
			await loadViewEntities(spaceStore.viewCacheVersion, true);
			return;
		}

		const sameZone = evt.from === evt.to;

		// Capture rollback state
		const rollbackItems = [...dndItems];
		const rollbackEntities = [...viewEntities];
		const rollbackOriginalUrls = new Set(originalItemUrls);

		try {
			if (sameZone) {
				// Simple reorder within this folder
				dndItems = reorder(dndItems, evt);
				viewEntities = dndItems.map(item => item.entity);
				await persistFolderReorder(dndItems, view.id);
			} else {
				// Cross-zone drop - item came from another zone
				await handleCrossZoneDrop(evt);
			}
		} catch (error) {
			console.error("[UnifiedFolder] Failed to handle drop, rolling back:", error);
			dndItems = rollbackItems;
			viewEntities = rollbackEntities;
			originalItemUrls = rollbackOriginalUrls;
			spaceStore.invalidateViewCache();
		}
	}

	async function handleCrossZoneDrop(evt: SortableEvent) {
		// Get the dropped item's data from the DOM element
		const droppedEl = evt.item;
		const itemUrl = droppedEl.getAttribute('data-url');
		const sourceViewId = droppedEl.getAttribute('data-source-view-id');
		const sourceSpaceId = droppedEl.getAttribute('data-source-space-id');
		const sourceIsSmartView = droppedEl.getAttribute('data-source-smart-view') === 'true';

		if (!itemUrl) {
			console.warn('[UnifiedFolder] Cross-zone drop missing item URL');
			return;
		}

		// Block folders being dropped into folders
		const isFolder = droppedEl.getAttribute('data-is-folder') === 'true';
		if (isFolder) {
			console.warn("[UnifiedFolder] Cannot drop folder into folder");
			return;
		}

		// PHASE 1: Add item to this folder
		await addViewItem(view.id, itemUrl);

		// PHASE 2: Remove from source (only for move operations, not smart view copies)
		if (!sourceIsSmartView) {
			if (sourceSpaceId) {
				await spaceStore.removeSpaceItem(itemUrl, sourceSpaceId);
			} else if (sourceViewId && sourceViewId !== view.id) {
				await removeViewItem(sourceViewId, itemUrl);
			}
		}

		// Update original items set
		originalItemUrls.add(itemUrl);

		// PHASE 3: Invalidate cache (reload happens in finally block of caller)
		spaceStore.invalidateViewCache();
	}

	// Get appropriate icon
	const folderIcon = $derived.by(() => {
		if (view.icon) return view.icon;
		// Smart views get filter icon, manual views get folder icon
		return view.view_type === "smart" ? "ri:filter-line" : "ri:folder-line";
	});

	// Display name
	const displayName = $derived(view.name || "Untitled Folder");

	// Get icon for entity
	function getIconForEntity(entity: ViewEntity): string {
		return entity.icon || "ri:file-line";
	}

	// Get href for entity - ensures it's always a full path
	// Backend should return "/namespace/id" format, but be defensive
	function getHrefForEntity(entity: ViewEntity): string {
		// If already a full path, use as-is
		if (entity.id.startsWith('/')) {
			return entity.id;
		}
		// Otherwise construct from namespace and id
		return `/${entity.namespace}/${entity.id}`;
	}

	// Extract raw ID from entity (strips path prefix if present)
	// Used for API calls where we need just the ID, not the full path
	function getRawId(entity: ViewEntity): string {
		if (entity.id.startsWith('/')) {
			// Entity ID is a full path like "/view/view_xxx" - extract just the ID
			const parts = entity.id.split('/').filter(Boolean);
			return parts[parts.length - 1]; // Last part is the actual ID
		}
		return entity.id;
	}

	// Listen for expand-folder custom event from parent (during drag hover)
	$effect(() => {
		const el = folderEl;
		if (!el) return;

		function handleExpandFolder() {
			if (!isExpanded && view.view_type !== 'smart') {
				isExpanded = true;
			}
		}

		el.addEventListener('expandfolder', handleExpandFolder);
		return () => el.removeEventListener('expandfolder', handleExpandFolder);
	});
</script>

<div
	bind:this={folderEl}
	class="unified-folder"
	class:collapsed
	class:expanded={isExpanded}
	class:smart-view={view.view_type === "smart"}
	data-folder-id={view.id}
>
	{#if collapsed}
		<!-- Collapsed mode: show nothing (handled by parent) -->
	{:else}
		<!-- Folder header -->
		<button
			class="sidebar-interactive {depthClasses}"
			class:renaming={isRenaming}
			class:system={view.is_system}
			class:expand-pending={isExpandPending}
			class:smart-view={view.view_type === "smart"}
			class:drop-target={isDragOver}
			onclick={handleClick}
			oncontextmenu={handleContextMenu}
		>
			{#if !view.is_system}
				<Icon icon={folderIcon} width="16" class="sidebar-icon" />
			{/if}
			{#if isRenaming}
				<!-- svelte-ignore a11y_autofocus -->
				<input
					type="text"
					class="sidebar-rename-input"
					bind:this={renameInputEl}
					bind:value={renameValue}
					onkeydown={handleRenameKeydown}
					onblur={handleRenameSubmit}
					onclick={(e) => e.stopPropagation()}
				/>
			{:else}
				<span class="sidebar-label">{displayName}</span>
			{/if}

			{#if !isRenaming}
				{#if viewLoading}
					<span class="sidebar-spinner">...</span>
				{/if}

				<svg
					class="sidebar-chevron"
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

		<!-- Folder contents - SortableJS handles drag-and-drop when expanded -->
		{#if isExpanded}
			<div class="sidebar-expandable-content" transition:slide={{ duration: 150 }}>
				{#if viewLoading}
					<div class="sidebar-expandable-inner" role="region">
						<div class="sidebar-loading">Loading...</div>
					</div>
				{:else}
					<!-- SortableJS zone - bind:this for initialization -->
					<div
						class="sidebar-expandable-inner sidebar-dnd-zone indented"
						class:empty={dndItems.length === 0}
						role="list"
						data-view-id={view.id}
						bind:this={folderListEl}
					>
					{#if dndItems.length === 0}
						<div class="sidebar-empty">
							{#if view.view_type === "smart"}
								No matches
							{:else}
								Empty
							{/if}
						</div>
					{:else}
					{#each dndItems as item (item.id)}
						<div
							class="sidebar-dnd-item"
							role="listitem"
							data-url={item.url}
							data-is-folder={item.entity?.namespace === 'view' ? 'true' : null}
							data-source-view-id={view.id}
							data-source-smart-view={view.view_type === 'smart' ? 'true' : null}
						>
							{#if item.entity?.namespace === 'view'}
								<!-- Sub-folder: render recursively (max depth enforced by backend) -->
								<UnifiedFolder
									view={{
										id: getRawId(item.entity),
										space_id: view.space_id,
										name: item.entity.name,
										icon: item.entity.icon,
										sort_order: 0,
										view_type: 'manual',
										is_system: false
									}}
									{collapsed}
									depth={depth + 1}
								/>
							{:else if item.entity}
								<!-- Regular item -->
								<SidebarNavItem
									item={{
										id: item.entity.id,
										type: "link",
										label: item.entity.name,
										href: item.id,
										icon: getIconForEntity(item.entity),
									}}
									{collapsed}
									indent={1}
									inFolderContext={{
										viewId: view.id,
										isSystemFolder: view.is_system,
									}}
								/>
							{/if}
						</div>
					{/each}
					{/if}
				</div>
				{/if}
			</div>
		{/if}

	{/if}
</div>

<style>
	@reference "../../../app.css";
	@reference "$lib/styles/sidebar.css";
	/* Base icon styles are in sidebar.css (globally imported in app.css) */

	.unified-folder {
		display: flex;
		flex-direction: column;
	}

	.unified-folder.collapsed {
		display: none;
	}

	/* Slide transition wrapper */
	.sidebar-expandable-content {
		overflow: hidden;
	}

	/* Empty drop zone styling */
	.sidebar-dnd-zone.empty {
		min-height: 32px;
		border: 1px dashed var(--color-border-subtle, rgba(128, 128, 128, 0.2));
		border-radius: 4px;
		margin: 4px 8px;
	}

	/* Drop indicator styling is in sidebar.css */
	/* svelte-dnd-action adds data-is-dnd-shadow-item-hint to shadow items */

	/* Sub-folder and indent styles are in sidebar.css */
</style>
