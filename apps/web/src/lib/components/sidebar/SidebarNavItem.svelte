<script lang="ts">
	import { page } from "$app/state";
	import { spaceStore } from "$lib/stores/space.svelte";
	import Icon from "$lib/components/Icon.svelte";
	import { contextMenu } from "$lib/stores/contextMenu.svelte";
	import { removeViewItem, deleteChat } from "$lib/api/client";
	import { pagesStore } from "$lib/stores/pages.svelte";
	import { getWorkspaceMenuItems } from "$lib/utils/contextMenuItems";
	import type { ContextMenuItem } from "$lib/stores/contextMenu.svelte";
	import type { SidebarNavItemData } from "./types";

	/**
	 * Context passed when this item is rendered inside a folder (view).
	 * Enables folder-specific context menu options like "Remove from Folder".
	 */
	interface FolderContext {
		viewId: string;
		isSystemFolder: boolean;
	}

	interface Props {
		item: SidebarNavItemData;
		collapsed?: boolean;
		indent?: number;
		/** When set, this item is inside a folder and can be removed from it */
		inFolderContext?: FolderContext;
		/** System items (e.g., VirtuesWorkspaceNav) can't be removed or deleted */
		isSystemItem?: boolean;
	}

	let { item, collapsed = false, indent = 0, inFolderContext, isSystemItem = false }: Props = $props();

	// Indent class for nested items
	const indentClass = $derived(indent === 1 ? 'sidebar-interactive--indent-1' : indent >= 2 ? 'sidebar-interactive--indent-2' : '');

	function isActive(href?: string, pagespace?: string): boolean {
		if (!href) return false;

		// Get active tabs from all visible panes (supports split view)
		const activeTabs = spaceStore.getActiveTabsForSidebar();

		// If we have active tabs, check if ANY of them match this nav item
		if (activeTabs.length > 0) {
			for (const activeTab of activeTabs) {
				// For exact route match
				if (activeTab.route === href) {
					return true;
				}
				// For pagespace-based matching (e.g., pagespace="chat" matches route "/chat/...")
				if (pagespace && activeTab.route.startsWith(`/${pagespace}`)) {
					return true;
				}
			}
			// Active tabs exist but none match this item
			return false;
		}

		// Fallback to URL-based checking ONLY when there are no active tabs
		// (e.g., during initial page load before tab system initializes)
		if (page.url.pathname === href) {
			return true;
		}

		if (pagespace === "") {
			return page.url.pathname === "/";
		}

		if (pagespace) {
			return page.url.pathname.startsWith(`/${pagespace}`);
		}

		return false;
	}

	function handleClick(e: MouseEvent) {
		if (!item.href) return;

		e.preventDefault();

		// Cmd/Ctrl+click forces a new tab
		const forceNew = e.metaKey || e.ctrlKey;
		// Pass the item label so chat tabs show proper titles like "Google Antigravity..."
		// preferEmptyPane: true so sidebar clicks can open in empty panes in split view
		spaceStore.openTabFromRoute(item.href, {
			forceNew,
			label: item.label,
			preferEmptyPane: true,
		});
	}

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === "Enter" || e.key === " ") {
			e.preventDefault();
			if (item.href) {
				spaceStore.openTabFromRoute(item.href, {
					label: item.label,
					preferEmptyPane: true,
				});
			}
		}
	}

	function handleContextMenu(e: MouseEvent) {
		if (!item.href || item.type === "action") return;
		e.preventDefault();
		e.stopPropagation(); // Prevent slide's context menu from overwriting

		const href = item.href;
		const label = item.label;

		// Build context menu items
		const items: ContextMenuItem[] = [
			// Open in New Tab
			{
				id: "open-new-tab",
				label: "Open in New Tab",
				icon: "ri:external-link-line",
				action: () => {
					spaceStore.openTabFromRoute(href, {
						forceNew: true,
						label,
						preferEmptyPane: true,
					});
				},
			},
			// Open in Split Pane
			{
				id: "open-split",
				label: "Open in Split Pane",
				icon: "ri:layout-column-line",
				action: () => {
					// If not split, enable it
					if (!spaceStore.isSplit) {
						spaceStore.enableSplit();
					}
					// Open in the other pane
					const otherPane =
						spaceStore.activePaneId === "left" ? "right" : "left";
					spaceStore.openTabFromRoute(href, {
						forceNew: true,
						label,
						paneId: otherPane,
					});
				},
			},
		];

		// Add "Add to Folder" / "Move to Workspace" submenus
		items.push(...getWorkspaceMenuItems(href));

		// If inside a non-system folder, add "Remove from Folder" option
		if (inFolderContext && !inFolderContext.isSystemFolder) {
			items.push({
				id: "remove-from-folder",
				label: "Remove from Folder",
				icon: "ri:close-line",
				dividerBefore: true,
				action: async () => {
					try {
						await removeViewItem(inFolderContext.viewId, href);
						spaceStore.invalidateViewCache();
					} catch (err) {
						console.error("[SidebarNavItem] Failed to remove from folder:", err);
					}
				},
			});
		}

		// If NOT inside a folder and NOT a system item, this is a root-level workspace item - allow removal
		if (!inFolderContext && !isSystemItem) {
			items.push({
				id: "remove-from-workspace",
				label: "Remove from Workspace",
				icon: "ri:close-circle-line",
				dividerBefore: true,
				action: async () => {
					try {
						await spaceStore.removeSpaceItem(href);
						spaceStore.invalidateViewCache();
					} catch (err) {
						console.error("[SidebarNavItem] Failed to remove from workspace:", err);
					}
				},
			});
		}

		// Add "Delete" option for deletable entities (pages, chats) - not for system items
		// Parse the href to determine entity type
		const parts = href.split('/').filter(Boolean);
		const entityType = parts[0]; // 'page', 'chat', etc.
		const entityId = parts[1];

		if (!isSystemItem && entityType && entityId && (entityType === 'page' || entityType === 'chat')) {
			items.push({
				id: "delete",
				label: "Delete",
				icon: "ri:delete-bin-line",
				variant: "destructive",
				action: async () => {
					try {
						if (entityType === 'page') {
							// Use pagesStore.removePage() which handles all side effects
							await pagesStore.removePage(entityId);
						} else if (entityType === 'chat') {
							// Close any open tabs for this chat first
							spaceStore.closeTabsByRoute(`/chat/${entityId}`);
							// Delete the chat
							await deleteChat(entityId);
							// Invalidate cache and refresh sidebar
							spaceStore.invalidateViewCache();
							if (!spaceStore.isSystemSpace) {
								await spaceStore.loadSpaceItems();
							}
						}
					} catch (err) {
						console.error("[SidebarNavItem] Failed to delete:", err);
					}
				},
			});
		}

		contextMenu.show({ x: e.clientX, y: e.clientY }, items);
	}

	const active = $derived.by(() => {
		// Access activeTabId directly to track it for reactivity
		const _activeTabId = spaceStore.activeTabId;
		// Also track split state for reactivity when panes change
		const _splitEnabled = spaceStore.isSplit;
		return item.forceActive ?? isActive(item.href, item.pagespace);
	});
</script>

{#if item.type === "action"}
	<button
		onclick={item.onclick}
		class="sidebar-interactive {indentClass}"
		class:collapsed
		title={collapsed ? item.label : undefined}
	>
		{#if item.icon}
			<Icon icon={item.icon} width="16" class="sidebar-icon" />
		{/if}
		{#if !collapsed}
			<span class="sidebar-label">{item.label}</span>
		{/if}
	</button>
{:else}
	<div
		role="link"
		tabindex="0"
		onclick={handleClick}
		onkeydown={handleKeydown}
		oncontextmenu={handleContextMenu}
		class="sidebar-interactive {indentClass}"
		class:active
		class:collapsed
		title={collapsed ? item.label : undefined}
	>
		{#if item.icon}
			<Icon icon={item.icon} width="16" class="sidebar-icon" />
		{/if}
		{#if !collapsed}
			<span class="sidebar-label">{item.label}</span>
		{/if}
	</div>
{/if}

<style>
	@reference "../../../app.css";
	@reference "$lib/styles/sidebar.css";
	/* Icon styles are in sidebar.css (globally imported in app.css) */
</style>
