<script lang="ts">
	import { page } from "$app/state";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import Icon from "$lib/components/Icon.svelte";
	import { contextMenu } from "$lib/stores/contextMenu.svelte";
	import { deleteChat, updatePage, updateChat } from "$lib/api/client";
	import { pagesStore } from "$lib/stores/pages.svelte";
	import { chatSessions } from "$lib/stores/chatSessions.svelte";
	import { iconPickerStore } from "$lib/stores/iconPicker.svelte";
	import { getWorkspaceMenuItems } from "$lib/utils/contextMenuItems";
	import { isEmoji } from "$lib/utils/iconHelpers";
	import type { ContextMenuItem } from "$lib/stores/contextMenu.svelte";
	import type { SidebarNavItemData } from "./types";

	interface Props {
		item: SidebarNavItemData;
		collapsed?: boolean;
		indent?: number;
		/** System items can't be removed or deleted */
		isSystemItem?: boolean;
		/** Workspace accent color — shows as a small dot before the icon */
		accentColor?: string | null;
		/** When provided, renders a hover-revealed quick-add (+) button */
		onQuickAdd?: (e: MouseEvent) => void;
		/** Tooltip for the quick-add button */
		quickAddTitle?: string;
	}

	let {
		item,
		collapsed = false,
		indent = 0,
		isSystemItem = false,
		accentColor = null,
		onQuickAdd,
		quickAddTitle,
	}: Props = $props();

	// Indent class for nested items
	const indentClass = $derived(indent === 1 ? 'sidebar-interactive--indent-1' : indent >= 2 ? 'sidebar-interactive--indent-2' : '');

	function isActive(href?: string, pagespace?: string): boolean {
		if (!href) return false;

		// Get active tabs from all visible panes (supports split view)
		const activeTabs = windowShellStore.getActiveTabsForSidebar();

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
		windowShellStore.openTabFromRoute(item.href, {
			forceNew,
			label: item.label,
			preferEmptyPane: true,
		});
	}

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === "Enter" || e.key === " ") {
			e.preventDefault();
			if (item.href) {
				windowShellStore.openTabFromRoute(item.href, {
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
					windowShellStore.openTabFromRoute(href, {
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
					if (!windowShellStore.isSplit) {
						windowShellStore.enableSplit();
					}
					// Open in the other pane
					const otherPane =
						windowShellStore.activePaneId === "left" ? "right" : "left";
					windowShellStore.openTabFromRoute(href, {
						forceNew: true,
						label,
						paneId: otherPane,
					});
				},
			},
		];

		// "Change Icon" for pages and chats
		const parts = href.split('/').filter(Boolean);
		const entityType = parts[0]; // 'page', 'chat', etc.
		const entityId = parts[1];

		if (entityType && entityId && (entityType === 'page' || entityType === 'chat')) {
			items.push({
				id: "change-icon",
				label: "Change Icon",
				icon: "ri:emotion-line",
				action: () => {
					iconPickerStore.show(item.icon ?? null, async (icon) => {
						try {
							if (entityType === 'page') {
								await updatePage(entityId, { icon });
								pagesStore.updatePageLocally(entityId, { icon });
							} else if (entityType === 'chat') {
								await updateChat(entityId, { icon });
								chatSessions.updateSessionIcon(entityId, icon);
							}
							windowShellStore.invalidateViewCache();
						} catch (err) {
							console.error("[SidebarNavItem] Failed to change icon:", err);
						}
					});
				},
			});
		}

		// Add "Add to Space" submenu
		items.push(...getWorkspaceMenuItems(href));

		// Add "Delete" option for deletable entities (pages, chats)
		// Always available if the entity is a page or chat, regardless of isSystemItem
		if (entityType && entityId && (entityType === 'page' || entityType === 'chat')) {
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
							windowShellStore.closeTabsByRoute(`/chat/${entityId}`);
							// Delete the chat
							await deleteChat(entityId);
							// Keep the shared session store (which the sidebar list now
							// binds to) in sync, plus the page cache.
							chatSessions.remove(entityId);
							windowShellStore.invalidateViewCache();
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
		const _activeTabId = windowShellStore.activeTabId;
		// Also track split state for reactivity when panes change
		const _splitEnabled = windowShellStore.isSplit;
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
		{#if item.icon && isEmoji(item.icon)}
			<span class="sidebar-emoji">{item.icon}</span>
		{:else if item.icon}
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
		{#if accentColor && !collapsed}
			<span class="sidebar-accent-dot" style="--dot-color: {accentColor}"></span>
		{/if}
		{#if item.icon && isEmoji(item.icon)}
			<span class="sidebar-emoji">{item.icon}</span>
		{:else if item.icon}
			<Icon icon={item.icon} width="16" class="sidebar-icon" />
		{/if}
		{#if !collapsed}
			<span class="sidebar-label">{item.label}</span>
			{#if item.href}
				<!-- svelte-ignore a11y_click_events_have_key_events -->
				<div class="sidebar-item-actions" onclick={handleContextMenu} role="presentation">
					{#if onQuickAdd}
						<button
							class="sidebar-item-action"
							title={quickAddTitle ?? 'New'}
							onclick={(e) => {
								e.preventDefault();
								e.stopPropagation();
								onQuickAdd(e);
							}}
						>
							<svg width="14" height="14" viewBox="0 0 16 16" fill="none">
								<path d="M8 3.5v9M3.5 8h9" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
							</svg>
						</button>
					{/if}
					<button class="sidebar-item-action" title="More options">
						<Icon icon="ri:more-line" width="14" />
					</button>
				</div>
			{/if}
		{/if}
	</div>
{/if}

<style>
	@reference "../../../app.css";
	@reference "$lib/styles/sidebar.css";
	/* Icon styles are in sidebar.css (globally imported in app.css) */

	.sidebar-emoji {
		font-size: 14px;
		line-height: 16px;
		width: 16px;
		text-align: center;
		flex-shrink: 0;
	}
</style>
