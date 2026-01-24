<script lang="ts">
	import { workspaceStore } from "$lib/stores/workspace.svelte";
	import { bookmarks } from "$lib/stores/bookmarks.svelte";

	interface Props {
		paneId?: "left" | "right"; // When set, renders as a pane tab bar in split mode
	}

	let { paneId }: Props = $props();

	// Context menu state
	let contextMenu = $state<{ x: number; y: number; tabId: string } | null>(
		null,
	);

	// Drag state for pane-to-pane (only show highlight when dragging from different pane)
	let dragOverFromOtherPane = $state(false);

	// Drag state for reordering within pane
	let draggedTabId = $state<string | null>(null);
	let dropTargetIndex = $state<number | null>(null);

	// Rename state
	let renamingTabId = $state<string | null>(null);
	let renameValue = $state("");

	// Derive tabs and active state based on mode
	const tabs = $derived(
		paneId
			? (paneId === "left"
					? workspaceStore.leftPane?.tabs
					: workspaceStore.rightPane?.tabs) || []
			: workspaceStore.tabs,
	);

	const activeTabId = $derived(
		paneId
			? paneId === "left"
				? workspaceStore.leftPane?.activeTabId
				: workspaceStore.rightPane?.activeTabId
			: workspaceStore.activeTabId,
	);

	const isActivePane = $derived(
		paneId ? workspaceStore.split.activePaneId === paneId : true,
	);
	const isSplitMode = $derived(workspaceStore.split.enabled);

	function handleTabClick(id: string) {
		if (paneId) {
			workspaceStore.setActiveTabInPane(id, paneId);
		} else {
			workspaceStore.setActiveTab(id);
		}
	}

	function handleTabClose(e: MouseEvent, id: string) {
		e.stopPropagation();
		if (paneId) {
			workspaceStore.closeTabInPane(id, paneId);
		} else {
			workspaceStore.closeTab(id);
		}
	}

	function handleToggleSplit() {
		workspaceStore.toggleSplit();
	}

	function handleMergePanes() {
		workspaceStore.disableSplit();
	}

	function handleMiddleClick(e: MouseEvent, id: string) {
		if (e.button === 1) {
			e.preventDefault();
			if (paneId) {
				workspaceStore.closeTabInPane(id, paneId);
			} else {
				workspaceStore.closeTab(id);
			}
		}
	}

	function handleContextMenu(e: MouseEvent, tabId: string) {
		e.preventDefault();
		contextMenu = { x: e.clientX, y: e.clientY, tabId };
	}

	function closeContextMenu() {
		contextMenu = null;
	}

	function handleCloseTab() {
		if (!contextMenu) return;
		if (paneId) {
			workspaceStore.closeTabInPane(contextMenu.tabId, paneId);
		} else {
			workspaceStore.closeTab(contextMenu.tabId);
		}
		closeContextMenu();
	}

	function handleCloseOthers() {
		if (!contextMenu) return;
		workspaceStore.closeOtherTabs(contextMenu.tabId, paneId);
		closeContextMenu();
	}

	function handleCloseToRight() {
		if (!contextMenu) return;
		workspaceStore.closeTabsToRight(contextMenu.tabId, paneId);
		closeContextMenu();
	}

	function handleTogglePin() {
		if (!contextMenu) return;
		workspaceStore.togglePin(contextMenu.tabId);
		closeContextMenu();
	}

	// Check if there are tabs to the right
	const hasTabsToRight = $derived(() => {
		const menu = contextMenu;
		if (!menu) return false;
		const index = tabs.findIndex((t) => t.id === menu.tabId);
		return index !== -1 && index < tabs.length - 1;
	});

	// Check if context menu tab is pinned
	const isContextTabPinned = $derived(() => {
		const menu = contextMenu;
		if (!menu) return false;
		const tab = tabs.find((t) => t.id === menu.tabId);
		return tab?.pinned ?? false;
	});

	// Check if context menu tab is bookmarked
	const isContextTabBookmarked = $derived(() => {
		const menu = contextMenu;
		if (!menu) return false;
		const tab = tabs.find((t) => t.id === menu.tabId);
		if (!tab) return false;
		return bookmarks.isRouteBookmarked(tab.route);
	});

	async function handleToggleBookmark() {
		if (!contextMenu) return;
		const tab = tabs.find((t) => t.id === contextMenu.tabId);
		if (!tab) return;

		await bookmarks.toggleRouteBookmark({
			route: tab.route,
			tab_type: tab.type,
			label: tab.label,
			icon: tab.icon,
		});
		closeContextMenu();
	}

	function handleStartRename() {
		if (!contextMenu) return;
		const tab = tabs.find((t) => t.id === contextMenu.tabId);
		if (!tab) return;
		renamingTabId = contextMenu.tabId;
		renameValue = tab.label;
		closeContextMenu();
	}

	function handleRenameSubmit() {
		if (!renamingTabId || !renameValue.trim()) {
			handleRenameCancel();
			return;
		}
		const newLabel = renameValue.trim();
		workspaceStore.updateTab(renamingTabId, { label: newLabel });
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

	// Drag and drop handlers
	function handleDragStart(e: DragEvent, tabId: string) {
		if (!e.dataTransfer) return;
		e.dataTransfer.effectAllowed = "move";
		e.dataTransfer.setData(
			"text/plain",
			JSON.stringify({ tabId, sourcePane: paneId }),
		);
		draggedTabId = tabId;
		workspaceStore.isDragging = true;
	}

	function handleDragEnd() {
		draggedTabId = null;
		dropTargetIndex = null;
		dragOverFromOtherPane = false;
		workspaceStore.isDragging = false;
	}

	function handleDragOver(e: DragEvent) {
		e.preventDefault();
		if (e.dataTransfer) {
			e.dataTransfer.dropEffect = "move";
		}
		// Only show pane highlight in split mode when drag might be from another pane
		// We can't check the source during dragover, so we check if we're in split mode
		// and the drag didn't originate from this pane (draggedTabId would be set if it did)
		if (paneId && !draggedTabId) {
			dragOverFromOtherPane = true;
		}
	}

	function handleDragLeave() {
		dragOverFromOtherPane = false;
	}

	// Handle drag over individual tab for reordering
	function handleTabDragOver(e: DragEvent, index: number) {
		e.preventDefault();
		e.stopPropagation();

		if (!e.dataTransfer) return;
		e.dataTransfer.dropEffect = "move";

		// Calculate if we're in the left or right half of the tab
		const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
		const midpoint = rect.left + rect.width / 2;
		const insertIndex = e.clientX < midpoint ? index : index + 1;

		dropTargetIndex = insertIndex;
	}

	function handleTabDragLeave() {
		// Don't clear immediately - let the next dragover set it
	}

	function handleDrop(e: DragEvent) {
		e.preventDefault();
		dragOverFromOtherPane = false;

		if (!e.dataTransfer) return;

		try {
			const data = JSON.parse(e.dataTransfer.getData("text/plain"));
			const { tabId, sourcePane } = data;

			// Handle reordering within the same pane
			if (sourcePane === paneId || (!sourcePane && !paneId)) {
				if (dropTargetIndex !== null) {
					const fromIndex = tabs.findIndex((t) => t.id === tabId);
					if (
						fromIndex !== -1 &&
						fromIndex !== dropTargetIndex &&
						fromIndex !== dropTargetIndex - 1
					) {
						// Adjust target index if we're moving forward
						const toIndex =
							dropTargetIndex > fromIndex
								? dropTargetIndex - 1
								: dropTargetIndex;
						if (paneId) {
							workspaceStore.reorderTabsInPane(
								fromIndex,
								toIndex,
								paneId,
							);
						} else {
							workspaceStore.reorderTabs(fromIndex, toIndex);
						}
					}
				}
			}
			// Handle moving to a different pane
			else if (sourcePane !== paneId && paneId && sourcePane) {
				workspaceStore.moveTabToPane(tabId, paneId);
			}
		} catch {
			// Invalid data, ignore
		}

		draggedTabId = null;
		dropTargetIndex = null;
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
			case "data-entities":
				return "ri:node-tree";
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

<svelte:window onclick={closeContextMenu} />

<div
	class="tab-bar"
	class:split-pane={!!paneId}
	class:active-pane={isActivePane && !!paneId}
	class:drag-over={dragOverFromOtherPane}
	ondragover={paneId ? handleDragOver : undefined}
	ondragleave={paneId ? handleDragLeave : undefined}
	ondrop={paneId ? handleDrop : undefined}
	role="toolbar"
	aria-label="Tab bar"
	tabindex="0"
>
	<!-- Only show nav buttons on left pane or when not in split mode -->
	{#if !paneId || paneId === "left"}
		<div class="nav-buttons">
			<button
				class="nav-button"
				class:disabled={!workspaceStore.canGoBack()}
				onclick={() => workspaceStore.goBack()}
				aria-label="Go back"
				title="Go back"
				disabled={!workspaceStore.canGoBack()}
			>
				<iconify-icon icon="ri:arrow-left-s-line"></iconify-icon>
			</button>
			<button
				class="nav-button"
				class:disabled={!workspaceStore.canGoForward()}
				onclick={() => workspaceStore.goForward()}
				aria-label="Go forward"
				title="Go forward"
				disabled={!workspaceStore.canGoForward()}
			>
				<iconify-icon icon="ri:arrow-right-s-line"></iconify-icon>
			</button>
		</div>
	{/if}

	<div
		class="tabs-scroll"
		ondragover={handleDragOver}
		ondragleave={handleDragLeave}
		ondrop={handleDrop}
		role="tablist"
		tabindex="0"
	>
		{#each tabs as tab, index (tab.id)}
			{@const showDropIndicatorBefore =
				dropTargetIndex === index && draggedTabId !== tab.id}
			{@const showDropIndicatorAfter =
				dropTargetIndex === index + 1 &&
				index === tabs.length - 1 &&
				draggedTabId !== tab.id}
			{#if showDropIndicatorBefore}
				<div class="drop-indicator"></div>
			{/if}
			<div
				class="tab"
				class:active={tab.id === activeTabId}
				class:active-in-active-pane={tab.id === activeTabId &&
					isActivePane}
				class:dragging={tab.id === draggedTabId}
				class:pinned={tab.pinned}
				class:renaming={tab.id === renamingTabId}
				draggable={tab.id !== renamingTabId ? "true" : "false"}
				ondragstart={(e) => handleDragStart(e, tab.id)}
				ondragend={handleDragEnd}
				ondragover={(e) => handleTabDragOver(e, index)}
				ondragleave={handleTabDragLeave}
				onclick={() =>
					tab.id !== renamingTabId && handleTabClick(tab.id)}
				ondblclick={(e) => handleDoubleClick(e, tab.id)}
				onauxclick={(e) => handleMiddleClick(e, tab.id)}
				oncontextmenu={(e) => handleContextMenu(e, tab.id)}
				onkeydown={(e) =>
					e.key === "Enter" &&
					tab.id !== renamingTabId &&
					handleTabClick(tab.id)}
				title={tab.id === renamingTabId ? "" : tab.label}
				role="button"
				tabindex="0"
			>
			<iconify-icon
				icon={tab.icon || getDefaultIcon(tab.type)}
				class="tab-icon"
			></iconify-icon>
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
					<iconify-icon icon="ri:close-line"></iconify-icon>
				</button>
			{/if}
			</div>
			{#if showDropIndicatorAfter}
				<div class="drop-indicator"></div>
			{/if}
		{/each}
	</div>

	{#if !paneId}
		<button
			class="split-toggle"
			onclick={handleToggleSplit}
			aria-label="Split view"
			title="Split view"
		>
			<iconify-icon icon="ri:layout-column-line"></iconify-icon>
		</button>
	{/if}

	{#if paneId === "right" && isSplitMode}
		<button
			class="merge-toggle"
			onclick={handleMergePanes}
			aria-label="Merge panes"
			title="Merge panes"
		>
			<iconify-icon icon="ri:layout-right-line"></iconify-icon>
		</button>
	{/if}
</div>

<!-- Context Menu -->
{#if contextMenu}
	<div
		class="context-menu"
		style="left: {contextMenu.x}px; top: {contextMenu.y}px;"
		role="menu"
	>
		<button
			class="context-menu-item"
			onclick={handleTogglePin}
			role="menuitem"
		>
			<iconify-icon
				icon={isContextTabPinned()
					? "ri:unpin-line"
					: "ri:pushpin-line"}
				width="14"
			></iconify-icon>
			{isContextTabPinned() ? "Unpin" : "Pin"}
		</button>
		<button
			class="context-menu-item"
			onclick={handleToggleBookmark}
			role="menuitem"
		>
			<iconify-icon
				icon={isContextTabBookmarked()
					? "ri:bookmark-fill"
					: "ri:bookmark-line"}
				width="14"
			></iconify-icon>
			{isContextTabBookmarked() ? "Remove Bookmark" : "Bookmark"}
		</button>
		<button
			class="context-menu-item"
			onclick={handleStartRename}
			role="menuitem"
		>
			<iconify-icon icon="ri:edit-line" width="14"></iconify-icon>
			Rename
		</button>
		<div class="context-menu-divider"></div>
		<button
			class="context-menu-item"
			onclick={handleCloseTab}
			role="menuitem"
		>
			Close
		</button>
		{#if tabs.length > 1}
			<button
				class="context-menu-item"
				onclick={handleCloseOthers}
				role="menuitem"
			>
				Close Others
			</button>
		{/if}
		{#if hasTabsToRight()}
			<button
				class="context-menu-item"
				onclick={handleCloseToRight}
				role="menuitem"
			>
				Close to the Right
			</button>
		{/if}
	</div>
{/if}

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

	.nav-buttons {
		display: flex;
		align-items: center;
		gap: 2px;
		margin-right: 4px;
	}

	.nav-button {
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
		transition:
			background-color 150ms ease,
			color 150ms ease;
	}

	.nav-button:hover {
		background: var(--color-surface-elevated);
		color: var(--color-foreground);
	}

	.nav-button:active:not(:disabled) {
		background: var(--color-border);
	}

	.nav-button.disabled,
	.nav-button:disabled {
		opacity: 0.3;
		cursor: default;
	}

	/* Active pane in split mode gets elevated background */
	.tab-bar.active-pane {
		background: var(--color-surface-elevated);
	}

	.tabs-scroll {
		display: flex;
		align-items: center;
		gap: 6px;
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
			color 150ms ease;
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

	.tab.pinned .tab-icon {
		color: var(--color-primary);
		opacity: 1;
	}

	.tab-icon {
		flex-shrink: 0;
		font-size: 13px;
		opacity: 0.7;
	}

	.tab.active .tab-icon {
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

	/* Drag over indicator */
	.tab-bar.drag-over {
		background: color-mix(
			in srgb,
			var(--color-primary) 10%,
			var(--color-surface)
		);
	}

	/* Tab being dragged */
	.tab.dragging {
		opacity: 0.5;
	}

	/* Drop indicator line */
	.drop-indicator {
		width: 2px;
		height: 20px;
		background: var(--color-primary);
		border-radius: 1px;
		flex-shrink: 0;
		margin: 0 -1px;
	}

	/* Context Menu */
	.context-menu {
		position: fixed;
		z-index: 1000;
		min-width: 160px;
		padding: 4px;
		background: var(--color-surface-elevated);
		border: 1px solid var(--color-border);
		border-radius: 8px;
		box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
	}

	.context-menu-item {
		display: flex;
		align-items: center;
		gap: 8px;
		width: 100%;
		padding: 8px 12px;
		border: none;
		border-radius: 4px;
		background: transparent;
		color: var(--color-foreground);
		font-size: 13px;
		text-align: left;
		cursor: pointer;
		transition: background-color 100ms ease;
	}

	.context-menu-item:hover {
		background: var(--color-surface);
	}

	.context-menu-item iconify-icon {
		color: var(--color-foreground-muted);
		flex-shrink: 0;
	}

	.context-menu-divider {
		height: 1px;
		background: var(--color-border);
		margin: 4px 0;
	}
</style>
