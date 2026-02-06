<script lang="ts">
	import { flip } from "svelte/animate";
	import { dndzone } from "svelte-dnd-action";
	import type { DndEvent } from "svelte-dnd-action";
	import Icon from "$lib/components/Icon.svelte";
	import { spaceStore } from "$lib/stores/space.svelte";
	import type { Tab } from "$lib/stores/space.svelte";
	import {
		dndManager,
		type DndTabItem,
		type ZoneId,
	} from "$lib/stores/dndManager.svelte";
	import { contextMenu } from "$lib/stores/contextMenu.svelte";
	import { getWorkspaceMenuItems } from "$lib/utils/contextMenuItems";
	import type { ContextMenuItem } from "$lib/stores/contextMenu.svelte";

	const FLIP_DURATION_MS = 150;

	interface Props {
		paneId?: "left" | "right"; // When set, renders as a pane tab bar in split mode
	}

	let { paneId }: Props = $props();

	// Rename state
	let renamingTabId = $state<string | null>(null);
	let renameValue = $state("");

	// DnD items state - we need mutable state for svelte-dnd-action
	let dndItems = $state<DndTabItem[]>([]);

	// Zone identifier for this tab bar instance
	const zoneId: ZoneId = $derived({
		type: "tab-bar" as const,
		paneId,
	});

	// Derive tabs and active state based on mode
	const tabs = $derived(
		paneId
			? (paneId === "left"
					? spaceStore.leftPane?.tabs
					: spaceStore.rightPane?.tabs) || []
			: spaceStore.tabs,
	);

	const activeTabId = $derived(
		paneId
			? paneId === "left"
				? spaceStore.leftPane?.activeTabId
				: spaceStore.rightPane?.activeTabId
			: spaceStore.activeTabId,
	);

	const isActivePane = $derived(
		paneId ? spaceStore.activePaneId === paneId : true,
	);
	const isSplitMode = $derived(spaceStore.isSplit);

	// Build DnD items from tabs with source information
	function buildDndItems(): DndTabItem[] {
		return tabs.map((tab) => ({
			id: tab.id,
			url: tab.route,
			label: tab.label,
			icon: tab.icon,
			source: zoneId,
			tab,
		}));
	}

	// Sync DnD items when tabs change
	$effect(() => {
		// Rebuild from tabs (the source of truth)
		dndItems = buildDndItems();
	});

	function handleTabClick(id: string) {
		if (paneId) {
			spaceStore.setActiveTabInPane(id, paneId);
		} else {
			spaceStore.setActiveTab(id);
		}
	}

	function handleTabClose(e: MouseEvent, id: string) {
		e.stopPropagation();
		if (paneId) {
			spaceStore.closeTabInPane(id, paneId);
		} else {
			spaceStore.closeTab(id);
		}
	}

	function handleToggleSplit() {
		spaceStore.toggleSplit();
	}

	function handleMergePanes() {
		spaceStore.disableSplit();
	}

	function handleMiddleClick(e: MouseEvent, id: string) {
		if (e.button === 1) {
			e.preventDefault();
			if (paneId) {
				spaceStore.closeTabInPane(id, paneId);
			} else {
				spaceStore.closeTab(id);
			}
		}
	}

	function handleContextMenu(e: MouseEvent, tabId: string) {
		e.preventDefault();

		const tab = tabs.find((t) => t.id === tabId);
		if (!tab) return;

		const tabIndex = tabs.findIndex((t) => t.id === tabId);
		const hasTabsToRight = tabIndex !== -1 && tabIndex < tabs.length - 1;

		// Build context menu items
		const items: ContextMenuItem[] = [
			// Compact/Expand
			{
				id: "compact",
				label: tab.pinned ? "Expand" : "Compact",
				icon: tab.pinned ? "ri:expand-left-right-line" : "ri:contract-left-right-line",
				action: () => spaceStore.togglePin(tabId),
			},
			// Rename
			{
				id: "rename",
				label: "Rename",
				icon: "ri:edit-line",
				action: () => {
					renamingTabId = tabId;
					renameValue = tab.label;
				},
			},
			// Divider + Close actions
			{
				id: "close",
				label: "Close",
				dividerBefore: true,
				action: () => {
					if (paneId) {
						spaceStore.closeTabInPane(tabId, paneId);
					} else {
						spaceStore.closeTab(tabId);
					}
				},
			},
		];

		// Close Others (only if more than 1 tab)
		if (tabs.length > 1) {
			items.push({
				id: "close-others",
				label: "Close Others",
				action: () => spaceStore.closeOtherTabs(tabId, paneId),
			});
		}

		// Close to Right (only if tabs exist to the right)
		if (hasTabsToRight) {
			items.push({
				id: "close-to-right",
				label: "Close to the Right",
				action: () => spaceStore.closeTabsToRight(tabId, paneId),
			});
		}

		// Add "Add to Folder" / "Move to Workspace" submenus if tab has a route
		if (tab.route) {
			items.push(...getWorkspaceMenuItems(tab.route));
		}

		contextMenu.show({ x: e.clientX, y: e.clientY }, items);
	}

	function handleRenameSubmit() {
		if (!renamingTabId || !renameValue.trim()) {
			handleRenameCancel();
			return;
		}
		const newLabel = renameValue.trim();
		spaceStore.updateTab(renamingTabId, { label: newLabel });
		renamingTabId = null;
		renameValue = "";
	}

	function handleRenameCancel() {
		renamingTabId = null;
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

	function handleDoubleClick(e: MouseEvent, tabId: string) {
		e.preventDefault();
		const tab = tabs.find((t) => t.id === tabId);
		if (!tab || tab.pinned) return;
		renamingTabId = tabId;
		renameValue = tab.label;
	}

	// svelte-dnd-action handlers - delegate to centralized dndManager
	function handleDndConsider(e: CustomEvent<DndEvent<DndTabItem>>) {
		// Pass current dndItems as originalItems - svelte-dnd-action modifies the array
		// before firing consider, so we need the pre-modified version to find the dragged item
		dndManager.handleConsider(
			e,
			zoneId,
			(items) => {
				dndItems = items;
			},
			dndItems,
		);
	}

	function handleDndFinalize(e: CustomEvent<DndEvent<DndTabItem>>) {
		dndManager.handleFinalize(e, zoneId, (items) => {
			dndItems = items;
		});
	}

	// Get icon for tab type
	function getDefaultIcon(type: string): string {
		switch (type) {
			case "chat":
				return "ri:chat-1-line";
			case "history":
				return "ri:history-line";
			case "wiki":
				return "ri:book-2-line";
			case "wiki-list":
				return "ri:list-check";
			case "data-sources":
				return "ri:database-2-line";
			case "data-sources-add":
				return "ri:add-circle-line";
			case "data-jobs":
				return "ri:refresh-line";
			case "storage":
				return "ri:hard-drive-2-line";
			case "usage":
				return "ri:bar-chart-line";
			case "profile":
				return "ri:user-settings-line";
			default:
				return "ri:file-line";
		}
	}
</script>

<div
	class="tab-bar"
	class:split-pane={!!paneId}
	class:active-pane={isActivePane && !!paneId}
	role="toolbar"
	aria-label="Tab bar"
	tabindex="0"
>
	<div
		class="tabs-scroll"
		role="tablist"
		tabindex="0"
		use:dndzone={{
			items: dndItems,
			type: "tab",
			flipDurationMs: FLIP_DURATION_MS,
			dropTargetStyle: {},
			dragDisabled: renamingTabId !== null,
		}}
		onconsider={handleDndConsider}
		onfinalize={handleDndFinalize}
	>
		{#each dndItems as item (item.id)}
			{@const tab = item.tab}
			<div
				class="tab"
				class:active={tab.id === activeTabId}
				class:active-in-active-pane={tab.id === activeTabId &&
					isActivePane}
				class:pinned={tab.pinned}
				class:renaming={tab.id === renamingTabId}
				animate:flip={{ duration: FLIP_DURATION_MS }}
				onclick={() =>
					tab.id !== renamingTabId && handleTabClick(tab.id)}
				ondblclick={(e) => handleDoubleClick(e, tab.id)}
				onauxclick={(e) => handleMiddleClick(e, tab.id)}
				oncontextmenu={(e) => handleContextMenu(e, tab.id)}
				onkeydown={(e) =>
					e.key === "Enter" &&
					tab.id !== renamingTabId &&
					handleTabClick(tab.id)}
				title={tab.id !== renamingTabId ? tab.label : ""}
				role="button"
				tabindex="0"
			>
				<Icon
					icon={item.icon || getDefaultIcon(tab.type)}
					class="tab-icon"
				/>
				{#if !tab.pinned}
					{#if tab.id === renamingTabId}
						<!-- svelte-ignore a11y_autofocus -->
						<input
							type="text"
							class="tab-rename-input"
							bind:value={renameValue}
							onkeydown={handleRenameKeydown}
							onblur={handleRenameSubmit}
							onclick={(e) => e.stopPropagation()}
							autofocus
						/>
					{:else}
						<span class="tab-label">{tab.label}</span>
					{/if}
				{/if}
				{#if !tab.pinned && tab.id !== renamingTabId}
					<button
						class="tab-close"
						onclick={(e) => handleTabClose(e, tab.id)}
						aria-label="Close tab"
					>
						<Icon icon="ri:close-line" />
					</button>
				{/if}
			</div>
		{/each}
	</div>

	{#if !paneId}
		<button
			class="split-toggle"
			onclick={handleToggleSplit}
			aria-label="Split view"
			title="Split view"
		>
			<Icon icon="ri:layout-column-line" />
		</button>
	{/if}

	{#if paneId === "right" && isSplitMode}
		<button
			class="merge-toggle"
			onclick={handleMergePanes}
			aria-label="Merge panes"
			title="Merge panes"
		>
			<Icon icon="ri:layout-right-line" />
		</button>
	{/if}
</div>

<style>
	.tab-bar {
		display: flex;
		align-items: center;
		gap: 4px;
		padding: 6px 8px;
		border-bottom: 1px solid var(--color-border);
		background: var(--color-surface);
		flex-shrink: 0;
		position: relative;
		z-index: 110; /* Above global drag overlays */
	}

	/* Active pane in split mode gets elevated background */
	.tab-bar.active-pane {
		background: var(--color-surface-elevated);
	}

	.tabs-scroll {
		display: flex;
		align-items: center;
		gap: 4px;
		overflow-x: auto;
		flex: 1;
		scrollbar-width: none;
		height: 28px;
	}

	.tabs-scroll::-webkit-scrollbar {
		display: none;
	}

	.tab {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 5px 8px;
		height: 24px;
		border: none;
		border-radius: 6px;
		background: transparent;
		color: var(--color-foreground-muted);
		font-size: 12px;
		cursor: pointer;
		white-space: nowrap;
		max-width: 160px;
		min-width: 80px;
		flex-shrink: 0;
		transition:
			background-color 150ms ease,
			color 150ms ease,
			opacity 150ms ease;
	}

	.tab:hover {
		background: var(--color-surface-elevated);
		color: var(--color-foreground);
	}

	.tab.active {
		background: var(--color-surface-elevated);
		color: var(--color-foreground);
	}

	/* Active tab in the active pane gets darker background */
	.tab.active-in-active-pane {
		background: var(--color-border);
	}

	/* Dragging state - svelte-dnd-action applies aria-grabbed */
	:global(.tab[aria-grabbed="true"]) {
		opacity: 0.5;
	}

	/* Pinned tabs are compact (icon only) with subtle tint */
	.tab.pinned {
		min-width: auto;
		max-width: none;
		padding: 5px 8px;
		gap: 0;
		background: color-mix(in srgb, var(--color-primary) 15%, transparent);
	}

	.tab.pinned:hover {
		background: color-mix(in srgb, var(--color-primary) 25%, transparent);
	}

	.tab.pinned :global(.tab-icon) {
		color: var(--color-primary);
		opacity: 1;
	}

	:global(.tab-icon) {
		flex-shrink: 0;
		font-size: 13px;
		opacity: 0.7;
	}

	.tab.active :global(.tab-icon) {
		opacity: 1;
	}

	.tab-label {
		overflow: hidden;
		text-overflow: ellipsis;
		flex: 1;
		text-align: left;
	}

	.tab-rename-input {
		flex: 1;
		min-width: 60px;
		padding: 0;
		border: none;
		background: transparent;
		color: var(--color-foreground);
		font-size: 12px;
		font-family: inherit;
		outline: none;
		caret-color: var(--color-primary);
	}

	.tab.renaming {
		background: color-mix(in srgb, var(--color-primary) 20%, transparent);
		cursor: text;
	}

	.tab-close {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 16px;
		height: 16px;
		padding: 0;
		border: none;
		border-radius: 4px;
		background: transparent;
		color: var(--color-foreground-muted);
		font-size: 12px;
		cursor: pointer;
		opacity: 0;
		transition:
			opacity 150ms ease,
			background-color 150ms ease;
		flex-shrink: 0;
	}

	.tab:hover .tab-close,
	.tab.active .tab-close {
		opacity: 1;
	}

	.tab-close:hover {
		background: var(--color-border);
		color: var(--color-foreground);
	}

	.split-toggle,
	.merge-toggle {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 24px;
		height: 24px;
		padding: 0;
		border: none;
		border-radius: 6px;
		background: transparent;
		color: var(--color-foreground-muted);
		font-size: 16px;
		cursor: pointer;
		flex-shrink: 0;
		transition:
			background-color 150ms ease,
			color 150ms ease;
	}

	.split-toggle:hover,
	.merge-toggle:hover {
		background: var(--color-surface-elevated);
		color: var(--color-foreground);
	}

	/* svelte-dnd-action drop indicator */
	:global(.tabs-scroll > [data-is-dnd-shadow-item-hint="true"]) {
		width: 2px !important;
		min-width: 2px !important;
		max-width: 2px !important;
		height: 20px !important;
		padding: 0 !important;
		margin: 0 2px;
		background: var(--color-primary) !important;
		border-radius: 1px;
		opacity: 1;
		animation: pulse 0.8s ease-in-out infinite;
	}

	@keyframes pulse {
		0%,
		100% {
			opacity: 1;
		}
		50% {
			opacity: 0.5;
		}
	}
</style>
