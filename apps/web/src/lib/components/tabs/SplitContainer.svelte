<script lang="ts">
	import { dndzone } from "svelte-dnd-action";
	import type { DndEvent } from "svelte-dnd-action";
	import { windowShellStore, SPLIT_GUTTER_PX } from "$lib/stores/window-shell.svelte";
	import ActionToolbar from "./ActionToolbar.svelte";
	import {
		dndManager,
		type DndTabItem,
		type ZoneId,
	} from "$lib/stores/dndManager.svelte";
	import { gettingStarted } from "$lib/stores/gettingStarted.svelte";
	import TabContent from "./TabContent.svelte";
	import { arrive } from "$lib/tabs/arrive";
	import { beginTabToPane, tabBecomesPane, tabRect } from "$lib/tabs/split";
	import { mountedTabs, touch } from "$lib/tabs/keepAlive";
	import { onMount, untrack } from "svelte";
	import { listenForBrowserOpen } from "./browserPaneListener";
	import ChatView from "./views/ChatView.svelte";
	import type { Tab } from "$lib/tabs/types";

	let isResizing = $state(false);
	let containerRef = $state<HTMLElement | null>(null);

	// Drop items for left and right overlays
	let leftDropItems = $state<DndTabItem[]>([]);
	let rightDropItems = $state<DndTabItem[]>([]);

	// Zone IDs for split overlays
	const leftZoneId: ZoneId = { type: "split-overlay", paneId: "left" };
	const rightZoneId: ZoneId = { type: "split-overlay", paneId: "right" };

	/** A pane narrower than this cannot show a chat or a page. */
	const MIN_PANE_PX = 360;
	/** Within this many pixels of the middle, the divider settles on it. */
	const SNAP_PX = 12;
	// In split the panes are two cards with a gap between them. The resize
	// handle fills the gap and its hit area reaches a few px into each.
	const GUTTER_PX = SPLIT_GUTTER_PX;

	// Derived state for split mode
	const isSplitEnabled = $derived(windowShellStore.isSplit);

	// Track isDragging reactively via direct session access
	// Public session property enables Svelte 5 fine-grained reactivity
	const isDragging = $derived(dndManager.session !== null);
	// The pane the dragged tab is in. Its own pane takes no drop: the move
	// would change nothing.
	const dragFromPane = $derived(dndManager.session?.sourceZone.paneId ?? null);
	// Over a chat composer the tab is mentioned there instead
	// (`MENTION_DROP_ATTR`), so neither half takes it.
	const overComposer = $derived(dndManager.mentionTarget !== null);
	const leftTakesDrop = $derived(isDragging && !overComposer && isSplitEnabled && dragFromPane !== "left");
	const rightTakesDrop = $derived(
		isDragging &&
			!overComposer &&
			(isSplitEnabled
				? dragFromPane !== "right"
				: (windowShellStore.panes[0]?.tabs.length ?? 0) >= 2),
	);

	// Over a pane that will take it, the dragged tab straightens to say so.
	$effect(() => {
		dndManager.setOverPane(leftDropItems.length > 0 || rightDropItems.length > 0);
	});

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
	/** The two panes' widths (%) and the gap between them (px). */
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

	/** A view's box inside its side's pane, inside the pane's left border.
	 *  A pane has no right border: the window's edge or the next pane's
	 *  left border is there. */
	function slotBox(side: Side, f: Frame): { left: string; width: string } {
		const half = f.gutter / 2;
		// Inside the card's 1px border on both sides.
		if (side === "left") {
			return { left: "1px", width: `calc(${f.left}% - ${half + 2}px)` };
		}
		return {
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

		// Splitting by moving a tab across: the tab becomes the new pane
		// ($lib/tabs/split). Its place in the first strip is read now, before
		// the strips change.
		if (!before.split && after.split) {
			for (const [id, side] of after.visible) {
				if (side !== "right" || before.panes.get(id) !== "left") continue;
				const from = tabRect(id);
				if (!from || !containerRef || !beginTabToPane(containerRef)) continue;
				requestAnimationFrame(() => {
					if (!containerRef) return;
					const { width, height } = containerRef.getBoundingClientRect();
					tabBecomesPane(containerRef, id, from, {
						left: (width * to.left) / 100 + to.gutter / 2,
						top: 0,
						width: (width * to.right) / 100 - to.gutter / 2,
						height,
					});
				});
			}
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
			if (previous) {
				if (!isResizing) startMotion(previous, next);
			}
			previous = next;
		});
	});

	onMount(() => () => clearTimeout(motionTimer));

	// TODO(2026-11-22): migrate the divider onto PaneDivider, which the Inbox
	// uses; its snap to the middle and percent widths would become props.
	// The divider's drag works as the sidebar seam's does: pointer events with
	// capture on the handle, so the drag follows the pointer anywhere, works
	// with touch, and ends however it is interrupted.
	function handleDividerDown(e: PointerEvent) {
		// The primary button only: a right-click is not a drag.
		if (!isSplitEnabled || e.button !== 0) return;
		e.preventDefault();
		isResizing = true;
		windowShellStore.paneResizing = true;
		try {
			(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
		} catch {
			// Synthetic or already-captured pointers: the move handler still
			// follows while the pointer is over the handle.
		}
	}

	function handleDividerMove(e: PointerEvent) {
		if (!isResizing || !containerRef || !isSplitEnabled) return;
		// No button held means the release went somewhere the handle never
		// heard about, such as another app after ⌘-Tab. End the drag here.
		if (e.buttons === 0) {
			stopResize();
			return;
		}

		const rect = containerRef.getBoundingClientRect();
		setDivider(((e.clientX - rect.left) / rect.width) * 100);
	}

	function handleDividerUp(e: PointerEvent) {
		if (!isResizing) return;
		stopResize();
		try {
			(e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
		} catch {
			// Capture can already be gone if the pointer left the window.
		}
	}

	function stopResize() {
		isResizing = false;
		windowShellStore.paneResizing = false;
	}

	// The split can close mid-drag (the browser's Back, for one), and then the
	// handle stops taking pointer events before its release arrives.
	$effect(() => {
		if (!isSplitEnabled) stopResize();
	});

	/**
	 * Put the divider at `percent` of the window, keeping each pane at least
	 * MIN_PANE_PX wide (or half the window, when it is too narrow for two)
	 * and settling on the middle when it comes within SNAP_PX of it.
	 */
	function setDivider(percent: number) {
		const total = containerRef?.getBoundingClientRect().width ?? 0;
		if (!total) return;
		const min = Math.min(50, (MIN_PANE_PX / total) * 100);
		let next = Math.max(min, Math.min(100 - min, percent));
		if (Math.abs(next - 50) * (total / 100) < SNAP_PX) next = 50;
		windowShellStore.setPaneWidth(next);
	}

	function handleDividerKey(e: KeyboardEvent) {
		if (!isSplitEnabled) return;
		const step = e.shiftKey ? 10 : 2;
		if (e.key === "ArrowLeft" || e.key === "ArrowRight") {
			e.preventDefault();
			setDivider(leftWidth + (e.key === "ArrowLeft" ? -step : step));
		} else if (e.key === "Home") {
			e.preventDefault();
			windowShellStore.setPaneWidth(50);
		}
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
>
	<!-- Drag to Split Overlays - Always mounted so svelte-dnd-action registers zones before drag starts.
	     With one pane only the right half takes a drop, and only when there is
	     a tab left behind. In split a pane takes no drop of its own tab. The zone under the pointer takes a soft tint the
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
				dropFromOthersDisabled: !leftTakesDrop,
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
				dropFromOthersDisabled: !rightTakesDrop,
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
		onpointerdown={handleDividerDown}
		onpointermove={handleDividerMove}
		onpointerup={handleDividerUp}
		onpointercancel={() => isResizing && stopResize()}
		onlostpointercapture={() => isResizing && stopResize()}
		ondblclick={() => isSplitEnabled && windowShellStore.setPaneWidth(50)}
		onkeydown={handleDividerKey}
		role="separator"
		aria-orientation="vertical"
		aria-label="Resize the panes. Double-click to make them equal"
		aria-valuenow={isSplitEnabled ? Math.round(leftWidth) : undefined}
		aria-valuemin={0}
		aria-valuemax={100}
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
			use:arrive={{ route: tab.route, active: isActive }}
			data-tab-id={tab.id}
			data-side={side}
			data-active={isActive ? "" : undefined}
			data-motion={leaving ? "leave" : entering ? "enter" : undefined}
			data-clip={(leaving?.clip ?? entering?.clip) ? "" : undefined}
			style:display={isActive || leaving ? "flex" : "none"}
			style:left={box.left}
			style:width={box.width}
			onpointerdown={() => isSplitEnabled && handlePaneClick(paneId)}
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
		{@const newChatTab = {
			id: "new-chat-left",
			type: "chat",
			label: "New Chat",
			route: "/chat",
			history: ["/chat"],
			historyIndex: 0,
			createdAt: Date.now(),
		} satisfies Tab}
		<div
			class="tab-slot"
			data-side="left"
			data-active=""
			style:display="flex"
			style:left={box.left}
			style:width={box.width}
			onpointerdown={() => isSplitEnabled && handlePaneClick("left")}
		>
			<ActionToolbar tab={newChatTab} />
			<ChatView tab={newChatTab} active={true} />
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

	/* Pane shells: each is a sheet of paper standing on the painting
	   (`main` is only the window in the app layout), rounded and edged by a
	   hairline, with the gap between two of them showing the picture. The
	   focused pane is told by its active tab, not by its surface.
	   With one pane the right pane is zero wide and faded out, so opening
	   a side is that pane growing in from the window's edge. */
	.pane-shell {
		display: flex;
		flex-direction: column;
		flex: none;
		min-width: 0;
		overflow: hidden;
		background-color: var(--color-page);
		border: 1px solid var(--card-edge);
		border-radius: var(--radius-xl);
		transition:
			width var(--motion-layout) var(--ease-premium),
			background-color var(--motion-layout) var(--ease-premium),
			border-color var(--motion-layout) var(--ease-premium);
	}

	/* Home has no paper: its headline, composer and cards stand straight on
	   the painting. The paper goes at once, so a window that opens on home
	   never shows a sheet dissolving while the view loads, and comes back
	   with a fade when the chat begins (the transition above). */
	.split-container:has(> .tab-slot[data-side="left"][data-active] :global(.page-container.home)) > .pane-shell.left-pane,
	.split-container:has(> .tab-slot[data-side="right"][data-active] :global(.page-container.home)) > .pane-shell.right-pane {
		background-color: transparent;
		background-image: none;
		border-color: transparent;
		transition: width var(--motion-layout) var(--ease-premium);
	}

	/* Nor a toolbar: there is nothing on home to go back from or to name.
	   When the chat begins the paper fades in under it (the transition
	   above) and the toolbar comes in with the page as the chat gets its
	   route ($lib/tabs/arrive). The toolbar has no motion of its own, so a
	   tab switch does not replay one. */
	.tab-slot:has(:global(.page-container.home)) :global(.action-toolbar) {
		display: none;
	}

	/* A pane that has closed leaves its border behind as a 1px sliver, so
	   it hides, but only once it has finished closing: fading it on the way
	   would show the desk through the card that is taking its place. */
	.pane-shell.collapsed {
		opacity: 0;
		pointer-events: none;
		transition:
			width var(--motion-layout) var(--ease-premium),
			opacity 0s linear var(--motion-layout);
	}

	/* Tab content slots: absolutely positioned inside their side's pane.
	   Their box is set inline (`slotBox`) and eases with the panes, so a
	   view that changes sides slides there. */
	.tab-slot {
		position: absolute;
		top: 1px;
		bottom: 1px;
		border-radius: calc(var(--radius-xl) - 1px);
		flex-direction: column;
		overflow: hidden;
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
	}

	/* The card that opens out of a tab when it becomes a pane
	   ($lib/tabs/split), drawn over the panes while it grows. Both cards
	   move on its spring meanwhile. */
	.split-container:global(.tab-to-pane) > .pane-shell,
	.split-container:global(.tab-to-pane) > .tab-slot {
		transition-duration: var(--split-ms);
		transition-timing-function: var(--split-ease);
	}

	.split-container :global(.pane-stand-in) {
		position: absolute;
		z-index: 4;
		pointer-events: none;
		background-color: var(--color-page);
		border: 1px solid var(--card-edge);
		border-radius: var(--radius-xl);
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
		from { clip-path: inset(0 0 0 100%); }
		to { clip-path: inset(0 0 0 0); }
	}

	@keyframes slot-open-left {
		from { clip-path: inset(0 100% 0 0); }
		to { clip-path: inset(0 0 0 0); }
	}

	@keyframes slot-close-right {
		from { clip-path: inset(0 0 0 0); }
		to { clip-path: inset(0 0 0 100%); }
	}

	@keyframes slot-close-left {
		from { clip-path: inset(0 0 0 0); }
		to { clip-path: inset(0 100% 0 0); }
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

	/* The handle has no width of its own: it sits on the line between the
	   panes and its hit area reaches into each. */
	/* z-index lifts the grip above the panes' own stacked content (the chat
	   composer), which otherwise paints over the pill's overhang. */
	.resize-handle.visible {
		z-index: 5;
		opacity: 1;
		pointer-events: auto;
		/* A touch drag moves the divider instead of scrolling the page. */
		touch-action: none;
	}

	/* The grab target reaches 4px into each pane, so the seam is easy to
	   find from either side. */
	.resize-handle.visible::after {
		content: "";
		position: absolute;
		top: 0;
		bottom: 0;
		left: -4px;
		right: -4px;
	}

	/* The grip: a pill centred in the gutter, hidden until the pointer
	   finds the seam. It fades in and lengthens on approach and takes the
	   accent only while held. Same object as the sidebar seam's grip. */
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
		border-radius: var(--radius-full);
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
		transition: background-color var(--motion-state) var(--ease-premium);
		background: transparent;
	}

	/* Only receive pointer events when parent is visible */
	.drag-overlays.visible .drag-overlay {
		pointer-events: auto;
	}

	/* In split each half lies over its own pane. */
	.split-enabled .drag-overlay.left {
		flex: none;
		width: calc(var(--left-width) - (var(--gutter-width) / 2));
		margin-right: var(--gutter-width);
	}

	.split-enabled .drag-overlay.right {
		flex: none;
		width: calc(var(--right-width) - (var(--gutter-width) / 2));
	}

	/* With one pane the halves sit where the two panes would, so the tint
	   is the shape of the pane a drop opens. */

	.drag-overlay.active {
		background: color-mix(in srgb, var(--color-primary) 15%, transparent);
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
