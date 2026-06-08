<script lang="ts">
	import { onMount } from "svelte";
	import Sortable from "sortablejs";
	import type { SortableEvent, MoveEvent } from "sortablejs";
	import { reorder } from "$lib/utils/useSortable.svelte";
	import { spaceStore } from "$lib/stores/space.svelte";
	import { type SidebarDndItem } from "$lib/stores/dndManager.svelte";
	import { contextMenu } from "$lib/stores/contextMenu.svelte";
	import {
		updateView,
		removeViewItem,
		type ViewEntity,
	} from "$lib/api/client";
	import Icon from "$lib/components/Icon.svelte";
	import type { ViewSummary } from "$lib/api/client";
	import { sidebarState } from "$lib/stores/sidebarState.svelte";
	import WorkspaceHeader from "./WorkspaceHeader.svelte";
	import { iconPickerStore } from "$lib/stores/iconPicker.svelte";
	import UnifiedFolder from "./UnifiedFolder.svelte";
	import SidebarNavItem from "./SidebarNavItem.svelte";
	import SidebarFooter from "./SidebarFooter.svelte";
	import SystemSection from "./SystemSection.svelte";
	import PinnedSection from "./PinnedSection.svelte";
	import { SYSTEM_SECTIONS } from "$lib/sidebar/sections";
	import SearchModal from "./SearchModal.svelte";
	import EntityPicker, { type EntityResult } from "$lib/components/EntityPicker.svelte";

	const ANIMATION_DURATION_MS = 150;
	const HOVER_EXPAND_DELAY_MS = 500;

	// Collapsed state from shared store (also consumed by WindowTabBar)
	const isCollapsed = $derived(sidebarState.collapsed);

	// Search modal state
	let isSearchOpen = $state(false);

	// "Add..." entity picker state (triggered from workspace context menu)
	let showAddPicker = $state(false);
	let addPickerPos = $state({ x: 0, y: 0 });

	// Track if store is ready
	let storeReady = $state(false);

	// Extended DnD item for workspace content (root items + folders)
	interface WorkspaceDndItem extends SidebarDndItem {
		itemType: "root-item" | "folder";
		entity?: ViewEntity;
		view?: ViewSummary;
		sortOrder: number; // Unified sort order for mixed ordering
		sourceSpaceId?: string; // Track source for cross-zone drops
		sourceViewId?: string; // Track source folder for folder-to-root drops
		sourceIsSmartView?: boolean; // True if dragged from a smart view (copy semantics)
	}

	// Track DnD items per workspace (combined root items + folders)
	let workspaceContentByWorkspace = $state<Map<string, WorkspaceDndItem[]>>(
		new Map(),
	);

	// Flag to prevent $effect from running during DnD operations
	let isDndInProgress = $state(false);

	// Sync DnD items when workspace data changes
	// IMPORTANT: Merges items and folders together, sorted by sort_order
	// This allows folders to appear anywhere in the list (not just at the end)
	$effect(() => {
		// Skip re-sync during DnD operations to prevent race conditions
		if (isDndInProgress) return;

		const newContentMap = new Map<string, WorkspaceDndItem[]>();

		for (const ws of spaceStore.spaces) {
			const contentItems: WorkspaceDndItem[] = [];

			// Root items — use actual sort_order from backend
			const wsItems = spaceStore.getSpaceItems(ws.id);
			for (const item of wsItems) {
				contentItems.push({
					id: `item:${getHrefForEntity(item)}`,
					url: getHrefForEntity(item),
					label: item.name,
					icon: item.icon,
					itemType: "root-item",
					entity: item,
					sortOrder: item.sort_order,
					sourceSpaceId: ws.id,
				});
			}

			// Folders (with their sort_order from views table)
			const wsViews = spaceStore.getViewsForSpace(ws.id);
			for (const view of wsViews) {
				contentItems.push({
					id: `folder:${view.id}`,
					url: `/view/${view.id}`,
					label: view.name,
					icon: view.icon ?? undefined,
					itemType: "folder",
					view,
					sortOrder: view.sort_order ?? 0,
					sourceSpaceId: ws.id,
				});
			}

			// De-duplicate by URL before sorting (prevents "each_key_duplicate" errors)
			const seenUrls = new Set<string>();
			const dedupedItems = contentItems.filter((item) => {
				if (seenUrls.has(item.url)) return false;
				seenUrls.add(item.url);
				return true;
			});

			// Sort by sortOrder so items and folders can be interleaved
			dedupedItems.sort((a, b) => a.sortOrder - b.sortOrder);

			newContentMap.set(ws.id, dedupedItems);
		}

		workspaceContentByWorkspace = newContentMap;
	});

	// Initialize workspace store and keyboard shortcuts
	onMount(() => {
		spaceStore
			.init()
			.then(() => {
				storeReady = true;
			})
			.catch((err) => {
				console.error("[UnifiedSidebar] Failed to initialize:", err);
				storeReady = true;
			});

		window.addEventListener("keydown", handleKeydown);

		return () => {
			window.removeEventListener("keydown", handleKeydown);
		};
	});

	function handleKeydown(e: KeyboardEvent) {
		// Cmd+Shift+N - New page
		if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.key === "n") {
			e.preventDefault();
			handleNewPage();
			return;
		}
		// Cmd+N or Ctrl+N - New chat
		if ((e.metaKey || e.ctrlKey) && e.key === "n") {
			e.preventDefault();
			handleNewChat();
		}
		// Cmd+S or Ctrl+S - Toggle sidebar collapse
		if ((e.metaKey || e.ctrlKey) && e.key === "s") {
			e.preventDefault();
			toggleCollapse();
		}
		// Cmd+K or Ctrl+K - Toggle search/command center
		if ((e.metaKey || e.ctrlKey) && e.key === "k") {
			e.preventDefault();
			toggleSearch();
		}
		// Cmd+W or Ctrl+W - Open wiki overview
		if ((e.metaKey || e.ctrlKey) && e.key === "w") {
			e.preventDefault();
			handleWikiOverview();
		}
	}

	function handleSearch() {
		isSearchOpen = true;
	}

	function toggleSearch() {
		isSearchOpen = !isSearchOpen;
	}

	function closeSearch() {
		isSearchOpen = false;
	}

	function handleWikiOverview() {
		spaceStore.openTabFromRoute("/wiki", {
			label: "Wiki",
			preferEmptyPane: true,
		});
	}

	function handleNewChat() {
		// Always open a new chat tab (forceNew ensures we don't reuse existing)
		spaceStore.openTabFromRoute("/", {
			label: "New Chat",
			forceNew: true,
		});
	}

	async function handleNewPage() {
		// Create a new page and open it in a new tab
		const { pagesStore } = await import("$lib/stores/pages.svelte");
		const page = await pagesStore.createNewPage();
		spaceStore.openTabFromRoute(`/page/${page.id}`, {
			label: page.title,
			forceNew: true,
		});
	}

	async function handleAddPickerSelect(entity: EntityResult) {
		await spaceStore.addSpaceItem(entity.url);
		showAddPicker = false;
	}

	function closeAddPicker() {
		showAddPicker = false;
	}

	function handleGoHome() {
		handleNewChat();
	}

	function toggleCollapse() {
		sidebarState.toggle();
	}

	// Track newly created view for auto-focus rename
	let pendingRenameViewId = $state<string | null>(null);

	async function handleCreateFolder() {
		const view = await spaceStore.createManualView("New Folder");
		if (view) {
			pendingRenameViewId = view.id;
			spaceStore.openTabFromRoute(`/view/${view.id}`, {
				label: view.name,
				forceNew: true,
			});
		}
	}

	async function handleCreateSmartFolder() {
		const view = await spaceStore.createSmartView("New Smart Folder");
		if (view) {
			pendingRenameViewId = view.id;
			spaceStore.openTabFromRoute(`/view/${view.id}`, {
				label: view.name,
				forceNew: true,
			});
		}
	}

	// Helper to get href for entity
	function getHrefForEntity(entity: ViewEntity): string {
		// External URLs — use as-is
		if (entity.id.startsWith("http://") || entity.id.startsWith("https://")) {
			return entity.id;
		}
		// If already a full path, use as-is
		if (entity.id.startsWith("/")) {
			return entity.id;
		}
		// Otherwise construct from namespace and id
		return `/${entity.namespace}/${entity.id}`;
	}

	// ============================================================================
	// SortableJS Integration
	// ============================================================================

	// Hover-to-expand state
	let expandTimer: ReturnType<typeof setTimeout> | null = null;
	let pendingExpandFolderId: string | null = null;
	let isPointerTrackingActive = false;

	// Pointer tracking for hover-to-expand during drag
	// This is separate from SortableJS's onMove because SortableJS reorders items
	// rapidly, making it impossible to hover on a folder long enough to trigger expand
	function handlePointerMove(e: PointerEvent) {
		// Get element under the cursor (skip the dragged item using pointer position)
		const elementsUnderCursor = document.elementsFromPoint(
			e.clientX,
			e.clientY,
		);

		// Find the currently dragged element to exclude it and its children
		const draggedItem = document.querySelector(".sidebar-dragging");

		// Find a folder element under the cursor (skip the dragged element and its children)
		let folderEl: HTMLElement | null = null;
		for (const el of elementsUnderCursor) {
			// Skip the dragging element and anything inside it
			if (el.classList.contains("sidebar-dragging")) continue;
			if (draggedItem?.contains(el)) continue;

			const htmlEl = el as HTMLElement;
			// Look for a folder - check both ancestor (closest) and descendant (querySelector)
			const folder =
				(htmlEl.closest("[data-folder-id]") as HTMLElement | null) ||
				(htmlEl.querySelector?.(
					"[data-folder-id]",
				) as HTMLElement | null);

			// Skip if the folder is the dragged item or inside it
			if (
				folder &&
				!folder.classList.contains("sidebar-dragging") &&
				!draggedItem?.contains(folder)
			) {
				folderEl = folder;
				break;
			}
		}

		const folderId = folderEl?.getAttribute("data-folder-id");

		// Clear timer if we moved to a different folder (or no folder)
		if (folderId !== pendingExpandFolderId) {
			clearExpandTimer();
		}

		// Start expand timer for collapsed folders
		if (folderId && !expandTimer && folderEl) {
			const isExpanded = folderEl.classList.contains("expanded");
			const isSmartView = folderEl.classList.contains("smart-view");

			if (!isExpanded && !isSmartView) {
				pendingExpandFolderId = folderId;
				// Add visual feedback immediately
				folderEl.classList.add("expand-pending");

				expandTimer = setTimeout(() => {
					// Dispatch custom event to expand folder
					folderEl?.dispatchEvent(
						new CustomEvent("expandfolder", { bubbles: true }),
					);
					folderEl?.classList.remove("expand-pending");
					expandTimer = null;
					pendingExpandFolderId = null;
				}, HOVER_EXPAND_DELAY_MS);
			}
		}
	}

	function startPointerTracking() {
		if (isPointerTrackingActive) return;
		isPointerTrackingActive = true;
		document.addEventListener("pointermove", handlePointerMove);
	}

	function stopPointerTracking() {
		if (!isPointerTrackingActive) return;
		isPointerTrackingActive = false;
		document.removeEventListener("pointermove", handlePointerMove);
	}

	// Initialize SortableJS for each workspace when mounted
	$effect(() => {
		// Clean up timers and listeners on destroy
		return () => {
			if (expandTimer) {
				clearTimeout(expandTimer);
				expandTimer = null;
			}
			stopPointerTracking();
		};
	});

	// Create Sortable instance for a workspace
	function initSortable(el: HTMLElement, workspaceId: string) {
		return Sortable.create(el, {
			group: {
				name: "sidebar",
				pull: true,
				put: true,
			},
			animation: ANIMATION_DURATION_MS,
			fallbackOnBody: true,
			swapThreshold: 0.65,
			emptyInsertThreshold: 20, // Allow drops into empty workspaces
			ghostClass: "sidebar-ghost",
			chosenClass: "sidebar-chosen",
			dragClass: "sidebar-dragging",

			// onStart fires when drag actually begins (after delay)
			onStart(evt: SortableEvent) {
				// Hide folder contents during drag to make it easier to position
				const expandableContent = evt.item.querySelector(
					".sidebar-expandable-content",
				);
				if (expandableContent instanceof HTMLElement) {
					expandableContent.style.display = "none";
				}
				// Start tracking pointer for hover-to-expand
				startPointerTracking();
			},

			// Handle items ADDED from another list (cross-zone drops TO this workspace)
			async onAdd(evt: SortableEvent) {
				try {
					// CAPTURE the FULL intended order from DOM BEFORE removing the element
					// Must include BOTH items and folders for proper interleaving
					const container = evt.to;
					const domItems = Array.from(
						container.querySelectorAll(
							":scope > .sidebar-dnd-item",
						),
					);

					const intendedFullOrder: Array<{
						type: "item" | "folder";
						url: string;
					}> = [];
					for (const el of domItems) {
						const url = el.getAttribute("data-url");
						const isFolder =
							el.getAttribute("data-is-folder") === "true";
						if (url) {
							intendedFullOrder.push({
								type: isFolder ? "folder" : "item",
								url,
							});
						}
					}

					// Remove the DOM element SortableJS added - we'll reload from API
					evt.item.remove();
					await handleCrossZoneMove(
						evt,
						workspaceId,
						intendedFullOrder,
					);
				} catch (error) {
					console.error("[UnifiedSidebar] Error in onAdd:", error);
					// On error, invalidate cache to reset state
					spaceStore.invalidateViewCache();
				} finally {
					// Always cleanup stuck visual state
					cleanupStuckDndState();
				}
			},

			// Handle drag end - restore visibility and process same-zone reorders
			async onEnd(evt: SortableEvent) {
				try {
					// Restore folder content visibility
					const expandableContent = evt.item.querySelector(
						".sidebar-expandable-content",
					);
					if (expandableContent instanceof HTMLElement) {
						expandableContent.style.display = "";
					}

					// Stop pointer tracking
					stopPointerTracking();
					clearExpandTimer();

					// Only handle same-zone reorders here (cross-zone handled by onAdd)
					if (evt.from === evt.to) {
						await handleDragEnd(evt, workspaceId);
					}
				} catch (error) {
					console.error("[UnifiedSidebar] Error in onEnd:", error);
				} finally {
					// Always cleanup stuck visual state
					cleanupStuckDndState();
				}
			},
		});
	}

	function clearExpandTimer() {
		if (expandTimer) {
			clearTimeout(expandTimer);
			expandTimer = null;
		}
		if (pendingExpandFolderId) {
			const folderEl = document.querySelector(
				`[data-folder-id="${pendingExpandFolderId}"]`,
			);
			folderEl?.classList.remove("expand-pending");
			pendingExpandFolderId = null;
		}
	}

	// Clean up any stuck DnD visual state (ghost elements, classes, etc.)
	// Uses a small delay to let SortableJS finish its own cleanup first
	function cleanupStuckDndState() {
		// Clear expand timer and pointer tracking immediately
		clearExpandTimer();
		stopPointerTracking();

		// Delay DOM cleanup to let SortableJS finish first
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

	// Handle drag end - persist same-zone reorder
	// Note: Cross-zone moves are handled by onAdd → handleCrossZoneMove
	async function handleDragEnd(evt: SortableEvent, workspaceId: string) {
		// Clear any pending expand timer
		clearExpandTimer();

		const items = workspaceContentByWorkspace.get(workspaceId) || [];

		// Prevent $effect from re-syncing during operation
		isDndInProgress = true;

		// Capture rollback state BEFORE making any changes
		const rollbackMap = new Map(workspaceContentByWorkspace);

		try {
			// Reorder within same zone
			const reorderedItems = reorder(items, evt);
			const newMap = new Map(workspaceContentByWorkspace);
			newMap.set(workspaceId, reorderedItems);
			workspaceContentByWorkspace = newMap;

			// Persist the reorder
			await persistReorder(reorderedItems, workspaceId);
		} catch (err) {
			console.error(
				"[UnifiedSidebar] Failed to persist drag operation, rolling back:",
				err,
			);
			workspaceContentByWorkspace = rollbackMap;
			spaceStore.invalidateViewCache();
		} finally {
			isDndInProgress = false;
		}
	}

	// Persist reorder to backend — single counter for items and folders
	async function persistReorder(
		items: WorkspaceDndItem[],
		workspaceId: string,
	) {
		const itemSortOrders: Array<{ url: string; sort_order: number }> = [];
		const folderUpdates: Array<{ viewId: string; sortOrder: number }> = [];

		for (let i = 0; i < items.length; i++) {
			const item = items[i];
			if (item.itemType === "root-item" && item.url) {
				itemSortOrders.push({ url: item.url, sort_order: i });
			} else if (item.itemType === "folder" && item.view) {
				const viewId = item.id.replace("folder:", "");
				folderUpdates.push({ viewId, sortOrder: i });
			}
		}

		// Update folder sort_order values
		for (const update of folderUpdates) {
			await updateView(update.viewId, { sort_order: update.sortOrder });
		}

		if (folderUpdates.length > 0) {
			await spaceStore.refreshViews();
		}

		// Update item sort_order values
		if (itemSortOrders.length > 0) {
			await spaceStore.reorderSpaceItems(itemSortOrders, workspaceId);
		}
	}

	// Handle cross-zone move (item dropped from folder or another workspace)
	async function handleCrossZoneMove(
		evt: SortableEvent,
		workspaceId: string,
		intendedFullOrder: Array<{ type: "item" | "folder"; url: string }>,
	) {
		// Get the dropped item's data from the DOM element
		const droppedEl = evt.item;
		const itemUrl = droppedEl.getAttribute("data-url");
		const sourceViewId = droppedEl.getAttribute("data-source-view-id");
		const sourceIsSmartView =
			droppedEl.getAttribute("data-source-smart-view") === "true";

		if (!itemUrl) {
			console.warn("[UnifiedSidebar] Cross-zone drop missing item URL");
			return;
		}

		// PHASE 1: Add item to workspace root
		await spaceStore.addSpaceItem(itemUrl, workspaceId);

		// PHASE 2: Remove from source (only for move operations, not smart view copies)
		if (sourceViewId && !sourceIsSmartView) {
			await removeViewItem(sourceViewId, itemUrl);
		}

		// PHASE 3: Persist full order — single counter for items and folders
		const itemSortOrders: Array<{ url: string; sort_order: number }> = [];
		const folderUpdates: Array<{ viewId: string; sortOrder: number }> = [];

		for (let i = 0; i < intendedFullOrder.length; i++) {
			const entry = intendedFullOrder[i];
			if (entry.type === "item") {
				itemSortOrders.push({ url: entry.url, sort_order: i });
			} else {
				const viewId = entry.url.replace("/view/", "");
				folderUpdates.push({ viewId, sortOrder: i });
			}
		}

		// Update folder sort_order values
		for (const update of folderUpdates) {
			await updateView(update.viewId, { sort_order: update.sortOrder });
		}

		// Update item sort_order values
		if (itemSortOrders.length > 0) {
			await spaceStore.reorderSpaceItems(itemSortOrders, workspaceId);
		}

		// PHASE 4: Invalidate cache and refresh
		spaceStore.invalidateViewCache();
		await spaceStore.refreshViews();
	}

	// Svelte action to initialize SortableJS on an element
	function sortableAction(
		node: HTMLElement,
		params: { workspaceId: string; immutable?: boolean },
	) {
		if (params.immutable) return { destroy() {} };
		const sortable = initSortable(node, params.workspaceId);

		return {
			destroy() {
				sortable.destroy();
			},
		};
	}

	function handleSidebarContextMenu(
		e: MouseEvent,
		workspace: (typeof spaceStore.spaces)[0],
	) {
		// Don't show create options for system workspace
		if (workspace.is_system) return;

		e.preventDefault();
		e.stopPropagation();

		const menuPos = { x: e.clientX, y: e.clientY };

		contextMenu.show(menuPos, [
			{
				id: "new-chat",
				label: "New Chat",
				icon: "ri:chat-new-line",
				shortcut: "⌘N",
				action: handleNewChat,
			},
			{
				id: "new-page",
				label: "New Page",
				icon: "ri:file-add-line",
				shortcut: "⌘⇧N",
				action: handleNewPage,
			},
			{
				id: "new-folder",
				label: "New Folder",
				icon: "ri:folder-add-line",
				dividerBefore: true,
				action: handleCreateFolder,
			},
			{
				id: "new-smart-folder",
				label: "New Smart Folder",
				icon: "ri:filter-line",
				action: handleCreateSmartFolder,
			},
			{
				id: "add-item",
				label: "Add...",
				icon: "ri:add-circle-line",
				dividerBefore: true,
				action: () => {
					addPickerPos = menuPos;
					showAddPicker = true;
				},
			},
		]);
	}

	// Stagger delay per item
	const STAGGER_DELAY = 30;

	// Tailwind utility class strings
	const sidebarClass = $derived.by(() =>
		[
			"sidebar-container relative h-full bg-transparent",
			"transition-[width] duration-300 ease-[cubic-bezier(0.34,1.56,0.64,1)]",
			isCollapsed ? "sidebar-collapsed" : "w-52 overflow-hidden",
		].join(" "),
	);

	const sidebarInnerClass = $derived.by(() =>
		[
			"flex h-full min-w-52 w-52 flex-col",
			isCollapsed ? "pointer-events-none" : "",
		].join(" "),
	);
</script>

<aside class={sidebarClass}>
	<!-- Book Spine: When collapsed, show expand button on hover -->
	{#if isCollapsed}
		<button
			class="sidebar-expand-button group absolute top-0 left-0 w-[36px] z-30 flex h-full cursor-pointer items-center justify-center border-none bg-transparent"
			onclick={toggleCollapse}
			aria-label="Expand sidebar"
		>
			<svg
				class="sidebar-expand-icon h-3.5 w-3.5 -translate-x-[3px] opacity-0 transition-all duration-200 ease-premium group-active:scale-95"
				style="color: var(--color-foreground-subtle)"
				viewBox="0 0 24 24"
				fill="none"
				stroke="currentColor"
				stroke-width="2"
				stroke-linecap="round"
				stroke-linejoin="round"
			>
				<!-- Double chevron right >> -->
				<polyline points="6 17 11 12 6 7" />
				<polyline points="13 17 18 12 13 7" />
			</svg>
		</button>
	{/if}

	<div class={sidebarInnerClass}>
		<WorkspaceHeader
			collapsed={isCollapsed}
			animationDelay={STAGGER_DELAY}
		/>

		<!-- Command bar -->
		<button
			class="command-bar"
			class:collapsed={isCollapsed}
			onclick={handleSearch}
			title="Command (⌘K)"
			style="animation-delay: {STAGGER_DELAY * 2}ms; --stagger-delay: {STAGGER_DELAY * 2}ms"
		>
			<span class="command-label">Command</span>
			<kbd class="command-kbd">⌘K</kbd>
		</button>

		<!-- Single workspace content (carousel removed) -->
		<nav
			class="workspace-nav"
			class:collapsed={isCollapsed}
			oncontextmenu={(e) => {
				const ws = spaceStore.activeSpace;
				if (ws) handleSidebarContextMenu(e, ws);
			}}
		>
			{#if !storeReady}
				<div class="loading-state">
					<Icon icon="ri:loader-4-line" width="16" class="spinner" />
					<span>Loading...</span>
				</div>
			{:else}
				{@const contentItems = workspaceContentByWorkspace.get(spaceStore.activeSpaceId) || []}
				{@const wsAccentColor = spaceStore.activeSpace?.accent_color || null}

				<!-- Pinned (user-curated; renders nothing when empty) -->
				<PinnedSection collapsed={isCollapsed} />

				<!-- System sections (from constants) -->
				{#each SYSTEM_SECTIONS as section (section.id)}
					<SystemSection
						{section}
						collapsed={isCollapsed}
						accentColor={wsAccentColor}
					/>
				{/each}

				<!-- User folders + root items (draggable) -->
				<div
					class="workspace-content"
					use:sortableAction={{ workspaceId: spaceStore.activeSpaceId }}
				>
					{#each contentItems as item (item.id)}
						<div
							class="sidebar-dnd-item"
							data-url={item.url}
							data-is-folder={item.itemType === "folder" ? "true" : null}
							data-source-space-id={item.sourceSpaceId || null}
							data-source-view-id={item.sourceViewId || null}
							data-source-smart-view={item.sourceIsSmartView ? "true" : null}
						>
							{#if item.itemType === "folder" && item.view}
								<UnifiedFolder
									view={item.view}
									collapsed={isCollapsed}
									accentColor={wsAccentColor}
									autoFocusRename={pendingRenameViewId === item.view.id}
									onRenameFocusConsumed={() => (pendingRenameViewId = null)}
								/>
							{:else if item.entity}
								<SidebarNavItem
									item={{
										id: item.entity.id,
										type: "link",
										label: item.entity.name,
										icon: item.entity.icon || "ri:file-text-line",
										href: item.url,
									}}
									collapsed={isCollapsed}
									accentColor={wsAccentColor}
								/>
							{/if}
						</div>
					{/each}
				</div>
			{/if}
		</nav>

		<SidebarFooter
			collapsed={isCollapsed}
			animationDelay={10 * STAGGER_DELAY}
		/>
	</div>
</aside>

<SearchModal open={isSearchOpen} onClose={closeSearch} />

{#if showAddPicker}
	<EntityPicker
		mode="single"
		position={addPickerPos}
		placeholder="Search or paste a URL..."
		onSelect={handleAddPickerSelect}
		onClose={closeAddPicker}
	/>
{/if}

<style>
	@reference "../../../app.css";
	@reference "$lib/styles/sidebar.css";

	/* Collapsed sidebar behavior */
	.sidebar-collapsed {
		width: 0;
		overflow: visible; /* Allow hover zone to extend beyond 0-width */
		/* Transition handled by Tailwind classes on parent */
	}

	/* Hover zone extends through the mini state + page padding area */
	.sidebar-collapsed::before {
		content: "";
		position: absolute;
		top: 0;
		left: 0;
		width: 36px; /* 20px mini state + padding area */
		height: 100%;
		z-index: 20;
		pointer-events: auto;
		cursor: pointer;
	}

	/* On hover, expand to show the open icon */
	.sidebar-collapsed:hover {
		width: 20px;
	}

	/* Show icon when sidebar is hovered */
	.sidebar-collapsed:hover .sidebar-expand-icon {
		opacity: 1;
	}

	@keyframes fadeSlideIn {
		from {
			opacity: 0;
			transform: translateX(-8px);
		}
		to {
			opacity: 1;
			transform: translateX(0);
		}
	}

	@keyframes spin {
		from {
			transform: rotate(0deg);
		}
		to {
			transform: rotate(360deg);
		}
	}

	/* Single workspace nav (carousel removed) */
	.workspace-nav {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		overflow-x: hidden;
		padding: 12px 0 12px 8px;
	}

	.workspace-nav.collapsed {
		display: flex;
		flex-direction: column;
		align-items: center;
	}

	.loading-state {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 12px;
		color: var(--color-foreground-subtle);
		font-size: 13px;
	}

	.spinner {
		animation: spin 1s linear infinite;
	}

	.empty-state {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 8px;
		padding: 24px 12px;
		color: var(--color-foreground-subtle);
		font-size: 13px;
		text-align: center;
	}

	.empty-state .empty-hint {
		font-size: 11px;
		opacity: 0.7;
	}

	/* Folder list for reordering */
	.folder-list {
		display: flex;
		flex-direction: column;
	}

	/* Folder wrappers use same spacing as items */
	.folder-wrapper {
		margin-bottom: var(--sidebar-item-gap, 4px);
	}

	:global(.folder-wrapper[aria-grabbed="true"]) {
		opacity: 0.5;
	}

	/* Root items section */
	.root-items {
		display: flex;
		flex-direction: column;
		margin-bottom: 8px;
	}

	/* Workspace content drop zone - fill available space for drops */
	.workspace-content {
		display: flex;
		flex-direction: column;
		min-height: 200px; /* Minimum for short lists */
		flex: 1; /* Grow to fill remaining space */
		padding-bottom: 100px; /* Extra padding at bottom for easier drops */
	}

	/* SortableJS item wrapper - inherits from sidebar.css */

	/* SortableJS styles are in sidebar.css */

	/* Command bar — visual separator between spaces and content */
	.command-bar {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 4px;
		margin: 0 0 4px 8px;
		padding: 5px 8px;
		background: color-mix(in srgb, var(--color-foreground) 5%, transparent);
		border: 1px solid transparent;
		border-radius: 6px;
		font-family: var(--font-sans);
		font-size: 12px;
		color: var(--color-foreground-subtle);
		cursor: pointer;
		transition: all 0.15s ease;
		animation: fadeSlideIn 200ms cubic-bezier(0.2, 0, 0, 1) backwards;
	}

	.command-bar:hover {
		background: color-mix(in srgb, var(--color-foreground) 8%, transparent);
		color: var(--color-foreground-muted);
	}

	.command-bar.collapsed {
		opacity: 0;
		transform: translateX(-8px);
		pointer-events: none;
		transition:
			opacity 150ms cubic-bezier(0.2, 0, 0, 1),
			transform 150ms cubic-bezier(0.2, 0, 0, 1);
	}

	.command-label {
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.command-kbd {
		font-family: inherit;
		font-size: 10px;
		color: var(--color-foreground-subtle);
		opacity: 0.7;
	}
</style>
