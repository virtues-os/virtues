<script lang="ts">
	import { dndzone } from "svelte-dnd-action";
	import type { DndEvent } from "svelte-dnd-action";
	import { spaceStore } from "$lib/stores/space.svelte";
	import {
		dndManager,
		type DndTabItem,
		type ZoneId,
	} from "$lib/stores/dndManager.svelte";
	import WindowTabBar from "./WindowTabBar.svelte";
	import TabContent from "./TabContent.svelte";
	import ChatView from "./views/ChatView.svelte";

	let isResizing = $state(false);
	let containerRef = $state<HTMLElement | null>(null);

	// Drop items for left and right overlays
	let leftDropItems = $state<DndTabItem[]>([]);
	let rightDropItems = $state<DndTabItem[]>([]);

	// Zone IDs for split overlays
	const leftZoneId: ZoneId = { type: "split-overlay", paneId: "left" };
	const rightZoneId: ZoneId = { type: "split-overlay", paneId: "right" };

	const MIN_WIDTH = 25; // minimum 25% (1/4) for each pane
	const GUTTER_PX = 10; // gap between cards in split mode

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

	// All tabs across all panes - single source for the unified {#each} loop
	// Svelte 5 keyed {#each} preserves component identity when items stay in the array
	const allTabs = $derived(spaceStore.getAllTabs());

	// Compute widths
	const leftWidth = $derived(
		isSplitEnabled ? spaceStore.leftPane?.width || 50 : 100,
	);

	const rightWidth = $derived(
		isSplitEnabled ? spaceStore.rightPane?.width || 50 : 0,
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
		dndManager.handleConsider(
			e,
			leftZoneId,
			(items) => {
				leftDropItems = items;
			},
			leftDropItems,
		);
	}

	async function handleLeftDndFinalize(e: CustomEvent<DndEvent<DndTabItem>>) {
		await dndManager.handleFinalize(e, leftZoneId, (items) => {
			leftDropItems = items;
		});
		leftDropItems = [];
	}

	function handleRightDndConsider(e: CustomEvent<DndEvent<DndTabItem>>) {
		// Pass current items as originalItems (overlays don't initiate drags but receive DRAG_STARTED broadcasts)
		dndManager.handleConsider(
			e,
			rightZoneId,
			(items) => {
				rightDropItems = items;
			},
			rightDropItems,
		);
	}

	async function handleRightDndFinalize(
		e: CustomEvent<DndEvent<DndTabItem>>,
	) {
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
	style:--left-width="{leftWidth}%"
	style:--right-width="{rightWidth}%"
	style:--gutter-width="{isSplitEnabled ? GUTTER_PX : 0}px"
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
				dropAnimationDisabled: true,
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
				dropAnimationDisabled: true,
			}}
			onconsider={handleRightDndConsider}
			onfinalize={handleRightDndFinalize}
		></div>
	</div>

	<!-- Left Pane Shell: tab bar + background + click zone (no content rendered here) -->
	<!-- svelte-ignore a11y_no_noninteractive_tabindex a11y_no_noninteractive_element_interactions -->
	<div
		class="pane-shell left-pane"
		class:active={!isSplitEnabled || spaceStore.activePaneId === "left"}
		style:width={isSplitEnabled ? `calc(${leftWidth}% - ${GUTTER_PX / 2}px)` : '100%'}
		onclick={() => handlePaneClick("left")}
		onkeydown={(e) => e.key === "Enter" && handlePaneClick("left")}
		role="region"
		tabindex="0"
		aria-label="Left pane"
	>
		<WindowTabBar paneId={isSplitEnabled ? "left" : undefined} />
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

	<!-- Right Pane Shell: tab bar + background + click zone (no content rendered here) -->
	<!-- svelte-ignore a11y_no_noninteractive_tabindex a11y_no_noninteractive_element_interactions -->
	<div
		class="pane-shell right-pane"
		class:active={isSplitEnabled && spaceStore.activePaneId === "right"}
		class:collapsed={!isSplitEnabled}
		style:width={isSplitEnabled ? `calc(${rightWidth}% - ${GUTTER_PX / 2}px)` : '0'}
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
	</div>

	<!-- Tab Content Layer: single {#each} loop, absolutely positioned per-pane.
	     When a tab moves between panes, only CSS classes change — the component
	     instance is never destroyed. This preserves Yjs documents, WebSocket
	     connections, chat streaming, undo history, and scroll position. -->
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	{#each allTabs as tab (tab.id)}
		{@const paneId = spaceStore.findTabPane(tab.id) ?? "left"}
		{@const isActive =
			tab.id ===
			(paneId === "left" ? leftPaneActiveTabId : rightPaneActiveTabId)}
		<div
			class="tab-slot"
			class:in-left={paneId === "left" || !isSplitEnabled}
			class:in-right={paneId === "right" && isSplitEnabled}
			style:display={isActive ? "flex" : "none"}
			onpointerdown={() => isSplitEnabled && handlePaneClick(paneId)}
		>
			<TabContent {tab} active={isActive} />
		</div>
	{/each}

	<!-- Empty pane fallbacks: ephemeral ChatView when a pane has no tabs -->
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	{#if leftPaneTabs.length === 0}
		<div class="tab-slot in-left" style:display="flex" onpointerdown={() => isSplitEnabled && handlePaneClick("left")}>
			<ChatView
				tab={{
					id: "new-chat-left",
					type: "chat",
					label: "New Chat",
					route: "/chat",
					createdAt: Date.now(),
				}}
				active={true}
			/>
		</div>
	{/if}
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	{#if isSplitEnabled && rightPaneTabs.length === 0}
		<div class="tab-slot in-right" style:display="flex" onpointerdown={() => handlePaneClick("right")}>
			<ChatView
				tab={{
					id: "new-chat-right",
					type: "chat",
					label: "New Chat",
					route: "/chat",
					createdAt: Date.now(),
				}}
				active={true}
			/>
		</div>
	{/if}
</div>

<style>
	.split-container {
		--tab-bar-h: 41px; /* 6px padding + 28px tabs-scroll + 6px padding + 1px border */
		--card-radius: 4px;
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

	/* Pane shells: tab bar + background only, no content */
	.pane-shell {
		display: flex;
		flex-direction: column;
		min-width: 0;
		overflow: hidden;
		transition:
			width 150ms var(--ease-premium),
			border-color 150ms var(--ease-premium);
	}

	.split-container.dragging .pane-shell {
		transition: none;
	}

	/* Inset panes so borders aren't clipped by container overflow:hidden */
	.split-enabled {
		padding: 1px;
	}

	/* Card styling in split mode */
	.split-enabled .pane-shell {
		background: var(--color-surface);
		background-image: var(--background-image);
		background-blend-mode: multiply;
		border: 1px solid var(--color-border);
		border-radius: var(--card-radius);
		overflow: hidden;
	}

	/* Active pane: darker border */
	.split-enabled .pane-shell.active {
		border-color: color-mix(in srgb, var(--color-foreground) 20%, var(--color-border));
	}

	/* Right pane collapsed styles */
	.pane-shell.collapsed {
		opacity: 0;
		pointer-events: none;
	}

	/* Tab content slots: absolutely positioned into pane areas.
	   Each slot covers the area below the tab bar within its pane.
	   --tab-bar-h is the measured height: 6px padding + 28px scroll + 6px padding + 1px border */
	.tab-slot {
		position: absolute;
		top: var(--tab-bar-h, 41px);
		bottom: 0;
		flex-direction: column;
		overflow: hidden;
		transition: width 150ms var(--ease-premium);
	}

	/* In split mode, tab-bar-h is offset by container padding */
	.split-enabled .tab-slot {
		top: calc(var(--tab-bar-h, 41px) + 1px);
	}

	.split-container.dragging .tab-slot {
		transition: none;
	}

	.tab-slot.in-left {
		left: 0;
		width: calc(var(--left-width) - (var(--gutter-width, 0px) / 2));
	}

	.tab-slot.in-right {
		right: 0;
		width: calc(var(--right-width) - (var(--gutter-width, 0px) / 2));
	}

	/* Card content inset: tab-slot is a sibling of pane-shell (not a child),
	   so we must inset by the card border width + container padding to avoid
	   content overlapping the border. Container padding = 1px, border = 1px. */
	.split-enabled .tab-slot.in-left {
		left: 2px; /* 1px container padding + 1px border */
		width: calc(var(--left-width) - (var(--gutter-width, 0px) / 2) - 4px);
		bottom: 2px;
		border-radius: 0 0 var(--card-radius) var(--card-radius);
		overflow: hidden;
	}

	.split-enabled .tab-slot.in-right {
		right: 2px;
		width: calc(var(--right-width) - (var(--gutter-width, 0px) / 2) - 4px);
		bottom: 2px;
		border-radius: 0 0 var(--card-radius) var(--card-radius);
		overflow: hidden;
	}

	.resize-handle {
		width: 0;
		cursor: col-resize;
		background: transparent;
		display: flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
		transition:
			width 200ms var(--ease-premium),
			opacity 200ms var(--ease-premium),
			background-color 150ms var(--ease-premium);
		position: relative;
		opacity: 0;
		pointer-events: none;
	}

	/* Gutter: full gap width between cards */
	.resize-handle.visible {
		width: var(--gutter-width, 10px);
		opacity: 1;
		pointer-events: auto;
	}

	/* Hover: subtle fill hint in gutter */
	.resize-handle.visible:hover,
	.resize-handle.dragging {
		background: color-mix(in srgb, var(--color-foreground) 5%, transparent);
	}

	.handle-grip {
		width: 3px;
		height: 32px;
		background: var(--color-foreground-muted);
		border-radius: 2px;
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

	/* Card-aware drag overlays in split mode */
	.split-enabled .drag-overlay.left {
		border-radius: var(--card-radius);
		margin-right: calc(var(--gutter-width, 0px) / 2);
	}

	.split-enabled .drag-overlay.right {
		border-radius: var(--card-radius);
		margin-left: calc(var(--gutter-width, 0px) / 2);
	}

	/* Hide the ghost/shadow item that svelte-dnd-action creates in the drop zone */
	.drag-overlay :global([data-is-dnd-shadow-item-hint="true"]) {
		display: none !important;
	}
</style>
