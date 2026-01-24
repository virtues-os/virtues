<script lang="ts">
	import { page } from "$app/state";
	import { workspaceStore } from "$lib/stores/workspace.svelte";
	import "iconify-icon";
	import type { SidebarNavItemData } from "./types";

	interface Props {
		item: SidebarNavItemData;
		collapsed?: boolean;
		indent?: number;
	}

	let { item, collapsed = false, indent = 0 }: Props = $props();

	// Calculate padding based on indent level
	const paddingLeft = $derived(10 + indent * 12);

	let showContextMenu = $state(false);
	let contextMenuPos = $state({ x: 0, y: 0 });

	function isActive(href?: string, pagespace?: string): boolean {
		if (!href) return false;

		// Get active tabs from all visible panes (supports split view)
		const activeTabs = workspaceStore.getActiveTabsForSidebar();

		// If we have active tabs, check if ANY of them match this nav item
		if (activeTabs.length > 0) {
			for (const activeTab of activeTabs) {
				// For chat routes with conversationId
				if (pagespace && activeTab.conversationId === pagespace) {
					return true;
				}
				// For exact route match
				if (activeTab.route === href) {
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
		workspaceStore.openTabFromRoute(item.href, {
			forceNew,
			label: item.label,
			preferEmptyPane: true,
		});
	}

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === "Enter" || e.key === " ") {
			e.preventDefault();
			if (item.href) {
				workspaceStore.openTabFromRoute(item.href, {
					label: item.label,
					preferEmptyPane: true,
				});
			}
		}
	}

	function handleContextMenu(e: MouseEvent) {
		if (!item.href || item.type === "action") return;
		e.preventDefault();
		contextMenuPos = { x: e.clientX, y: e.y };
		showContextMenu = true;
	}

	function openInNewTab() {
		if (item.href) {
			workspaceStore.openTabFromRoute(item.href, {
				forceNew: true,
				label: item.label,
				preferEmptyPane: true,
			});
		}
		showContextMenu = false;
	}

	function openInSplitPane() {
		if (item.href) {
			// If not split, enable it
			if (!workspaceStore.split.enabled) {
				workspaceStore.enableSplit();
			}
			// Open in the other pane
			const otherPane = workspaceStore.split.activePaneId === "left" ? "right" : "left";
			workspaceStore.openTabFromRoute(item.href, {
				forceNew: true,
				label: item.label,
				paneId: otherPane,
			});
		}
		showContextMenu = false;
	}

	const active = $derived.by(() => {
		// Access activeTabId directly to track it for reactivity
		const _activeTabId = workspaceStore.activeTabId;
		// Also track split state for reactivity when panes change
		const _splitEnabled = workspaceStore.split.enabled;
		return item.forceActive ?? isActive(item.href, item.pagespace);
	});
</script>

{#if item.type === "action"}
	<button
		onclick={item.onclick}
		class="nav-item"
		class:collapsed
		title={collapsed ? item.label : undefined}
	>
		{#if item.icon}
			<iconify-icon icon={item.icon} width="16" class="nav-icon"
			></iconify-icon>
		{/if}
		{#if !collapsed}
			<span class="nav-label">{item.label}</span>
		{/if}
	</button>
{:else}
	<div
		role="link"
		tabindex="0"
		onclick={handleClick}
		onkeydown={handleKeydown}
		oncontextmenu={handleContextMenu}
		class="nav-item"
		class:active
		class:collapsed
		title={collapsed ? item.label : undefined}
		style="padding-left: {paddingLeft}px"
	>
		{#if item.icon}
			<iconify-icon icon={item.icon} width="16" class="nav-icon"
			></iconify-icon>
		{/if}
		{#if !collapsed}
			<span class="nav-label">{item.label}</span>
		{/if}
	</div>
{/if}

{#if showContextMenu}
	<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
	<div 
		class="context-menu-backdrop" 
		onclick={() => showContextMenu = false}
		oncontextmenu={(e) => { e.preventDefault(); showContextMenu = false; }}
	>
		<div 
			class="context-menu"
			style="top: {contextMenuPos.y}px; left: {contextMenuPos.x}px"
		>
			<button onclick={openInNewTab}>
				<iconify-icon icon="ri:external-link-line"></iconify-icon>
				Open in New Tab
			</button>
			<button onclick={openInSplitPane}>
				<iconify-icon icon="ri:layout-column-line"></iconify-icon>
				Open in Split Pane
			</button>
		</div>
	</div>
{/if}

<style>
	@reference "../../../app.css";

	/* Premium easing for refined feel */
	:root {
		--ease-premium: cubic-bezier(0.2, 0, 0, 1);
	}

	.nav-item {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 6px 10px;
		border-radius: 8px;
		text-decoration: none;
		background: transparent;
		color: var(--color-foreground-muted);
		font-size: 13px;
		border: none;
		cursor: pointer;
		width: 100%;
		text-align: left;
		transition:
			background-color 200ms var(--ease-premium),
			color 200ms var(--ease-premium);
	}

	.nav-item:hover {
		background: color-mix(in srgb, var(--color-foreground) 7%, transparent);
		color: var(--color-foreground);
	}

	/* Active state - zinc shadow style */
	.nav-item.active {
		background: color-mix(in srgb, var(--color-foreground) 9%, transparent);
		color: var(--color-foreground);
		font-weight: 500;
	}

	/* Collapsed state */
	.nav-item.collapsed {
		justify-content: center;
		padding: 8px;
		width: 32px;
		height: 32px;
	}

	/* Icon */
	.nav-icon {
		flex-shrink: 0;
		color: var(--color-foreground-subtle);
		transition: color 200ms var(--ease-premium);
	}

	.nav-item:hover .nav-icon {
		color: var(--color-foreground);
	}

	.nav-item.active .nav-icon {
		color: var(--color-foreground);
	}

	/* Label */
	.nav-label {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		line-height: 1.4;
	}

	/* Focus states for accessibility */
	.nav-item:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: -2px;
	}

	.context-menu-backdrop {
		position: fixed;
		inset: 0;
		z-index: 10000;
		background: transparent;
	}

	.context-menu {
		position: fixed;
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: 8px;
		box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
		padding: 4px;
		min-width: 160px;
		display: flex;
		flex-direction: column;
		gap: 2px;
		animation: menu-fade-in 100ms ease-out;
	}

	@keyframes menu-fade-in {
		from { opacity: 0; transform: scale(0.95); }
		to { opacity: 1; transform: scale(1); }
	}

	.context-menu button {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 8px 10px;
		border-radius: 6px;
		border: none;
		background: transparent;
		color: var(--foreground);
		font-size: 13px;
		cursor: pointer;
		width: 100%;
		text-align: left;
		transition: background-color 100ms ease;
	}

	.context-menu button:hover {
		background: var(--primary-subtle);
	}

	.context-menu button iconify-icon {
		color: var(--foreground-muted);
	}
</style>
