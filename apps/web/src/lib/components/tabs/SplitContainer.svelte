<script lang="ts">
	import { dndzone } from "svelte-dnd-action";
	import type { DndEvent } from "svelte-dnd-action";
	import { spaceStore } from "$lib/stores/space.svelte";
	import { dndManager, type DndTabItem, type ZoneId } from "$lib/stores/dndManager.svelte";
	import WindowTabBar from "./WindowTabBar.svelte";
	import TabContent from "./TabContent.svelte";
	import ChatView from "./views/ChatView.svelte";

	let isResizing = $state(false);
	let containerRef = $state<HTMLElement | null>(null);

	// Drop items for left and right overlays
	let leftDropItems = $state<DndTabItem[]>([]);
	let rightDropItems = $state<DndTabItem[]>([]);

	// Zone IDs for split overlays
	const leftZoneId: ZoneId = { type: 'split-overlay', paneId: 'left' };
	const rightZoneId: ZoneId = { type: 'split-overlay', paneId: 'right' };

	const MIN_WIDTH = 25; // minimum 25% (1/4) for each pane

	// Derived state for split mode
	const isSplitEnabled = $derived(spaceStore.isSplit);

	// Track isDragging reactively via direct session access
	// Public session property enables Svelte 5 fine-grained reactivity
	const isDragging = $derived(dndManager.session !== null);

	// Direct pane access (unified model - tabs always live in panes)
	const leftPaneTabs = $derived(spaceStore.panes[0]?.tabs ?? []);
	const leftPaneActiveTabId = $derived(
		spaceStore.panes[0]?.activeTabId ?? null,
	);

	// Right pane only used in split mode
	const rightPaneTabs = $derived(spaceStore.panes[1]?.tabs ?? []);
	const rightPaneActiveTabId = $derived(
		spaceStore.panes[1]?.activeTabId ?? null,
	);

	// Compute widths
	const leftWidth = $derived(
		isSplitEnabled ? spaceStore.leftPane?.width || 50 : 100,
	);

	const rightWidth = $derived(
		isSplitEnabled ? spaceStore.rightPane?.width || 50 : 0,
	);

	// Content key for triggering page transition animation
	// Use tab ID (not route) so updating a tab's route/entityId doesn't trigger animation
	// Animation fires when: switching workspaces, switching tabs, or opening new tabs
	const leftActiveTab = $derived(
		leftPaneTabs.find((t) => t.id === leftPaneActiveTabId),
	);
	const rightActiveTab = $derived(
		rightPaneTabs.find((t) => t.id === rightPaneActiveTabId),
	);
	const leftContentKey = $derived(
		`${spaceStore.activeSpaceId}-${leftPaneActiveTabId ?? "empty"}`,
	);
	const rightContentKey = $derived(
		`${spaceStore.activeSpaceId}-${rightPaneActiveTabId ?? "empty"}`,
	);

	function handleMouseDown(e: MouseEvent) {
		if (!isSplitEnabled) return;
		e.preventDefault();
		isResizing = true;
	}

	function handleMouseMove(e: MouseEvent) {
		if (!isResizing || !containerRef || !isSplitEnabled) return;

		const rect = containerRef.getBoundingClientRect();
		const newWidth = ((e.clientX - rect.left) / rect.width) * 100;
		const clampedWidth = Math.max(
			MIN_WIDTH,
			Math.min(100 - MIN_WIDTH, newWidth),
		);

		spaceStore.setPaneWidth(clampedWidth);
	}

	function handleMouseUp() {
		isResizing = false;
	}

	function handlePaneClick(paneId: "left" | "right") {
		if (!isSplitEnabled) return;
		spaceStore.setActivePane(paneId);
	}

	// svelte-dnd-action handlers delegated to centralized dndManager
	function handleLeftDndConsider(e: CustomEvent<DndEvent<DndTabItem>>) {
		// Pass current items as originalItems (overlays don't initiate drags but receive DRAG_STARTED broadcasts)
		dndManager.handleConsider(e, leftZoneId, (items) => {
			leftDropItems = items;
		}, leftDropItems);
	}

	async function handleLeftDndFinalize(e: CustomEvent<DndEvent<DndTabItem>>) {
		await dndManager.handleFinalize(e, leftZoneId, (items) => {
			leftDropItems = items;
		});
		leftDropItems = [];
	}

	function handleRightDndConsider(e: CustomEvent<DndEvent<DndTabItem>>) {
		// Pass current items as originalItems (overlays don't initiate drags but receive DRAG_STARTED broadcasts)
		dndManager.handleConsider(e, rightZoneId, (items) => {
			rightDropItems = items;
		}, rightDropItems);
	}

	async function handleRightDndFinalize(e: CustomEvent<DndEvent<DndTabItem>>) {
		await dndManager.handleFinalize(e, rightZoneId, (items) => {
			rightDropItems = items;
		});
		rightDropItems = [];
	}
</script>

<svelte:window on:mousemove={handleMouseMove} on:mouseup={handleMouseUp} />

