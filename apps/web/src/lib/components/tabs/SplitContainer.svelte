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
	// The slots are rendered in the order the tabs were made, never in pane
	// order: merging the panes reorders `allTabs`, and a keyed {#each} moves
	// a reordered slot's node, which drops the transition carrying it to its
	// new place.
	const slotTabs = $derived(
		[...allTabs].sort((a, b) => a.createdAt - b.createdAt || a.id.localeCompare(b.id)),
	);

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

	// ---------------------------------------------------------------------
	// Opening and closing a side
	//
	// Both panes always draw a card. With one pane the right card is zero
	// wide, so splitting is the right card growing out of the window's edge
	// as the gutter opens, and closing is the reverse: the gutter shuts and
	// one card is left. Everything moves on --motion-layout, the header's
	// strips included.
	//
	// A view that appears with its pane is revealed with its card's edge; a
	// view that goes with its pane is clipped away with it. When the side
	// that stays is the right one, the right card grows over the whole window
	// and the panes swap back, unseen, once it has.
	// ---------------------------------------------------------------------

	type Side = "left" | "right";
	/** The two cards' widths (%) and the desk gap between them (px). */
	type Frame = { left: number; right: number; gutter: number };
	type Leaving = { id: string; side: Side; frame: Frame; clip: boolean };
	type Entering = { side: Side; clip: boolean };
	type Motion = {
		leaving: Leaving[];
		entering: Map<string, Entering>;
		/** The tab that keeps the window when the right side survives. */
		survivor: string | null;
	};

	const WHOLE: Frame = { left: 100, right: 0, gutter: 0 };
	const RIGHT_WHOLE: Frame = { left: 0, right: 100, gutter: 0 };

	let motion = $state<Motion | null>(null);
	/** One frame with transitions off, for the unseen swap back. */
	let settling = $state(false);
	let motionTimer: ReturnType<typeof setTimeout> | undefined;

	const storeFrame = $derived<Frame>(
		isSplitEnabled
			? { left: leftWidth, right: rightWidth, gutter: GUTTER_PX }
			: WHOLE,
	);
	const frame = $derived<Frame>(motion?.survivor ? RIGHT_WHOLE : storeFrame);

	function sideOf(tabId: string): Side {
		if (motion?.survivor === tabId) return "right";
		return isSplitEnabled && windowShellStore.findTabPane(tabId) === "right"
			? "right"
			: "left";
	}

	/** A view's box inside its side's card, 1px in from the card's border. */
	function slotBox(side: Side, f: Frame): { left: string; width: string } {
		const half = f.gutter / 2;
		return side === "left"
			? { left: "1px", width: `calc(${f.left}% - ${half + 2}px)` }
			: {
					left: `calc(${f.left}% + ${half + 1}px)`,
					width: `calc(${f.right}% - ${half + 2}px)`,
				};
	}

	type Arrangement = {
		split: boolean;
		frame: Frame;
		visible: Map<string, Side>;
		panes: Map<string, string | null>;
	};
	// Plain: only read to compare the arrangement before a change with after.
	let previous: Arrangement | null = null;

	function layoutMs(): number {
		if (!containerRef) return 300;
		const raw = getComputedStyle(containerRef).getPropertyValue("--motion-layout");
		return parseFloat(raw) || 300;
	}

	function startMotion(before: Arrangement, after: Arrangement) {
		const splitChanged = before.split !== after.split;
		const moved = [...after.panes].some(
			([id, pane]) => before.panes.has(id) && before.panes.get(id) !== pane,
		);
		if (!splitChanged && !moved) return;

		let survivor: string | null = null;
		if (before.split && !after.split) {
			const [kept] = after.visible.keys();
			if (kept && before.visible.get(kept) === "right") survivor = kept;
		}
		const to = survivor ? RIGHT_WHOLE : after.frame;
		const from = before.frame;
		const grew = (side: Side) => to[side] > from[side];
		const shrank = (side: Side) => to[side] < from[side];

		const leaving: Leaving[] = [];
		for (const [id, side] of before.visible) {
			if (after.visible.has(id) || !after.panes.has(id)) continue;
			leaving.push({ id, side, frame: from, clip: shrank(side) });
		}
		const entering = new Map<string, Entering>();
		for (const [id, side] of after.visible) {
			if (before.visible.has(id)) continue;
			entering.set(id, { side, clip: grew(side) });
		}

		clearTimeout(motionTimer);
		settling = false;
		motion = { leaving, entering, survivor };
		motionTimer = setTimeout(endMotion, layoutMs() + 40);
	}

	function endMotion() {
		if (!motion) return;
		const swap = motion.survivor !== null;
		motion = null;
		if (!swap) return;
		settling = true;
		requestAnimationFrame(() =>
			requestAnimationFrame(() => (settling = false)),
		);
	}

	$effect.pre(() => {
		const visible = new Map<string, Side>();
		if (leftPaneActiveTabId) visible.set(leftPaneActiveTabId, "left");
		if (isSplitEnabled && rightPaneActiveTabId)
			visible.set(rightPaneActiveTabId, "right");
		const next: Arrangement = {
			split: isSplitEnabled,
			frame: storeFrame,
			visible,
			panes: new Map(allTabs.map((t) => [t.id, windowShellStore.findTabPane(t.id)])),
		};
		untrack(() => {
			if (previous && !isResizing) startMotion(previous, next);
			previous = next;
		});
	});

	onMount(() => () => clearTimeout(motionTimer));

	function handleMouseDown(e: MouseEvent) {
		if (!isSplitEnabled) return;
		e.preventDefault();
		isResizing = true;
		windowShellStore.paneResizing = true;
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
		if (!isResizing) return;
		isResizing = false;
		windowShellStore.paneResizing = false;
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
	class:in-motion={motion !== null}
	class:settling
	style:--left-width="{frame.left}%"
	style:--right-width="{frame.right}%"
	style:--gutter-width="{frame.gutter}px"
	style:--split-gap="{GUTTER_PX}px"
>
	<!-- Drag to Split Overlays - Always mounted so svelte-dnd-action registers zones before drag starts.
	     With one pane only the right half takes a drop, and only when there is
	     a tab left behind. The zone under the pointer takes a soft tint the
	     shape of the card the drop lands in. The zones render no children:
	     svelte-dnd-action treats a zone's children as its items. -->
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
				dropFromOthersDisabled: !isDragging || !isSplitEnabled,
				dropAnimationDisabled: true,
				dropTargetStyle: {},
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
				dropFromOthersDisabled: !isDragging || (!isSplitEnabled && leftPaneTabs.length < 2),
				dropAnimationDisabled: true,
				dropTargetStyle: {},
			}}
			onconsider={handleRightDndConsider}
			onfinalize={handleRightDndFinalize}
		></div>
	</div>

	<!-- Left Pane Shell: the card and a click zone. Its tabs are in the window
	     header (WindowHeader), and no content is rendered here. -->
	<!-- svelte-ignore a11y_no_noninteractive_tabindex a11y_no_noninteractive_element_interactions -->
	<div
		class="pane-shell left-pane"
		class:active={!isSplitEnabled || windowShellStore.activePaneId === "left"}
		class:collapsed={frame.left === 0}
		style:width="calc({frame.left}% - {frame.gutter / 2}px)"
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

	<!-- Right Pane Shell: the card and a click zone, as the left. Zero wide
	     with one pane. -->
	<!-- svelte-ignore a11y_no_noninteractive_tabindex a11y_no_noninteractive_element_interactions -->
	<div
		class="pane-shell right-pane"
		class:active={isSplitEnabled && windowShellStore.activePaneId === "right"}
		class:collapsed={frame.right === 0}
		style:width="calc({frame.right}% - {frame.gutter / 2}px)"
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
	     When a tab moves between panes, only its box changes, so a mounted
	     view survives the move and slides to its new place. A hidden tab
	     outside the keep-alive set ($lib/tabs/keepAlive) keeps its slot but
	     not its view, and remounts from its route when it is opened again. -->
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	{#each slotTabs as tab (tab.id)}
		{@const paneId = windowShellStore.findTabPane(tab.id) ?? "left"}
		{@const isActive =
			tab.id ===
			(paneId === "left" ? leftPaneActiveTabId : rightPaneActiveTabId)}
		{@const leaving = isActive ? undefined : motion?.leaving.find((l) => l.id === tab.id)}
		{@const entering = isActive ? motion?.entering.get(tab.id) : undefined}
		{@const side = leaving ? leaving.side : sideOf(tab.id)}
		{@const box = slotBox(side, leaving ? leaving.frame : frame)}
		<div
			class="tab-slot"
			data-side={side}
			data-motion={leaving ? "leave" : entering ? "enter" : undefined}
			data-clip={(leaving?.clip ?? entering?.clip) ? "" : undefined}
			style:display={isActive || leaving ? "flex" : "none"}
			style:left={box.left}
			style:width={box.width}
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
		{@const box = slotBox("left", frame)}
		<div
			class="tab-slot"
			data-side="left"
			style:display="flex"
			style:left={box.left}
			style:width={box.width}
			onpointerdown={() => isSplitEnabled && handlePaneClick("left")}
		>
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

	/* Pane shells: each is a card on the desk (`main` is only the desk in
	   the app layout): the page's surface, the card's border and corners.
	   The focused pane is told by its active tab's ink, not by its card.
	   With one pane the right card is zero wide and faded out, so opening
	   a side is that card growing in from the edge as the gutter opens. */
	.pane-shell {
		display: flex;
		flex-direction: column;
		flex: none;
		min-width: 0;
		overflow: hidden;
		background-color: var(--color-page);
		background-image: var(--background-image);
		background-blend-mode: multiply;
		border: 1px solid var(--card-edge);
		border-radius: var(--card-radius);
		transition: width var(--motion-layout) var(--ease-premium);
	}

	/* A card that has closed leaves its border behind as a 2px sliver, so
	   it hides, but only once it has finished closing: fading it on the way
	   would show the desk through the card that is taking its place. */
	.pane-shell.collapsed {
		opacity: 0;
		pointer-events: none;
		transition:
			width var(--motion-layout) var(--ease-premium),
			opacity 0s linear var(--motion-layout);
	}

	/* Tab content slots: absolutely positioned inside their side's card.
	   Their box is set inline (`slotBox`) and eases with the cards, so a
	   view that changes sides slides there. */
	.tab-slot {
		position: absolute;
		top: 1px;
		bottom: 1px;
		flex-direction: column;
		overflow: hidden;
		border-radius: calc(var(--card-radius) - 1px);
		transition:
			left var(--motion-layout) var(--ease-premium),
			width var(--motion-layout) var(--ease-premium);
	}

	.split-container.dragging .pane-shell,
	.split-container.dragging .tab-slot,
	.split-container.dragging .resize-handle,
	.split-container.settling .pane-shell,
	.split-container.settling .tab-slot {
		transition: none;
	}

	/* While a side opens or closes, the views that stay are opaque and sit
	   over the ones going, which fade and clip away beneath them. */
	.in-motion .tab-slot:not([data-motion="leave"]) {
		z-index: 1;
		background-color: var(--color-page);
		background-image: var(--background-image);
		background-blend-mode: multiply;
	}

	.tab-slot[data-motion="leave"] {
		pointer-events: none;
		animation: slot-fade-out var(--motion-state) var(--ease-premium) forwards;
	}

	/* Going with its card: clipped at the card's moving edge. */
	.tab-slot[data-motion="leave"][data-clip][data-side="right"] {
		animation:
			slot-close-right var(--motion-layout) var(--ease-premium) forwards,
			slot-fade-out var(--motion-layout) var(--ease-premium) forwards;
	}

	.tab-slot[data-motion="leave"][data-clip][data-side="left"] {
		animation:
			slot-close-left var(--motion-layout) var(--ease-premium) forwards,
			slot-fade-out var(--motion-layout) var(--ease-premium) forwards;
	}

	/* Arriving in a pane that is already there: a short fade and drift in
	   from its own side. */
	.tab-slot[data-motion="enter"][data-side="right"] {
		animation: slot-drift-in-right var(--motion-enter) var(--ease-premium) both;
	}

	.tab-slot[data-motion="enter"][data-side="left"] {
		animation: slot-drift-in-left var(--motion-enter) var(--ease-premium) both;
	}

	/* Arriving with its card: revealed at the card's moving edge, the view
	   itself settling in just behind it. */
	.tab-slot[data-motion="enter"][data-clip][data-side="right"] {
		animation:
			slot-open-right var(--motion-layout) var(--ease-premium) both,
			slot-drift-in-right var(--motion-layout) var(--ease-premium) 60ms both;
	}

	.tab-slot[data-motion="enter"][data-clip][data-side="left"] {
		animation:
			slot-open-left var(--motion-layout) var(--ease-premium) both,
			slot-drift-in-left var(--motion-layout) var(--ease-premium) 60ms both;
	}

	@keyframes slot-open-right {
		from { clip-path: inset(0 0 0 100% round var(--card-radius)); }
		to { clip-path: inset(0 0 0 0 round var(--card-radius)); }
	}

	@keyframes slot-open-left {
		from { clip-path: inset(0 100% 0 0 round var(--card-radius)); }
		to { clip-path: inset(0 0 0 0 round var(--card-radius)); }
	}

	@keyframes slot-close-right {
		from { clip-path: inset(0 0 0 0 round var(--card-radius)); }
		to { clip-path: inset(0 0 0 100% round var(--card-radius)); }
	}

	@keyframes slot-close-left {
		from { clip-path: inset(0 0 0 0 round var(--card-radius)); }
		to { clip-path: inset(0 100% 0 0 round var(--card-radius)); }
	}

	@keyframes slot-drift-in-right {
		from { opacity: 0; translate: 12px 0; }
	}

	@keyframes slot-drift-in-left {
		from { opacity: 0; translate: -12px 0; }
	}

	@keyframes slot-fade-in {
		from { opacity: 0; }
	}

	@keyframes slot-fade-out {
		to { opacity: 0; }
	}

	.resize-handle {
		width: var(--gutter-width, 0px);
		cursor: col-resize;
		background: transparent;
		display: flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
		transition:
			width var(--motion-layout) var(--ease-premium),
			opacity var(--motion-layout) var(--ease-premium);
		position: relative;
		opacity: 0;
		pointer-events: none;
	}

	/* The gutter itself is the handle: desk, no line. */
	/* z-index lifts the grip above the panes' own stacked content (the chat
	   composer), which otherwise paints over the pill's overhang. */
	.resize-handle.visible {
		z-index: 5;
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
		position: relative;
		flex: 1;
		display: flex;
		align-items: center;
		justify-content: center;
		pointer-events: none; /* Default: pass-through when hidden */
		border-radius: var(--card-radius);
		transition: background-color var(--motion-state) var(--ease-premium);
		background: transparent;
	}

	/* Only receive pointer events when parent is visible */
	.drag-overlays.visible .drag-overlay {
		pointer-events: auto;
	}

	/* In split each half lies over its own card. */
	.split-enabled .drag-overlay.left {
		flex: none;
		width: calc(var(--left-width) - (var(--gutter-width) / 2));
		margin-right: var(--gutter-width);
	}

	.split-enabled .drag-overlay.right {
		flex: none;
		width: calc(var(--right-width) - (var(--gutter-width) / 2));
	}

	/* With one pane the halves sit where the two cards would, a desk gap
	   apart, so the tint is the shape of the card a drop opens. */
	.split-container:not(.split-enabled) .drag-overlay.left {
		margin-right: calc(var(--split-gap) / 2);
	}

	.split-container:not(.split-enabled) .drag-overlay.right {
		margin-left: calc(var(--split-gap) / 2);
	}

	.drag-overlay.active {
		background: color-mix(in srgb, var(--color-primary) 15%, transparent);
	}

	/* Hide the ghost/shadow item that svelte-dnd-action creates in the drop zone */
	.drag-overlay :global([data-is-dnd-shadow-item-hint="true"]) {
		display: none !important;
	}

	/* Without motion the cards and views change place at once, and views
	   only cross-fade. */
	@media (prefers-reduced-motion: reduce) {
		.pane-shell,
		.tab-slot,
		.resize-handle,
		.drag-overlay,
		.resize-handle.visible .resize-grip {
			transition: none;
		}

		.tab-slot[data-motion="enter"],
		.tab-slot[data-motion="enter"][data-clip][data-side] {
			animation: slot-fade-in var(--motion-state) linear both;
		}

		.tab-slot[data-motion="leave"],
		.tab-slot[data-motion="leave"][data-clip][data-side] {
			animation: slot-fade-out var(--motion-state) linear forwards;
		}
	}
</style>
