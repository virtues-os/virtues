<script lang="ts">
	import { dndzone } from "svelte-dnd-action";
	import type { DndEvent } from "svelte-dnd-action";
	import { windowShellStore, SPLIT_GUTTER_PX } from "$lib/stores/window-shell.svelte";
	import {
		dndManager,
		type DndTabItem,
		type ZoneId,
	} from "$lib/stores/dndManager.svelte";
	import { gettingStarted } from "$lib/stores/gettingStarted.svelte";
	import TabContent from "./TabContent.svelte";
	import { mountedTabs, touch } from "$lib/tabs/keepAlive";
	import { onMount, untrack } from "svelte";
	import { listenForBrowserOpen } from "./browserPaneListener";
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
	// In split each pane is its own card, with the desk between them. The
	// gutter is the resize handle's hit area.
	const GUTTER_PX = SPLIT_GUTTER_PX;

	// Derived state for split mode
	const isSplitEnabled = $derived(windowShellStore.isSplit);

	// Track isDragging reactively via direct session access
	// Public session property enables Svelte 5 fine-grained reactivity
	const isDragging = $derived(dndManager.session !== null);

	// Direct pane access (unified model - tabs always live in panes)
	const leftPaneTabs = $derived(windowShellStore.panes[0]?.tabs ?? []);
	const leftPaneActiveTabId = $derived(
		windowShellStore.panes[0]?.activeTabId ?? null,
	);

	// Right pane only used in split mode
	const rightPaneActiveTabId = $derived(
		windowShellStore.panes[1]?.activeTabId ?? null,
	);

	// All tabs across all panes - single source for the unified {#each} loop
	// Svelte 5 keyed {#each} preserves component identity when items stay in the array
	const allTabs = $derived(windowShellStore.getAllTabs());

	// Which hidden tabs keep their view mounted ($lib/tabs/keepAlive). The
	// visible ones always do; the recency list decides the rest.
	const visibleTabIds = $derived(
		[leftPaneActiveTabId, isSplitEnabled ? rightPaneActiveTabId : null].filter(
			(id): id is string => !!id,
		),
	);
	let recentTabIds = $state<string[]>([]);
	$effect(() => {
		const visible = visibleTabIds;
		untrack(() => (recentTabIds = touch(recentTabIds, visible)));
	});
	const mountedTabIds = $derived(mountedTabs(allTabs, visibleTabIds, recentTabIds));

	// Compute widths
	const leftWidth = $derived(
		isSplitEnabled ? windowShellStore.leftPane?.width || 50 : 100,
	);

	const rightWidth = $derived(
		isSplitEnabled ? windowShellStore.rightPane?.width || 50 : 0,
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

		windowShellStore.setPaneWidth(clampedWidth);
	}

	function handleMouseUp() {
		isResizing = false;
	}

	function handlePaneClick(paneId: "left" | "right") {
		if (!isSplitEnabled) return;
		windowShellStore.setActivePane(paneId);
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

	// The assistant's browser_open, and "Log in to X", open the Browser beside
	// the current view (Mac).
	onMount(() => {
		let off: (() => void) | undefined;
		void listenForBrowserOpen().then((f) => (off = f));
		return () => off?.();
	});
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

	<!-- Left Pane Shell: background + click zone. Its tabs are in the window
	     header (WindowHeader), and no content is rendered here. -->
	<!-- svelte-ignore a11y_no_noninteractive_tabindex a11y_no_noninteractive_element_interactions -->
	<div
		class="pane-shell left-pane"
		class:active={!isSplitEnabled || windowShellStore.activePaneId === "left"}
		style:width={isSplitEnabled ? `calc(${leftWidth}% - ${GUTTER_PX / 2}px)` : '100%'}
		onclick={() => handlePaneClick("left")}
		onkeydown={(e) => e.key === "Enter" && handlePaneClick("left")}
		role="region"
		tabindex="0"
		aria-label="Left pane"
	>
		
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
	><span class="resize-grip" aria-hidden="true"></span></div>

	<!-- Right Pane Shell: background + click zone, as the left. -->
	<!-- svelte-ignore a11y_no_noninteractive_tabindex a11y_no_noninteractive_element_interactions -->
	<div
		class="pane-shell right-pane"
		class:active={isSplitEnabled && windowShellStore.activePaneId === "right"}
		class:collapsed={!isSplitEnabled}
		style:width={isSplitEnabled ? `calc(${rightWidth}% - ${GUTTER_PX / 2}px)` : '0'}
		onclick={() => handlePaneClick("right")}
		onkeydown={(e) => e.key === "Enter" && handlePaneClick("right")}
		role="region"
		tabindex={isSplitEnabled ? 0 : -1}
		aria-label="Right pane"
		aria-hidden={!isSplitEnabled}
	>
	</div>

	<!-- Typing anywhere in a preview tab (a composer, a page, a field) keeps
	     it: you have started working in it. -->
	<!-- Tab Content Layer: single {#each} loop, absolutely positioned per-pane.
	     When a tab moves between panes, only CSS classes change, so a mounted
	     view survives the move. A hidden tab outside the keep-alive set
	     ($lib/tabs/keepAlive) keeps its slot but not its view, and remounts
	     from its route when it is opened again. -->
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	{#each allTabs as tab (tab.id)}
		{@const paneId = windowShellStore.findTabPane(tab.id) ?? "left"}
		{@const isActive =
			tab.id ===
			(paneId === "left" ? leftPaneActiveTabId : rightPaneActiveTabId)}
		<div
			class="tab-slot"
			class:in-left={paneId === "left" || !isSplitEnabled}
			class:in-right={paneId === "right" && isSplitEnabled}
			style:display={isActive ? "flex" : "none"}
			onpointerdown={() => isSplitEnabled && handlePaneClick(paneId)}
			onbeforeinput={() => tab.preview && windowShellStore.keepTab(tab.id)}
		>
			{#if mountedTabIds.has(tab.id)}
				<TabContent {tab} active={isActive} />
			{/if}
		</div>
	{/each}

	<!-- An empty window shows a new chat. A split pane is never empty: one is
	     only made by putting a tab in it (window-shell `moveTabAcross`). -->
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	{#if leftPaneTabs.length === 0}
		<div class="tab-slot in-left" style:display="flex" onpointerdown={() => isSplitEnabled && handlePaneClick("left")}>
			<ChatView
				tab={{
					id: "new-chat-left",
					type: "chat",
					label: "New Chat",
					route: "/chat",
					history: ["/chat"],
					historyIndex: 0,
					createdAt: Date.now(),
				}}
				active={true}
			/>
		</div>
	{/if}
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

	/* One pane is `main`'s card, so the shell paints nothing. In split each
	   pane is a card of its own on the desk (`main` turns transparent in the
	   app layout): the page's surface, the card's border and corners. The
	   focused pane is told by its active tab's ink, not by its card. */
	.split-enabled .pane-shell {
		background-color: var(--color-page);
		background-image: var(--background-image);
		background-blend-mode: multiply;
		border: 1px solid var(--color-border);
		border-radius: var(--card-radius);
	}

	/* Right pane collapsed styles */
	.pane-shell.collapsed {
		opacity: 0;
		pointer-events: none;
	}

	/* Tab content slots: absolutely positioned into pane areas. Each slot
	   covers its whole pane; the tabs are in the window header above. */
	.tab-slot {
		position: absolute;
		top: 0;
		bottom: 0;
		flex-direction: column;
		overflow: hidden;
		transition: width 150ms var(--ease-premium);
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

	/* In split, inside each card's border, and clipped to its corners. */
	.split-enabled .tab-slot {
		top: 1px;
		bottom: 1px;
		border-radius: calc(var(--card-radius) - 1px);
	}

	.split-enabled .tab-slot.in-left {
		left: 1px;
		width: calc(var(--left-width) - (var(--gutter-width, 0px) / 2) - 2px);
	}

	.split-enabled .tab-slot.in-right {
		right: 1px;
		width: calc(var(--right-width) - (var(--gutter-width, 0px) / 2) - 2px);
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

	/* The gutter itself is the handle: desk, no line. */
	/* z-index lifts the grip above the panes' own stacked content (the chat
	   composer), which otherwise paints over the pill's overhang. */
	.resize-handle.visible {
		z-index: 5;
		width: var(--gutter-width, 1px);
		background: transparent;
		opacity: 1;
		pointer-events: auto;
	}

	/* The grab target reaches 4px past the gutter into each card's edge, so
	   the seam is easy to find from either side. */
	.resize-handle.visible::after {
		content: "";
		position: absolute;
		top: 0;
		bottom: 0;
		left: -4px;
		right: -4px;
	}

	/* The grip: a pill centred on the 1px line, hidden until the pointer
	   finds the seam. The line itself never changes; the grip is the one
	   thing that responds. It fades in and lengthens on approach and takes
	   the accent only while held. Same object as the sidebar seam's grip. */
	.resize-grip {
		display: none;
	}

	.resize-handle.visible .resize-grip {
		display: block;
		position: absolute;
		top: 50%;
		left: 50%;
		width: 4px;
		height: 32px;
		transform: translate(-50%, -50%);
		border-radius: 999px;
		background: var(--color-foreground-subtle);
		opacity: 0;
		pointer-events: none;
		z-index: 1;
		transition:
			opacity 160ms var(--ease-premium),
			height 160ms var(--ease-premium),
			background-color 160ms var(--ease-premium);
	}

	.resize-handle.visible:hover .resize-grip,
	.resize-handle.visible:focus-visible .resize-grip {
		opacity: 1;
		height: 40px;
	}

	/* Held: the accent says you have it. No easing — it follows the pointer. */
	.resize-handle.dragging .resize-grip {
		background: var(--color-primary);
		opacity: 1;
		height: 40px;
		transition: none;
	}

	@media (prefers-reduced-motion: reduce) {
		.resize-handle.visible .resize-grip {
			transition: none;
		}
	}

	/* Drag to Split Overlays - Always in DOM but hidden until drag starts */
	.drag-overlays {
		position: absolute;
		inset: 0;
		display: flex;
		z-index: var(--z-overlay);
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