<div
	class="split-container"
	bind:this={containerRef}
	class:dragging={isResizing}
	class:split-enabled={isSplitEnabled}
>
	<!-- Drag to Split Overlays - Always mounted so svelte-dnd-action registers zones before drag starts -->
	<div class="drag-overlays" class:visible={isDragging}>
		<div
			class="drag-overlay left"
			class:active={leftDropItems.length > 0}
			role="presentation"
			use:dndzone={{
				items: leftDropItems,
				type: "tab",
				flipDurationMs: 0,
				dragDisabled: true,
				dropFromOthersDisabled: !isDragging,
				dropAnimationDisabled: true
			}}
			onconsider={handleLeftDndConsider}
			onfinalize={handleLeftDndFinalize}
		></div>
		<div
			class="drag-overlay right"
			class:active={rightDropItems.length > 0}
			role="presentation"
			use:dndzone={{
				items: rightDropItems,
				type: "tab",
				flipDurationMs: 0,
				dragDisabled: true,
				dropFromOthersDisabled: !isDragging,
				dropAnimationDisabled: true
			}}
			onconsider={handleRightDndConsider}
			onfinalize={handleRightDndFinalize}
		></div>
	</div>

	<!-- Left Pane (always visible) -->
	<!-- svelte-ignore a11y_no_noninteractive_tabindex a11y_no_noninteractive_element_interactions -->
	<div
		class="pane left-pane"
		class:active={!isSplitEnabled || spaceStore.activePaneId === "left"}
		style:width="{leftWidth}%"
		onclick={() => handlePaneClick("left")}
		onkeydown={(e) => e.key === "Enter" && handlePaneClick("left")}
		role="region"
		tabindex="0"
		aria-label="Left pane"
	>
		<WindowTabBar paneId={isSplitEnabled ? "left" : undefined} />
		{#key leftContentKey}
			<div class="pane-content page-transition">
				{#if leftPaneTabs.length === 0}
					<ChatView tab={{ id: 'new-chat-left', type: 'chat', label: 'New Chat', route: '/chat', createdAt: Date.now() }} active={true} />
				{:else}
					{#each leftPaneTabs as tab (tab.id)}
						<TabContent
							{tab}
							active={tab.id === leftPaneActiveTabId}
						/>
					{/each}
				{/if}
			</div>
		{/key}
	</div>

	<!-- Resize Handle (only interactive in split mode) -->
	<!-- svelte-ignore a11y_no_noninteractive_tabindex a11y_no_noninteractive_element_interactions -->
	<div
		class="resize-handle"
		class:dragging={isResizing}
		class:visible={isSplitEnabled}
		onmousedown={handleMouseDown}
		role="separator"
		aria-orientation="vertical"
		tabindex={isSplitEnabled ? 0 : -1}
	>
		<div class="handle-grip"></div>
	</div>

	<!-- Right Pane (always mounted, collapsed when not in split mode) -->
	<!-- svelte-ignore a11y_no_noninteractive_tabindex a11y_no_noninteractive_element_interactions -->
	<div
		class="pane right-pane"
		class:active={isSplitEnabled && spaceStore.activePaneId === "right"}
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
		{#key rightContentKey}
			<div class="pane-content page-transition">
				{#if rightPaneTabs.length === 0}
					<ChatView tab={{ id: 'new-chat-right', type: 'chat', label: 'New Chat', route: '/chat', createdAt: Date.now() }} active={true} />
				{:else}
					{#each rightPaneTabs as tab (tab.id)}
						<TabContent
							{tab}
							active={tab.id === rightPaneActiveTabId}
						/>
					{/each}
				{/if}
			</div>
		{/key}
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
		background: var(--color-background);
		transition: width 150ms ease;
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

	.page-transition {
		animation: pageFadeIn 200ms ease both;
	}

	@keyframes pageFadeIn {
		from {
			opacity: 0;
		}
		to {
			opacity: 1;
		}
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

	/* Drag to Split Overlays - Always in DOM but hidden until drag starts */
	.drag-overlays {
		position: absolute;
		inset: 0;
		display: flex;
		z-index: 100;
		pointer-events: none;
		opacity: 0;
		visibility: hidden;
	}

	.drag-overlays.visible {
		opacity: 1;
		visibility: visible;
	}

	.drag-overlay {
		flex: 1;
		display: flex;
		align-items: center;
		justify-content: center;
		pointer-events: none; /* Default: pass-through when hidden */
		transition: background-color 75ms linear;
		background: transparent;
	}

	/* Only receive pointer events when parent is visible */
	.drag-overlays.visible .drag-overlay {
		pointer-events: auto;
	}

	.drag-overlay.active {
		background: color-mix(in srgb, var(--color-primary) 15%, transparent);
	}

	/* Hide the ghost/shadow item that svelte-dnd-action creates in the drop zone */
	.drag-overlay :global([data-is-dnd-shadow-item-hint="true"]) {
		display: none !important;
	}
</style>
