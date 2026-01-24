<script lang="ts">
	import { workspaceStore } from "$lib/stores/workspace.svelte";
	import WindowTabBar from "./WindowTabBar.svelte";
	import TabContent from "./TabContent.svelte";
	import DiscoveryPage from "./views/DiscoveryPage.svelte";

	let isDragging = $state(false);
	let containerRef = $state<HTMLElement | null>(null);

	const MIN_WIDTH = 25; // minimum 25% (1/4) for each pane

	// Derived state for split mode
	const isSplitEnabled = $derived(workspaceStore.split.enabled);

	// Compute left pane tabs - in mono mode use tabs[], in split mode use left pane
	const leftPaneTabs = $derived(
		isSplitEnabled && workspaceStore.split.panes
			? workspaceStore.split.panes[0].tabs
			: workspaceStore.tabs,
	);

	const leftPaneActiveTabId = $derived(
		isSplitEnabled && workspaceStore.split.panes
			? workspaceStore.split.panes[0].activeTabId
			: workspaceStore.activeTabId,
	);

	// Right pane only used in split mode
	const rightPaneTabs = $derived(
		isSplitEnabled && workspaceStore.split.panes
			? workspaceStore.split.panes[1].tabs
			: [],
	);

	const rightPaneActiveTabId = $derived(
		isSplitEnabled && workspaceStore.split.panes
			? workspaceStore.split.panes[1].activeTabId
			: null,
	);

	// Compute widths
	const leftWidth = $derived(
		isSplitEnabled ? workspaceStore.leftPane?.width || 50 : 100,
	);

	const rightWidth = $derived(
		isSplitEnabled ? workspaceStore.rightPane?.width || 50 : 0,
	);

	function handleMouseDown(e: MouseEvent) {
		if (!isSplitEnabled) return;
		e.preventDefault();
		isDragging = true;
	}

	function handleMouseMove(e: MouseEvent) {
		if (!isDragging || !containerRef || !isSplitEnabled) return;

		const rect = containerRef.getBoundingClientRect();
		const newWidth = ((e.clientX - rect.left) / rect.width) * 100;
		const clampedWidth = Math.max(
			MIN_WIDTH,
			Math.min(100 - MIN_WIDTH, newWidth),
		);

		workspaceStore.setPaneWidth(clampedWidth);
	}

	function handleMouseUp() {
		isDragging = false;
	}

	function handlePaneClick(paneId: "left" | "right") {
		if (!isSplitEnabled) return;
		workspaceStore.setActivePane(paneId);
	}

	// Drag to split state
	let dragOverSide = $state<"left" | "right" | null>(null);

	function handleGlobalDragOver(e: DragEvent, side: "left" | "right") {
		e.preventDefault();
		if (e.dataTransfer) {
			e.dataTransfer.dropEffect = "move";
		}
		if (dragOverSide !== side) {
			dragOverSide = side;
		}
	}

	function handleGlobalDragLeave() {
		dragOverSide = null;
	}

	function handleGlobalDrop(e: DragEvent, side: "left" | "right") {
		e.preventDefault();
		dragOverSide = null;

		if (!e.dataTransfer) return;

		try {
			const data = JSON.parse(e.dataTransfer.getData("text/plain"));
			const { tabId } = data;

			if (tabId) {
				// If not in split mode, enable it first
				if (!workspaceStore.split.enabled) {
					workspaceStore.enableSplit();
				}
				workspaceStore.moveTabToPane(tabId, side);
			}
		} catch (err) {
			console.error("Failed to handle drop:", err);
		}
	}
</script>

<svelte:window on:mousemove={handleMouseMove} on:mouseup={handleMouseUp} />

<div
	class="split-container"
	bind:this={containerRef}
	class:dragging={isDragging}
	class:split-enabled={isSplitEnabled}
