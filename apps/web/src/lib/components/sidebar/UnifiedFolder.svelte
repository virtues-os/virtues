<script lang="ts">
	import Sortable from "sortablejs";
	import type { SortableEvent, MoveEvent } from "sortablejs";
	import { reorder } from "$lib/utils/useSortable.svelte";
	import Icon from "$lib/components/Icon.svelte";
	import { spaceStore } from "$lib/stores/space.svelte";
	import {
		persistFolderReorder,
		type SidebarDndItem,
	} from "$lib/stores/dndManager.svelte";
	import { pagesStore } from "$lib/stores/pages.svelte";
	import { contextMenu } from "$lib/stores/contextMenu.svelte";
	import {
		updateView,
		deleteView,
		addViewItem,
		removeViewItem,
	} from "$lib/api/client";
	import { iconPickerStore } from "$lib/stores/iconPicker.svelte";
	import { isEmoji } from "$lib/utils/iconHelpers";
	import type { ViewSummary, ViewEntity } from "$lib/api/client";
	import type { ContextMenuItem } from "$lib/stores/contextMenu.svelte";
	import SidebarNavItem from "./SidebarNavItem.svelte";

	const ANIMATION_DURATION_MS = 150;

	interface Props {
		view: ViewSummary;
		collapsed?: boolean;
		/** When true, auto-focus rename input on mount */
		autoFocusRename?: boolean;
		/** Callback when rename focus is consumed */
		onRenameFocusConsumed?: () => void;
		/** Workspace accent color — passed to child items */
		accentColor?: string | null;
	}

	let {
		view,
		collapsed = false,
		autoFocusRename = false,
		onRenameFocusConsumed,
		accentColor = null,
	}: Props = $props();

	// No nesting — folders are always at top level within their section

	const isExpanded = $derived(spaceStore.isViewExpanded(view.id));

	// Extract namespace from query_config (for system folder context menu actions)
	function getFolderNamespace(): string | null {
		if (!view.query_config) return null;
		try {
			const config = JSON.parse(view.query_config);
			return config.namespace ?? null;
		} catch {
			return null;
		}
	}

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

	// Eagerly seed from prefetch cache so the slide transition starts with
	// the correct content height (prevents stutter from items popping in mid-slide)
	{
		const cached = spaceStore.viewCache.get(view.id);
		if (cached) {
			applyEntities(cached, spaceStore.viewCacheVersion);
		}
	}

	// Load view entities when expanded or when cache is invalidated.
	// $effect.pre runs BEFORE DOM update, so dndItems is populated before
	// the {#if isExpanded} block renders — slide transition measures correct height.
	$effect.pre(() => {
		const currentVersion = spaceStore.viewCacheVersion;
		if (isExpanded && lastCacheVersion !== currentVersion) {
			// Force refresh if cache was invalidated (version changed from known value)
			const forceRefresh = lastCacheVersion !== -1;
			loadViewEntities(currentVersion, forceRefresh);
		}
	});

	async function loadViewEntities(
		cacheVersion: number,
		forceRefresh = false,
	) {
		// If already loading, wait for current load to complete then trigger another if needed
		if (viewLoading) {
			// Queue a reload after current finishes
			setTimeout(() => loadViewEntities(cacheVersion, true), 100);
			return;
		}

		// Check cache synchronously — skip loading flash if already warm
		if (!forceRefresh) {
			const cached = spaceStore.viewCache.get(view.id);
			if (cached) {
				applyEntities(cached, cacheVersion);
				return;
			}
		}

		viewLoading = true;
		lastCacheVersion = cacheVersion;

		try {
			const entities = await spaceStore.resolveView(
				view.id,
				forceRefresh,
			);
			applyEntities(entities, cacheVersion);
		} catch (e) {
			console.error("[UnifiedFolder] Failed to load view entities:", e);
		} finally {
			viewLoading = false;
		}
	}

	function applyEntities(entities: ViewEntity[], cacheVersion: number) {
		lastCacheVersion = cacheVersion;
		viewEntities = entities;
		const isSmartView = view.view_type === "smart";
		dndItems = entities.map((entity) => ({
			id: getHrefForEntity(entity),
			url: getHrefForEntity(entity),
			label: entity.name,
			icon: entity.icon,
			entity,
			sourceViewId: view.id,
			sourceIsSmartView: isSmartView,
		}));
		originalItemUrls = new Set(dndItems.map((item) => item.url));
	}

	function toggleExpanded() {
		spaceStore.toggleViewExpanded(view.id);
	}

	function handleClick(e: MouseEvent) {
		e.preventDefault();
		e.stopPropagation();
		if (!isRenaming) {
			toggleExpanded();
		}
	}

	function handleMoreClick(e: MouseEvent) {
		e.preventDefault();
		e.stopPropagation();
		handleContextMenu(e);
	}

	// Quick-add for system folders (Chats/Pages)
	function handleQuickAdd(e: MouseEvent) {
		e.preventDefault();
		e.stopPropagation();
		const ns = getFolderNamespace();
		if (ns === 'chat') handleNewChat();
		else if (ns === 'page') handleNewPage();
	}

	const hasQuickAdd = $derived(view.is_system && ['chat', 'page'].includes(getFolderNamespace() ?? ''));

	function handleContextMenu(e: MouseEvent) {
		e.preventDefault();
		e.stopPropagation();

		const items: ContextMenuItem[] = [];

		// Open Folder — always available (all folder types)
		items.push({
			id: "open-folder",
			label: "Open Folder",
			icon: "ri:folder-open-line",
			action: () => {
				spaceStore.openTabFromRoute(`/view/${view.id}`, {
					label: view.name || "Folder",
				});
			},
		});

		// Creation options
		if (view.is_system) {
			// System folders: contextual creation based on namespace
			const ns = getFolderNamespace();
			if (ns === 'chat') {
				items.push({
					id: "new-chat",
					label: "New Chat",
					icon: "ri:chat-1-line",
					shortcut: "⌘N",
					action: handleNewChat,
				});
			} else if (ns === 'page') {
				items.push({
					id: "new-page",
					label: "New Page",
					icon: "ri:file-text-line",
					shortcut: "⌘⇧N",
					action: handleNewPage,
				});
			}
		} else {
			// User folders: offer both creation options
			items.push({
				id: "new-chat",
				label: "New Chat",
				icon: "ri:chat-1-line",
				shortcut: "⌘N",
				action: handleNewChat,
			});
			items.push({
				id: "new-page",
				label: "New Page",
				icon: "ri:file-text-line",
				shortcut: "⌘⇧N",
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

		// Change Icon (non-system only)
		if (!view.is_system) {
			items.push({
				id: "change-icon",
				label: "Change Icon",
				icon: "ri:emotion-line",
				action: () => {
					iconPickerStore.show(view.icon ?? null, async (icon) => {
						try {
							await updateView(view.id, {
								icon: icon ?? undefined,
							});
							spaceStore.invalidateViewCache();
						} catch (err) {
							console.error(
								"[UnifiedFolder] Failed to change icon:",
								err,
							);
						}
					});
				},
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

			// Open the page in a tab immediately
			spaceStore.openTabFromRoute(pageRoute, {
				label: page.title,
				forceNew: true,
				preferEmptyPane: true,
			});

			// Add the new page to this folder (non-blocking for tab)
			await addViewItem(view.id, pageRoute);
			spaceStore.invalidateViewCache();
		} catch (e) {
			console.error("[UnifiedFolder] Failed to create page:", e);
		}
	}

	// ============================================================================
	// SortableJS Integration
	// ============================================================================

	// Initialize SortableJS when folder is expanded (skip for system views)
	$effect(() => {
		if (view.is_system) return;
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
			document.querySelectorAll(".sidebar-ghost").forEach((el) => {
				el.classList.remove("sidebar-ghost");
			});
			document
				.querySelectorAll(".sidebar-chosen, .sidebar-dragging")
				.forEach((el) => {
					el.classList.remove("sidebar-chosen", "sidebar-dragging");
				});
			document.querySelectorAll(".expand-pending").forEach((el) => {
				el.classList.remove("expand-pending");
			});
			// Only remove sortable-fallback elements (these are definitely SortableJS artifacts)
			document.querySelectorAll(".sortable-fallback").forEach((el) => {
				el.remove();
			});
		});
	}

	function initSortable(el: HTMLElement): Sortable {
		const isSmartView = view.view_type === "smart";

		return Sortable.create(el, {
			group: {
				name: "sidebar",
				pull: isSmartView ? "clone" : true, // Copy from smart view, move from manual
				put: isSmartView
					? false // Cannot drop INTO smart views
					: (_to: Sortable, _from: Sortable, dragEl: HTMLElement) => {
						// Reject folders — no folder-in-folder nesting
						return dragEl?.getAttribute("data-is-folder") !== "true";
					},
			},
			animation: ANIMATION_DURATION_MS,
			fallbackOnBody: true,
			swapThreshold: 0.65,
			emptyInsertThreshold: 20, // Allow drops into empty folders
			filter: ".sidebar-empty", // Don't allow dragging the empty message
			ghostClass: "sidebar-ghost",
			chosenClass: "sidebar-chosen",
			dragClass: "sidebar-dragging",

			// Freeze reordering over collapsed sub-folders (for hover-to-expand)
			onMove(evt: MoveEvent) {
				// Folder rejection handled by group.put function (for hover-to-expand)
				const related = evt.related as HTMLElement | null;
				const folderEl =
					(related?.closest(
						"[data-folder-id]",
					) as HTMLElement | null) ||
					(related?.querySelector(
						"[data-folder-id]",
					) as HTMLElement | null);
				if (folderEl) {
					const isFolderExpanded =
						folderEl.classList.contains("expanded");
					const isSmartView =
						folderEl.classList.contains("smart-view");
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
			},
		});
	}

	async function handleDragEnd(evt: SortableEvent) {
		// Block drops into smart views
		if (view.view_type === "smart" && evt.from !== evt.to) {
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
				viewEntities = dndItems.map((item) => item.entity);
				await persistFolderReorder(dndItems, view.id);
			} else {
				// Cross-zone drop - item came from another zone
				await handleCrossZoneDrop(evt);
			}
		} catch (error) {
			console.error(
				"[UnifiedFolder] Failed to handle drop, rolling back:",
				error,
			);
			dndItems = rollbackItems;
			viewEntities = rollbackEntities;
			originalItemUrls = rollbackOriginalUrls;
			spaceStore.invalidateViewCache();
		}
	}

	async function handleCrossZoneDrop(evt: SortableEvent) {
		// Get the dropped item's data from the DOM element
		const droppedEl = evt.item;
		const itemUrl = droppedEl.getAttribute("data-url");
		const sourceViewId = droppedEl.getAttribute("data-source-view-id");
		const sourceSpaceId = droppedEl.getAttribute("data-source-space-id");
		const sourceIsSmartView =
			droppedEl.getAttribute("data-source-smart-view") === "true";

		if (!itemUrl) {
			console.warn("[UnifiedFolder] Cross-zone drop missing item URL");
			return;
		}

		// Block folders being dropped into folders
		const isFolder = droppedEl.getAttribute("data-is-folder") === "true";
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
		if (entity.id.startsWith("/")) {
			return entity.id;
		}
		// Otherwise construct from namespace and id
		return `/${entity.namespace}/${entity.id}`;
	}

	// Listen for expand-folder custom event from parent (during drag hover)
	$effect(() => {
		const el = folderEl;
		if (!el) return;

		function handleExpandFolder() {
			if (!isExpanded && view.view_type !== "smart") {
				spaceStore.expandView(view.id);
			}
		}

		el.addEventListener("expandfolder", handleExpandFolder);
		return () => el.removeEventListener("expandfolder", handleExpandFolder);
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
		<button
			class="sidebar-interactive"
			class:renaming={isRenaming}
			class:system={view.is_system}
			class:expand-pending={isExpandPending}
			class:smart-view={view.view_type === "smart"}
			class:drop-target={isDragOver}
			onclick={handleClick}
			oncontextmenu={handleContextMenu}
		>
			<span class="folder-toggle" class:expanded={isExpanded}>
				<span class="folder-toggle-icon">
					{#if isEmoji(folderIcon)}
						<span class="sidebar-emoji">{folderIcon}</span>
					{:else}
						<Icon
							icon={folderIcon}
							width="16"
							class="sidebar-icon"
						/>
					{/if}
				</span>
				<svg
					class="folder-toggle-chevron"
					width="12"
					height="12"
					viewBox="0 0 16 16"
					fill="none"
				>
					<path
						d="M6 4L10 8L6 12"
						stroke="currentColor"
						stroke-width="1.5"
						stroke-linecap="round"
						stroke-linejoin="round"
					/>
				</svg>
			</span>

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
				<!-- svelte-ignore a11y_no_static_element_interactions -->
				<span class="sidebar-item-actions">
					<button class="sidebar-item-action" title="More options" onclick={handleMoreClick}>
						<svg
							width="14"
							height="14"
							viewBox="0 0 16 16"
							fill="currentColor"
						>
							<circle cx="4" cy="8" r="1.25" />
							<circle cx="8" cy="8" r="1.25" />
							<circle cx="12" cy="8" r="1.25" />
						</svg>
					</button>
					{#if hasQuickAdd}
						<button
							class="sidebar-item-action"
							title="New {getFolderNamespace() === 'chat' ? 'Chat' : 'Page'}"
							onclick={handleQuickAdd}
						>
							<svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
								<path d="M8 3.5v9M3.5 8h9" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" fill="none" />
							</svg>
						</button>
					{/if}
				</span>
			{/if}
		</button>

		<!-- Folder contents — CSS grid 0fr/1fr handles expand/collapse animation.
			 Content is always in DOM so there's no Svelte transition measurement on init. -->
		<div class="sidebar-expandable-content" class:expanded={isExpanded}>
			<div class="sidebar-expandable-overflow">
				{#if viewLoading && dndItems.length === 0}
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
							{#each dndItems.slice(0, view.view_type === "smart" ? 8 : undefined) as item (item.id)}
								<div
									class="sidebar-dnd-item"
									role="listitem"
									data-url={item.url}
									data-source-view-id={view.id}
									data-source-smart-view={view.view_type ===
									"smart"
										? "true"
										: null}
								>
									{#if item.entity}
										<SidebarNavItem
											item={{
												id: item.entity.id,
												type: "link",
												label: item.entity.name,
												href: item.id,
												icon: getIconForEntity(
													item.entity,
												),
											}}
											{collapsed}
											indent={1}
											inFolderContext={{
												viewId: view.id,
												isSystemFolder: view.is_system,
											}}
											{accentColor}
										/>
									{/if}
								</div>
							{/each}
						{/if}
					</div>
				{/if}
			</div>
		</div>
	{/if}
</div>

<style>
	@reference "../../../app.css";
	@reference "$lib/styles/sidebar.css";
	/* Base icon styles are in sidebar.css (globally imported in app.css) */

	.sidebar-emoji {
		font-size: 14px;
		line-height: 16px;
		width: 16px;
		text-align: center;
		flex-shrink: 0;
	}

	.unified-folder {
		display: flex;
		flex-direction: column;
	}

	.unified-folder.collapsed {
		display: none;
	}

	/* ------- Icon ↔ Chevron slide toggle ------- */
	.folder-toggle {
		position: relative;
		width: 16px;
		height: 16px;
		flex-shrink: 0;
		overflow: hidden;
		cursor: pointer;
	}

	.folder-toggle-icon,
	.folder-toggle-chevron {
		position: absolute;
		inset: 0;
		display: flex;
		align-items: center;
		justify-content: center;
		transition:
			opacity 120ms ease,
			transform 160ms ease;
	}

	/* Default: icon in place, chevron below */
	.folder-toggle-icon {
		opacity: 1;
		transform: translateY(0);
	}

	.folder-toggle-chevron {
		opacity: 0;
		transform: translateY(6px);
		color: var(--color-foreground-subtle);
		margin: auto;
	}

	/* Hover: icon slides up, chevron slides up into place */
	.sidebar-interactive:hover .folder-toggle-icon {
		opacity: 0;
		transform: translateY(-6px);
	}

	.sidebar-interactive:hover .folder-toggle-chevron {
		opacity: 1;
		transform: translateY(0);
	}

	/* Expanded: always show chevron rotated 90°, hide icon */
	.folder-toggle.expanded .folder-toggle-icon {
		opacity: 0;
		transform: translateY(-6px);
	}

	.folder-toggle.expanded .folder-toggle-chevron {
		opacity: 1;
		transform: translateY(0) rotate(90deg);
	}

	/* Expanded + hover: keep rotated */
	.sidebar-interactive:hover .folder-toggle.expanded .folder-toggle-chevron {
		transform: translateY(0) rotate(90deg);
	}

	/* CSS grid expand/collapse — no JS measurement, no intro stutter */
	.sidebar-expandable-content {
		display: grid;
		grid-template-rows: 0fr;
		transition: grid-template-rows 150ms ease;
	}

	.sidebar-expandable-content.expanded {
		grid-template-rows: 1fr;
	}

	.sidebar-expandable-overflow {
		overflow: hidden;
		padding-top: 4px;
	}

	/* Empty drop zone styling */
	.sidebar-dnd-zone.empty {
		min-height: 32px;
		border: 1px dashed var(--color-border-subtle, rgba(128, 128, 128, 0.2));
		border-radius: 4px;
		margin: 4px 8px;
	}
</style>
