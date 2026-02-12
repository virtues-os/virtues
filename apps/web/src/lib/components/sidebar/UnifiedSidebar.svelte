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
	import { buildSpaceContextMenu } from "$lib/utils/contextMenuItems";
	import { sidebarState } from "$lib/stores/sidebarState.svelte";
	import Modal from "$lib/components/Modal.svelte";
	import WorkspaceHeader from "./WorkspaceHeader.svelte";
	import { iconPickerStore } from "$lib/stores/iconPicker.svelte";
	import UnifiedFolder from "./UnifiedFolder.svelte";
	import SidebarNavItem from "./SidebarNavItem.svelte";
	import SidebarFooter from "./SidebarFooter.svelte";
	import SearchModal from "./SearchModal.svelte";
	import WorkspaceInfoModal from "./WorkspaceInfoModal.svelte";
	import ColorPickerModal from "./ColorPickerModal.svelte";
	import EntityPicker, { type EntityResult } from "$lib/components/EntityPicker.svelte";

	const ANIMATION_DURATION_MS = 150;
	const HOVER_EXPAND_DELAY_MS = 500;

	const SLIDE_WIDTH = 208; // w-52 = 13rem = 208px

	// Collapsed state from shared store (also consumed by WindowTabBar)
	const isCollapsed = $derived(sidebarState.collapsed);

	// Search modal state
	let isSearchOpen = $state(false);

	// Workspace info modal state
	let isWorkspaceInfoOpen = $state(false);

	// New Space modal state
	let showNewSpaceModal = $state(false);
	let newSpaceName = $state("");
	let isCreatingSpace = $state(false);

	// Inline rename state
	let isRenaming = $state(false);
	let renameValue = $state("");

	// Icon picker uses shared iconPickerStore (modal rendered in +layout.svelte)

	// Color picker modal state
	let showColorPicker = $state(false);

	// "Add..." entity picker state (triggered from workspace context menu)
	let showAddPicker = $state(false);
	let addPickerPos = $state({ x: 0, y: 0 });

	// Track if store is ready
	let storeReady = $state(false);

	// Scroll progress for title animation (-1 to 1 relative to current snap)
	let scrollProgress = $state(0);

	// Viewport element for scroll snap
	let viewportEl: HTMLDivElement | null = $state(null);
	let currentIndex = $state(0);
	let scrollEndTimeout: ReturnType<typeof setTimeout> | null = null;
	let isScrolling = $state(false);
	let isProgrammaticScroll = false;
	let scrollStartIndex = $state(0);

	// Track active touch/drag to defer snap until release (Arc-style behavior)
	let isPointerDown = $state(false);

	// rAF-based smoothing for scroll progress updates
	// This ensures updates happen exactly once per frame, eliminating jitter
	let rafId: number | null = null;
	let pendingProgress = 0;

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

	// Workspace info type for header transitions
	type WorkspaceInfo = {
		name: string;
		icon: string | null;
		accentColor: string | null;
		isSystem: boolean;
	};

	// Get workspace info by index
	function getWorkspaceInfo(index: number): WorkspaceInfo | null {
		const ws = spaceStore.spaces[index];
		if (!ws) return null;
		return {
			name: ws.is_system ? "Virtues" : ws.name,
			icon: ws.icon || null,
			accentColor: ws.accent_color || null,
			isSystem: ws.is_system,
		};
	}

	// Current transition workspaces [prev, current, next]
	// Derived from currentIndex (local) rather than activeSpaceId (store) so that
	// scrollProgress and the workspace info update in the same render frame.
	const transitionWorkspaces = $derived.by(
		(): [WorkspaceInfo | null, WorkspaceInfo, WorkspaceInfo | null] => {
			return [
				getWorkspaceInfo(currentIndex - 1),
				getWorkspaceInfo(currentIndex) || {
					name: "Workspace",
					icon: null,
					accentColor: null,
					isSystem: false,
				},
				getWorkspaceInfo(currentIndex + 1),
			];
		},
	);

	// Handle pointer/touch down - disable snap for free dragging (Arc-style)
	function handlePointerDown() {
		isPointerDown = true;
		if (viewportEl) {
			// Disable snap and smooth behavior for instant 1:1 tracking
			viewportEl.style.scrollSnapType = "none";
			viewportEl.style.scrollBehavior = "auto";
		}
	}

	// Handle pointer/touch up - re-enable snap and let browser settle
	function handlePointerUp() {
		if (!isPointerDown) return;
		isPointerDown = false;

		// Clear any pending scroll end timeout
		if (scrollEndTimeout) {
			clearTimeout(scrollEndTimeout);
			scrollEndTimeout = null;
		}

		if (viewportEl) {
			// Re-enable smooth behavior first, then snap
			// This ensures the snap animation is smooth
			viewportEl.style.scrollBehavior = "smooth";
			viewportEl.style.scrollSnapType = "x mandatory";

			// Wait for snap animation to complete before finalizing
			// The scroll events from snapping will reset this timeout,
			// but if there's no snapping needed, this ensures we still finalize
			scrollEndTimeout = setTimeout(handleScrollEnd, 300);
		}
	}

	// Touch event handlers as fallback for devices where pointer events might not fire correctly
	function handleTouchStart() {
		handlePointerDown();
	}

	function handleTouchEnd() {
		handlePointerUp();
	}

	// Handle native scroll events
	function handleScroll() {
		if (!viewportEl || isProgrammaticScroll) return;

		const scrollLeft = viewportEl.scrollLeft;

		// Mark as scrolling and capture start index
		if (!isScrolling) {
			isScrolling = true;
			scrollStartIndex = currentIndex;
		}

		// Clamp scroll to max 1 slide away from start index (prevents 1→3 jumps)
		const minScroll = Math.max(0, (scrollStartIndex - 1) * SLIDE_WIDTH);
		const maxScroll = Math.min(
			(spaceStore.spaces.length - 1) * SLIDE_WIDTH,
			(scrollStartIndex + 1) * SLIDE_WIDTH,
		);

		if (scrollLeft < minScroll || scrollLeft > maxScroll) {
			const clampedScroll = Math.max(
				minScroll,
				Math.min(maxScroll, scrollLeft),
			);
			viewportEl.scrollLeft = clampedScroll;
			return;
		}

		// Calculate progress relative to current index (-1 to 1)
		const offset = scrollLeft - currentIndex * SLIDE_WIDTH;
		pendingProgress = Math.max(-1, Math.min(1, offset / SLIDE_WIDTH));

		// Schedule ONE update per frame using rAF
		// This batches multiple scroll events into a single visual update,
		// eliminating jitter from irregular scroll event timing
		if (!rafId) {
			rafId = requestAnimationFrame(() => {
				scrollProgress = pendingProgress;
				spaceStore.swipeProgress = pendingProgress;
				rafId = null;
			});
		}

		// Debounce scroll end detection
		if (scrollEndTimeout) clearTimeout(scrollEndTimeout);
		scrollEndTimeout = setTimeout(handleScrollEnd, 100);
	}

	async function handleScrollEnd() {
		if (!viewportEl) return;

		// Don't finalize while user is still dragging
		if (isPointerDown) return;

		// Cancel any pending rAF to prevent stale updates
		if (rafId) {
			cancelAnimationFrame(rafId);
			rafId = null;
		}

		// Determine which slide we landed on (clamping ensures it's only ±1 from start)
		const scrollLeft = viewportEl.scrollLeft;
		const newIndex = Math.round(scrollLeft / SLIDE_WIDTH);

		try {
			if (newIndex !== currentIndex) {
				currentIndex = newIndex;

				// Reset progress BEFORE switching workspace so the header
				// doesn't flash stale accent colors for one frame.
				scrollProgress = 0;
				pendingProgress = 0;
				spaceStore.swipeProgress = 0;

				const workspace = spaceStore.spaces[newIndex];
				if (workspace && workspace.id !== spaceStore.activeSpaceId) {
					await spaceStore.switchSpace(workspace.id);
				}
			}
		} catch (e) {
			console.error("[UnifiedSidebar] Error during workspace switch:", e);
		} finally {
			// Always reset state, even on error - prevents frozen UI
			isScrolling = false;
			scrollProgress = 0;
			pendingProgress = 0;
			spaceStore.swipeProgress = 0;
			isProgrammaticScroll = false;
		}
	}

	// Programmatic workspace navigation with title animation
	// Used by both the external sync $effect and scrollToWorkspace()
	let programmaticAnimCancelled = false;

	function navigateToIndex(targetIndex: number) {
		if (!viewportEl || targetIndex === currentIndex) return;

		// Cancel any in-flight rAF title animation
		programmaticAnimCancelled = true;

		// Record where we're scrolling FROM for proper clamping,
		// and mark as scrolling BEFORE updating currentIndex so that
		// handleScroll doesn't overwrite scrollStartIndex
		scrollStartIndex = currentIndex;
		isScrolling = true;
		currentIndex = targetIndex;

		// Smooth scroll — the native scroll events will drive scrollProgress
		// naturally as the browser animates, giving us a real swipe feel
		viewportEl.scrollTo({
			left: targetIndex * SLIDE_WIDTH,
			behavior: "smooth",
		});
	}

	// Navigate to workspace (for external navigation like chevron clicks)
	export function scrollToWorkspace(workspaceId: string) {
		const index = spaceStore.spaces.findIndex((w) => w.id === workspaceId);
		if (index >= 0 && viewportEl) {
			navigateToIndex(index);
		}
	}

	// Sync scroll position when workspace changes externally (e.g. ⌘1-9 shortcuts)
	$effect(() => {
		if (!viewportEl || !storeReady || isScrolling) return;
		const targetIndex = spaceStore.spaces.findIndex(
			(w) => w.id === spaceStore.activeSpaceId,
		);
		if (targetIndex >= 0 && targetIndex !== currentIndex) {
			navigateToIndex(targetIndex);
		}
	});

	// Initialize workspace store and keyboard shortcuts
	onMount(() => {
		// Initialize workspace store
		spaceStore
			.init()
			.then(() => {
				// Set initial scroll position BEFORE storeReady
				// This prevents the $effect from triggering a smooth scroll animation
				if (viewportEl) {
					const idx = spaceStore.spaces.findIndex(
						(w) => w.id === spaceStore.activeSpaceId,
					);
					if (idx >= 0) {
						currentIndex = idx;
						// Temporarily disable smooth scroll for instant initial position
						viewportEl.style.scrollBehavior = "auto";
						viewportEl.scrollLeft = idx * SLIDE_WIDTH;
						// Re-enable smooth scroll for user interactions
						viewportEl.style.scrollBehavior = "";
					}
				}
				storeReady = true;
			})
			.catch((err) => {
				console.error(
					"[UnifiedSidebar] Failed to initialize workspace store:",
					err,
				);
				storeReady = true;
			});

		// Keyboard shortcuts
		window.addEventListener("keydown", handleKeydown);

		return () => {
			window.removeEventListener("keydown", handleKeydown);
			if (scrollEndTimeout) clearTimeout(scrollEndTimeout);
			if (rafId) cancelAnimationFrame(rafId);
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

	function handleWorkspaceClick(e: MouseEvent) {
		const space = spaceStore.activeSpace;
		if (!space) return;
		showSpaceContextMenuAt(e, space.id);
	}

	function showSpaceContextMenuAt(e: MouseEvent, spaceId: string) {
		const space = spaceStore.spaces.find(s => s.id === spaceId);
		if (!space) return;

		// Build space switcher items at the top
		const spaceItems: import("$lib/stores/contextMenu.svelte").ContextMenuItem[] =
			spaceStore.spaces.map((s, i) => ({
				id: `switch-space-${s.id}`,
				label: s.is_system ? "Virtues" : s.name,
				icon: s.icon || (s.is_system ? "virtues:logo" : "ri:folder-line"),
				checked: s.id === spaceStore.activeSpaceId,
				shortcut: i < 9 ? `⌘${i + 1}` : undefined,
				action: () => {
					if (s.id !== spaceStore.activeSpaceId) {
						spaceStore.switchSpace(s.id);
					}
				},
			}));

		// Build active space options (rename, icon, color, etc.)
		const spaceOptions = buildSpaceContextMenu(space, {
			onRename: (id) => {
				if (id !== spaceStore.activeSpaceId) spaceStore.switchSpace(id, true);
				startRename();
			},
			onChangeIcon: (id) => {
				if (id !== spaceStore.activeSpaceId) spaceStore.switchSpace(id, true);
				startChangeIcon();
			},
			onChangeColor: (id) => {
				if (id !== spaceStore.activeSpaceId) spaceStore.switchSpace(id, true);
				startChangeColor();
			},
			onNewSpace: () => {
				newSpaceName = "";
				showNewSpaceModal = true;
			},
			onSettings: (id) => {
				if (id !== spaceStore.activeSpaceId) spaceStore.switchSpace(id, true);
				openWorkspaceSettings();
			},
			onDelete: async (id) => {
				const target = spaceStore.spaces.find(s => s.id === id);
				if (!target || target.is_system) return;
				if (confirm(`Delete "${target.name}"? Items will be moved to Virtues.`)) {
					await spaceStore.deleteSpace(id);
				}
			},
		});

		// Add divider before space options
		if (spaceOptions.length > 0) {
			spaceOptions[0].dividerBefore = true;
		}

		contextMenu.show({ x: e.clientX, y: e.clientY }, [...spaceItems, ...spaceOptions]);
	}

	async function handleCreateNewSpace() {
		if (!newSpaceName.trim() || isCreatingSpace) return;
		isCreatingSpace = true;
		try {
			await spaceStore.createSpace(newSpaceName.trim());
			showNewSpaceModal = false;
			newSpaceName = "";
		} catch (error) {
			console.error("Failed to create workspace:", error);
		} finally {
			isCreatingSpace = false;
		}
	}

	function openWorkspaceSettings() {
		isWorkspaceInfoOpen = true;
	}

	function closeWorkspaceInfo() {
		isWorkspaceInfoOpen = false;
	}

	function startRename() {
		const activeSpace = spaceStore.activeSpace;
		if (!activeSpace || activeSpace.is_system) return;
		renameValue = activeSpace.name;
		isRenaming = true;
	}

	async function handleRenameDone(newName: string) {
		const activeSpace = spaceStore.activeSpace;
		if (!activeSpace) return;
		isRenaming = false;
		if (newName !== activeSpace.name) {
			try {
				await spaceStore.updateSpace(activeSpace.id, { name: newName });
			} catch (e) {
				console.error("Failed to rename workspace:", e);
			}
		}
	}

	function handleRenameCancel() {
		isRenaming = false;
	}

	function startChangeIcon() {
		const activeSpace = spaceStore.activeSpace;
		if (!activeSpace || activeSpace.is_system) return;
		iconPickerStore.show(activeSpace.icon ?? null, async (icon) => {
			try {
				await spaceStore.updateSpace(activeSpace.id, {
					icon: icon ?? undefined,
				});
			} catch (e) {
				console.error("Failed to update workspace icon:", e);
			}
		});
	}

	function startChangeColor() {
		const activeSpace = spaceStore.activeSpace;
		if (!activeSpace || activeSpace.is_system) return;
		showColorPicker = true;
	}

	async function handleColorSelect(color: string | null) {
		const activeSpace = spaceStore.activeSpace;
		if (!activeSpace) return;
		try {
			await spaceStore.updateSpace(activeSpace.id, {
				accent_color: color ?? undefined,
			});
		} catch (e) {
			console.error("Failed to update workspace color:", e);
		}
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
			onTitleAction={(e) => {
				const space = spaceStore.activeSpace;
				if (space) showSpaceContextMenuAt(e, space.id);
			}}
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

		<!-- Workspace slides with native scroll snap -->
		<div
			class="slides-viewport"
			class:collapsed={isCollapsed}
			bind:this={viewportEl}
			onscroll={handleScroll}
			onpointerdown={handlePointerDown}
			onpointerup={handlePointerUp}
			onpointerleave={handlePointerUp}
			onpointercancel={handlePointerUp}
			ontouchstart={handleTouchStart}
			ontouchend={handleTouchEnd}
		>
			{#each spaceStore.spaces as workspace (workspace.id)}
				<nav
					class="slide"
					class:collapsed={isCollapsed}
					oncontextmenu={(e) =>
						handleSidebarContextMenu(e, workspace)}
				>
					{#if !storeReady}
						<div class="loading-state">
							<Icon
								icon="ri:loader-4-line"
								width="16"
								class="spinner"
							/>
							<span>Loading...</span>
						</div>
					{:else}
						<!-- Unified workspace content: folders + root items -->
						{@const contentItems =
							workspaceContentByWorkspace.get(workspace.id) || []}
						{@const wsAccentColor = workspace.accent_color || null}

						<!-- Folders + root items -->
						<div
							class="workspace-content"
							use:sortableAction={{ workspaceId: workspace.id, immutable: workspace.is_system }}
						>
							{#if contentItems.length === 0 && !workspace.is_system}
								<div class="empty-state">
									<p>This workspace is empty</p>
									<p class="empty-hint">
										Drag items here or right-click to add
										folders
									</p>
								</div>
							{:else}
								{#each contentItems as item (item.id)}
									<div
										class="sidebar-dnd-item"
										data-url={item.url}
										data-is-folder={item.itemType ===
										"folder"
											? "true"
											: null}
										data-source-space-id={item.sourceSpaceId ||
											null}
										data-source-view-id={item.sourceViewId ||
											null}
										data-source-smart-view={item.sourceIsSmartView
											? "true"
											: null}
									>
										{#if item.itemType === "folder" && item.view}
											<UnifiedFolder
												view={item.view}
												collapsed={isCollapsed}
												accentColor={wsAccentColor}
												autoFocusRename={pendingRenameViewId ===
													item.view.id}
												onRenameFocusConsumed={() =>
													(pendingRenameViewId =
														null)}
											/>
										{:else if item.entity}
											<SidebarNavItem
												item={{
													id: item.entity.id,
													type: "link",
													label: item.entity.name,
													icon:
														item.entity.icon ||
														"ri:file-text-line",
													href: item.url,
												}}
												collapsed={isCollapsed}
												accentColor={wsAccentColor}
											/>
										{/if}
									</div>
								{/each}
							{/if}
						</div>
					{/if}
				</nav>
			{/each}
		</div>

		<SidebarFooter
			collapsed={isCollapsed}
			animationDelay={10 * STAGGER_DELAY}
		/>
	</div>
</aside>

<SearchModal open={isSearchOpen} onClose={closeSearch} />
<Modal open={showNewSpaceModal} onClose={() => (showNewSpaceModal = false)} title="New Space" width="sm">
	{#snippet children()}
		<div style="display: flex; flex-direction: column;">
			<label class="modal-label" for="new-space-name">Name</label>
			<input
				bind:value={newSpaceName}
				onkeydown={(e) => { if (e.key === 'Enter') { e.preventDefault(); handleCreateNewSpace(); } if (e.key === 'Escape') { e.preventDefault(); showNewSpaceModal = false; } }}
				id="new-space-name"
				type="text"
				class="modal-input"
				placeholder="My Space"
				autocomplete="off"
			/>
		</div>
	{/snippet}
	{#snippet footer()}
		<button class="modal-btn modal-btn-secondary" onclick={() => (showNewSpaceModal = false)}>
			Cancel
		</button>
		<button
			class="modal-btn modal-btn-primary"
			onclick={handleCreateNewSpace}
			disabled={!newSpaceName.trim() || isCreatingSpace}
		>
			{isCreatingSpace ? "Creating..." : "Create"}
		</button>
	{/snippet}
</Modal>
<WorkspaceInfoModal
	open={isWorkspaceInfoOpen}
	workspace={spaceStore.activeSpace ?? null}
	onClose={closeWorkspaceInfo}
/>
<ColorPickerModal
	open={showColorPicker}
	value={spaceStore.activeSpace?.accent_color ?? null}
	onSelect={handleColorSelect}
	onClose={() => (showColorPicker = false)}
/>

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

	/* Native scroll snap carousel */
	.slides-viewport {
		flex: 1;
		min-height: 0; /* Allow flex shrinking */
		overflow-x: auto;
		overflow-y: hidden;
		scroll-snap-type: x mandatory;
		scroll-behavior: smooth;
		display: flex;
		/* Hide scrollbar */
		scrollbar-width: none;
		-ms-overflow-style: none;
		/* Prevent text selection during drag */
		user-select: none;
		-webkit-user-select: none;
		/* Smooth touch scrolling on iOS */
		-webkit-overflow-scrolling: touch;
	}

	.slides-viewport::-webkit-scrollbar {
		display: none;
	}

	.slides-viewport.collapsed {
		pointer-events: none;
		overflow: hidden;
	}

	.slide {
		flex: 0 0 208px; /* SLIDE_WIDTH - must match JS constant */
		width: 208px;
		min-width: 208px;
		overflow-y: auto;
		overflow-x: hidden;
		padding: 12px 0 12px 8px;
		scroll-snap-align: start;
		scroll-snap-stop: always;
	}

	.slide.collapsed {
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