>
	<!-- Drag to Split Overlays -->
	{#if workspaceStore.isDragging}
		<div class="drag-overlays">
			<div
				class="drag-overlay left"
				class:active={dragOverSide === "left"}
				ondragover={(e) => handleGlobalDragOver(e, "left")}
				ondragleave={handleGlobalDragLeave}
				ondrop={(e) => handleGlobalDrop(e, "left")}
				role="presentation"
			></div>
			<div
				class="drag-overlay right"
				class:active={dragOverSide === "right"}
				ondragover={(e) => handleGlobalDragOver(e, "right")}
				ondragleave={handleGlobalDragLeave}
				ondrop={(e) => handleGlobalDrop(e, "right")}
				role="presentation"
			></div>
		</div>
	{/if}

	<!-- Left Pane (always visible) -->
	<div
		class="pane left-pane"
		class:active={!isSplitEnabled ||
			workspaceStore.split.activePaneId === "left"}
		style:width="{leftWidth}%"
		onclick={() => handlePaneClick("left")}
		onkeydown={(e) => e.key === "Enter" && handlePaneClick("left")}
		role="region"
		tabindex="0"
		aria-label="Left pane"
	>
		<WindowTabBar paneId={isSplitEnabled ? "left" : undefined} />
		<div class="pane-content">
			{#if leftPaneTabs.length === 0}
				<DiscoveryPage />
			{:else}
				{#each leftPaneTabs as tab (tab.id)}
					<TabContent {tab} active={tab.id === leftPaneActiveTabId} />
				{/each}
			{/if}
		</div>
	</div>

	<!-- Resize Handle (only interactive in split mode) -->
	<div
		class="resize-handle"
		class:dragging={isDragging}
		class:visible={isSplitEnabled}
		onmousedown={handleMouseDown}
		role="separator"
		aria-orientation="vertical"
		tabindex={isSplitEnabled ? 0 : -1}
	>
		<div class="handle-grip"></div>
	</div>

	<!-- Right Pane (always mounted, collapsed when not in split mode) -->
	<div
		class="pane right-pane"
		class:active={isSplitEnabled &&
			workspaceStore.split.activePaneId === "right"}
		class:collapsed={!isSplitEnabled}
		style:width="{rightWidth}%"
		onclick={() => handlePaneClick("right")}
		onkeydown={(e) => e.key === "Enter" && handlePaneClick("right")}
		role="region"
		tabindex={isSplitEnabled ? 0 : -1}
		aria-label="Right pane"
		aria-hidden={!isSplitEnabled}
	>
		{#if isSplitEnabled}
			<WindowTabBar paneId="right" />
		{/if}
		<div class="pane-content">
			{#if rightPaneTabs.length === 0}
				<DiscoveryPage />
			{:else}
				{#each rightPaneTabs as tab (tab.id)}
					<TabContent
						{tab}
						active={tab.id === rightPaneActiveTabId}
					/>
				{/each}
			{/if}
		</div>
	</div>
</div>

<style>
	.split-container {
		display: flex;
		flex: 1;
		min-height: 0;
		overflow: hidden;
		position: relative;
	}

	.split-container.dragging {
		cursor: col-resize;
		user-select: none;
	}

	.pane {
		display: flex;
		flex-direction: column;
		min-width: 0;
		overflow: hidden;
		border-radius: 0;
		transition:
			width 150ms ease,
			box-shadow 150ms ease,
			opacity 150ms ease;
	}

	.split-container.dragging .pane {
		transition: none;
	}

	/* Only show active indicator in split mode */
	.split-enabled .pane.active {
		box-shadow: inset 0 2px 0 0 var(--color-accent);
	}

	/* Right pane collapsed styles */
	.pane.collapsed {
		opacity: 0;
		pointer-events: none;
	}

	.pane-content {
		flex: 1;
		position: relative;
		min-height: 0;
		overflow: hidden;
	}

	.resize-handle {
		width: 0;
		cursor: col-resize;
		background: var(--color-border);
		display: flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
		transition:
			width 200ms ease,
			opacity 200ms ease,
			background-color 150ms ease;
		position: relative;
		opacity: 0;
		pointer-events: none;
	}

	/* Only show resize handle in split mode */
	.resize-handle.visible {
		width: 1px;
		opacity: 1;
		pointer-events: auto;
	}

	.resize-handle.visible::before {
		content: "";
		position: absolute;
		inset: 0;
		width: 11px;
		left: -5px;
		cursor: col-resize;
	}

	.resize-handle.visible:hover,
	.resize-handle.dragging {
		width: 5px;
		background: var(--color-border);
	}

	.handle-grip {
		width: 2px;
		height: 32px;
		background: var(--color-foreground-muted);
		border-radius: 1px;
		opacity: 0;
		transition: opacity 150ms ease;
	}

	.resize-handle.visible:hover .handle-grip,
	.resize-handle.dragging .handle-grip {
		opacity: 0.5;
	}

	/* Drag to Split Overlays */
	.drag-overlays {
		position: absolute;
		inset: 0;
		display: flex;
		z-index: 100;
		pointer-events: none;
	}

	.drag-overlay {
		flex: 1;
		display: flex;
		align-items: center;
		justify-content: center;
		pointer-events: auto;
		transition: background-color 75ms linear;
		background: transparent;
	}

	.drag-overlay.active {
		background: color-mix(in srgb, var(--color-primary) 15%, transparent);
	}
</style>
