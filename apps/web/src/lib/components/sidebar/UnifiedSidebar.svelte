<script lang="ts">
	import { onMount } from "svelte";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import Icon from "$lib/components/Icon.svelte";
	import {
		sidebarState,
		SIDEBAR_COLLAPSE_AT,
		SIDEBAR_MIN_WIDTH,
		SIDEBAR_MAX_WIDTH,
		SIDEBAR_INSET,
		PANE_GAP_PX,
		RAIL_GLASS_PX,
		RAIL_WIDTH,
	} from "$lib/stores/sidebarState.svelte";
	import { search } from "$lib/stores/search.svelte";
	import { trafficLights } from "$lib/tauri/trafficLights.svelte";
	import SidebarFooter from "./SidebarFooter.svelte";
	import HomePanel from "./panels/HomePanel.svelte";
	import { shortcuts } from "$lib/shortcuts/registry.svelte";
	import { newPage, newTask } from "$lib/utils/newActions";
	import { onSummon, setSummonShortcut, storedSummonChord } from "$lib/tauri/bridge";

	// Collapsed state from shared store (also consumed by WindowTabBar)
	const isCollapsed = $derived(sidebarState.collapsed);

	// The palette's open state lives in a store so the phone shell can reach it
	// too; the modal itself mounts once at the app layout. See stores/search.

	// Track if store is ready
	let storeReady = $state(false);

	// Initialize window shell store and keyboard shortcuts
	onMount(() => {
		windowShellStore
			.init()
			.then(() => {
				storeReady = true;
			})
			.catch((err) => {
				console.error("[UnifiedSidebar] Failed to initialize:", err);
				storeReady = true;
			});

		// The OS-global chord. Native has already focused the window by the time
		// this fires; summoning the app and then making you press ⌘K is two
		// steps for one intent, so it opens the palette. Reaching for Virtues
		// from another app is nearly always reaching for something *in* it.
		//
		// `open`, not `toggle`: the chord arrives from outside, where you can't
		// see whether the palette is already up, and a toggle would close it
		// half the time for no reason the user could have predicted.
		let unlistenSummon: (() => void) | null = null;
		let disposed = false;
		void onSummon(() => {
			search.show();
		}).then((un) => {
			// onMount's cleanup may already have run — this resolves a tick late.
			if (disposed) un();
			else unlistenSummon = un;
		});

		// Re-apply the stored rebind. Native binds the default at startup so the
		// chord works before any window exists; this replaces it if the user has
		// chosen another.
		const chord = storedSummonChord();
		void setSummonShortcut(chord);

		// Global shortcuts live in the registry, not in a hand-rolled if-chain.
		// Besides discoverability, the registry matches modifiers exactly — the
		// old chain tested `metaKey && key === 's'` without excluding Shift, so
		// ⌘⇧S collapsed the sidebar as a side effect.
		const unregisterShortcuts = shortcuts.register(
			{
				id: "chat.new-temporary",
				// ⌘⇧T is the browsers' reopen-closed-tab, below. I for incognito.
				keys: "mod+shift+i",
				label: "New temporary chat",
				group: "Create",
				run: handleNewTemporaryChat,
			},
			{
				id: "page.new",
				keys: "mod+shift+n",
				label: "New page",
				group: "Create",
				run: handleNewPage,
			},
			{
				// ⌘⇧T reopens a closed tab, so a task takes E.
				id: "task.new",
				keys: "mod+shift+e",
				label: "New task",
				group: "Create",
				run: handleNewTask,
			},
			{
				id: "tab.reopen",
				keys: "mod+shift+t",
				label: "Reopen closed tab",
				group: "Window",
				run: () => void windowShellStore.reopenClosedTab(),
			},
			{
				id: "chat.new",
				keys: "mod+n",
				label: "New chat",
				group: "Create",
				run: handleNewChat,
			},
			{
				id: "sidebar.toggle",
				keys: "mod+b",
				label: "Show or hide the sidebar",
				group: "Window",
				// ⌘B is bold in the composer and the pages editor. Inside a text
				// field the editor keeps it; everywhere else it moves the sidebar.
				allowInInput: false,
				run: toggleCollapse,
			},
			{
				id: "search.toggle",
				keys: "mod+k",
				label: "Ask or search",
				group: "Window",
				run: toggleSearch,
			},
			{
				id: "wiki.open",
				keys: "mod+w",
				label: "Open the wiki",
				group: "Go to",
				run: handleWikiOverview,
			},
		);

		return () => {
			disposed = true;
			unlistenSummon?.();
			unregisterShortcuts();
		};
	});

	function toggleSearch() {
		search.toggle();
	}

	function handleWikiOverview() {
		windowShellStore.openTabFromRoute("/wiki", { label: "Wiki", focusExisting: true });
	}

	// New chat / page navigate the window you are in. Only "+" and "open
	// beside" make windows — see openTabFromRoute in the window shell store.
	function handleNewChat() {
		windowShellStore.newChat();
	}

	function handleNewTemporaryChat() {
		// Ghost chat — never saved to history
		windowShellStore.openTabFromRoute("/?temporary=1", { label: "Temporary chat" });
	}

	function handleNewPage() {
		void newPage((route, label) => windowShellStore.openTabFromRoute(route, { label }));
	}

	function handleNewTask() {
		void newTask((route, label) => windowShellStore.openTabFromRoute(route, { label, focusExisting: true }));
	}

	function toggleCollapse() {
		sidebarState.toggle();
	}

	// The sidebar floats on the painting as glass; the aside carries no inset
	// of its own, and the column supplies its margin (the window's air).
	// It eases in and out on --ease-sidebar, the curve the header's strips
	// share, and never springs: an overshoot grows the aside past its final
	// width for a few frames, so the pane beside it jumps out and back and
	// the whole window seems to rubber-band.
	const sidebarClass = $derived.by(() =>
		[
			"sidebar-container relative flex h-full bg-transparent",
			resizing ? "" : "transition-[width] duration-[var(--motion-sidebar)] ease-[var(--ease-sidebar)] motion-reduce:transition-none",
			// The panel must not spill while the width animates. A clip-path
			// would also cut the glass off from the painting behind the aside
			// (it makes the aside a backdrop root), so this is a plain overflow
			// clip; the seam sits inside the aside, in the gap before the pane.
			"z-[1] min-w-0 overflow-hidden",
		].join(" "),
	);

	// ── Resizing the seam ─────────────────────────────────────
	// The aside is the window's air, the glass and the gap to the pane.
	// Closed, it is the rail; the ∴ (⌘B) opens the full sidebar again.
	const panelWidth = $derived(sidebarState.width);

	// Each word's place in the column, so they come back top to bottom.
	// Set on every open and close: the rows change between them. Capped so a
	// long Recent list does not drag the last words out.
	let columnEl = $state<HTMLElement | null>(null);
	const WORDS = ".panel-row-text, .sidebar-label, .door-meta, .group-head, .pin-head, .recent-zone";
	const WORD_STAGGER_CAP = 16;
	$effect(() => {
		void isCollapsed;
		columnEl?.querySelectorAll<HTMLElement>(WORDS).forEach((el, i) => {
			el.style.setProperty("--i", String(Math.min(i, WORD_STAGGER_CAP)));
		});
	});
	const asideWidth = $derived(isCollapsed ? RAIL_WIDTH : SIDEBAR_INSET + panelWidth + PANE_GAP_PX);

	// ── The glass ─────────────────────────────────────────────
	// One pane of glass behind the column, reaching up into the window
	// header so the ∴ (HeaderMark) sits on it as the sidebar's first line.
	// It is fixed to the window rather than to the aside, which starts below
	// the header and clips. Open, it runs to the window's bottom air, as the
	// panes do. Closed, it narrows to the rail and ends just under its last
	// glyph (the doors, then the pins), so the rail rests on the painting as
	// an object rather than a bar. Both sizes ease on the sidebar's clock,
	// so the glass and the words move together.
	//
	// In the Mac app the window's buttons sit in the header row while the
	// window is not full screen. The rail is too narrow to hold them, so
	// with the sidebar closed the glass starts below the header and the ∴
	// stands beside the buttons on its own glass (HeaderMark).
	const reachesTop = $derived(!(isCollapsed && trafficLights.shown));
	const glassTop = $derived(reachesTop ? "var(--chrome-control-top)" : "var(--chrome-row-h)");
	/** Air below the rail's last glyph. */
	const RAIL_END_PX = 4;
	/** Where the rail ends, in px from the window's top. */
	let railBottom = $state(0);
	$effect(() => {
		const column = columnEl;
		if (!column) return;
		const measure = () => {
			const end = column.querySelector(".pinned-zone") ?? column.querySelector(".panel-doors");
			if (!end) return;
			// In the rail a group's label takes no room (.railed .group-head),
			// and while the sidebar opens or closes it is partway there, so
			// the pins below it are moving. Leaving out whatever height it
			// has now gives the rail's end in its closed layout at every
			// moment, so the glass's target holds still for the whole close
			// instead of moving on every frame.
			const head = end.querySelector(".group-head");
			const label = head ? head.getBoundingClientRect().height : 0;
			railBottom = Math.round(end.getBoundingClientRect().bottom - label + RAIL_END_PX);
		};
		// The doors and the pins change size as they fold, and the pins mount
		// and unmount with the first and last pin.
		const sizes = new ResizeObserver(measure);
		const watch = () => {
			sizes.disconnect();
			column.querySelectorAll(".panel-doors, .pinned-zone").forEach((el) => sizes.observe(el));
		};
		const parts = new MutationObserver(() => {
			watch();
			measure();
		});
		watch();
		measure();
		parts.observe(column, { childList: true, subtree: true });
		return () => {
			sizes.disconnect();
			parts.disconnect();
		};
	});
	// The rows load after the window opens, and the pins after them, so
	// the rail cannot know its height at first. Until it has measured its
	// rows it is not shown, and until the window has settled it takes each
	// new size at once, so it never opens full height and then shrinks.
	const GLASS_SETTLE_MS = 800;
	let glassSettled = $state(false);
	onMount(() => {
		const t = setTimeout(() => (glassSettled = true), GLASS_SETTLE_MS);
		return () => clearTimeout(t);
	});
	const glassWidth = $derived(isCollapsed ? RAIL_GLASS_PX : panelWidth);
	// Both heights are plain calc() lengths, so the glass eases straight
	// from one to the other. The window's height caps the rail through
	// max-height, which does not move.
	const fullHeight = $derived(`calc(100% - ${glassTop} - var(--window-air))`);
	const glassHeight = $derived(
		isCollapsed && railBottom > 0 ? `calc(${railBottom}px - ${glassTop})` : fullHeight,
	);

	let resizing = $state(false);
	$effect(() => {
		sidebarState.resizing = resizing;
	});
	let dragStartX = 0;
	let dragStartY = 0;
	let dragStartWidth = 0;
	// The farthest the pointer got from where it pressed, in either axis. A
	// drag out and back ends where it started, so the release point alone
	// can't tell it from a click.
	let dragTravel = 0;
	// The UNCLAMPED width the pointer is asking for. The stored width is clamped
	// to [MIN, MAX], so it can't tell us the user has dragged well past the
	// floor — which is exactly the gesture that should close the sidebar.
	let dragIntent = 0;
	// The pointer is far enough past the floor that releasing will close the
	// sidebar. The grip bends into a chevron and the column dims, so you can
	// see that before you let go.
	let collapseArmed = $state(false);
	// Under this much travel, a press on the seam is a click.
	const CLICK_SLOP_PX = 3;

	function onResizeStart(e: PointerEvent) {
		// The primary button only: a right-click or a two-finger tap would
		// otherwise count as a click on release and hide the sidebar.
		if (e.button !== 0) return;
		hideTip();
		resizing = true;
		dragStartX = e.clientX;
		dragStartY = e.clientY;
		dragStartWidth = sidebarState.width;
		dragIntent = dragStartWidth;
		dragTravel = 0;
		try {
			(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
		} catch {
			// Synthetic or already-captured pointers: the drag still works off
			// the move handler, it just won't survive leaving the element.
		}
		e.preventDefault();
	}

	function onResizeMove(e: PointerEvent) {
		if (!resizing) return;
		// No button held means the release went somewhere this element never
		// heard about. End the drag here, or plain hovering would resize.
		if (e.buttons === 0) {
			stopDrag();
			return;
		}
		dragTravel = Math.max(dragTravel, Math.abs(e.clientX - dragStartX), Math.abs(e.clientY - dragStartY));
		dragIntent = dragStartWidth + (e.clientX - dragStartX);
		// The setter clamps, so the panel stops at the floor while the pointer
		// keeps going — the drag feels like it hit a wall rather than jittering.
		sidebarState.width = dragIntent;
		collapseArmed = dragIntent < SIDEBAR_COLLAPSE_AT;
	}

	function onResizeEnd(e: PointerEvent) {
		if (!resizing) return;
		// Decided on release, not mid-drag: collapsing while the pointer is still
		// down would yank the handle out from under it and drop the capture.
		// A press that never moved is a click, and the tooltip promises a click
		// hides the sidebar. Otherwise release does what the grip showed: it
		// closes only when the grip was armed. Decided before the capture is
		// released, since losing capture ends the drag.
		const clicked = Math.max(dragTravel, Math.abs(e.clientX - dragStartX)) < CLICK_SLOP_PX;
		const close = clicked || collapseArmed;
		stopDrag();
		try {
			(e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
		} catch {
			// Capture can already be gone if the pointer left the window.
		}
		if (close) {
			sidebarState.width = dragStartWidth;
			sidebarState.collapsed = true;
		}
	}

	/** End the drag where it is, deciding nothing. */
	function stopDrag() {
		resizing = false;
		collapseArmed = false;
	}

	// ── The seam's tooltip ────────────────────────────────────
	// Rendered outside the aside: the aside's clip-path would cut it off.
	// Fixed-positioned beside the grip, which sits at the seam's middle.
	const TIP_DELAY_MS = 500;
	let tipAt = $state<{ left: number; top: number } | null>(null);
	let tipTimer: ReturnType<typeof setTimeout> | undefined;

	function showTipSoon(e: PointerEvent) {
		if (resizing) return;
		const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
		clearTimeout(tipTimer);
		tipTimer = setTimeout(() => {
			tipAt = { left: rect.right + 8, top: rect.top + rect.height / 2 };
		}, TIP_DELAY_MS);
	}

	function hideTip() {
		clearTimeout(tipTimer);
		tipAt = null;
	}

	// Collapsing unmounts the seam before pointerleave or pointerup can fire:
	// ⌘B or the header button can close the sidebar mid-drag.
	$effect(() => {
		if (!isCollapsed) return;
		hideTip();
		stopDrag();
	});

	// design.md: any drag affordance needs a keyboard path, because HTML5 drag
	// events don't fire on touch at all.
	function onResizeKey(e: KeyboardEvent) {
		const step = e.shiftKey ? 32 : 16;
		if (e.key === "ArrowLeft") {
			e.preventDefault();
			sidebarState.width = sidebarState.width - step;
		} else if (e.key === "ArrowRight") {
			e.preventDefault();
			sidebarState.width = sidebarState.width + step;
		} else if (e.key === "Home") {
			e.preventDefault();
			sidebarState.resetWidth();
		} else if (e.key === "Enter") {
			e.preventDefault();
			sidebarState.collapsed = true;
		}
	}

	// ── Moving through the column by keyboard ─────────────────────────────
	// Up and Down step through the rows and group heads, Home and End go to
	// the first and last. Tab still moves on to the next control as usual.
	// Alt-arrows belong to Pinned (they move a pin), so only bare arrows count.
	const NAV_ROWS = ".panel-row:not(.panel-empty):not(.pin-drop-empty), .group-toggle";

	function onColumnKey(e: KeyboardEvent) {
		if (e.altKey || e.metaKey || e.ctrlKey || e.shiftKey) return;
		if (!['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(e.key)) return;
		const target = e.target as HTMLElement;
		if (target.closest('input, textarea, [contenteditable="true"]')) return;
		const column = e.currentTarget as HTMLElement;
		const rows = [...column.querySelectorAll<HTMLElement>(NAV_ROWS)].filter((r) => r.offsetParent !== null && !r.closest('[inert]'));
		if (rows.length === 0) return;
		const at = rows.findIndex((r) => r === target || r.contains(target));
		let next = 0;
		if (e.key === 'Home') next = 0;
		else if (e.key === 'End') next = rows.length - 1;
		else if (at < 0) next = 0;
		else next = Math.max(0, Math.min(rows.length - 1, at + (e.key === 'ArrowDown' ? 1 : -1)));
		e.preventDefault();
		rows[next].focus();
		rows[next].scrollIntoView({ block: 'nearest' });
	}

	const panelColumnClass = $derived.by(() =>
		[
			"flex flex-col",
			// Transparent: the glass behind it is its own element, so it can
			// narrow and shorten to the rail while the rows stay put.
			"mb-3 ml-3 overflow-hidden",
			"sidebar-column",
			// Dimmed while releasing the seam would close the sidebar.
			"transition-opacity duration-150 ease-[var(--ease-premium)]",
			collapseArmed ? "opacity-50" : "",
		].join(" "),
	);
</script>

<aside class={sidebarClass} style="width: {asideWidth}px">
	<div
		class="sidebar-glass lq lq-regular"
		class:railed={isCollapsed}
		class:reaches-top={reachesTop}
		class:unmeasured={isCollapsed && railBottom === 0}
		class:resizing={resizing || !glassSettled}
		aria-hidden="true"
		style:left="{SIDEBAR_INSET}px"
		style:top={glassTop}
		style:width="{glassWidth}px"
		style:height={glassHeight}
		style:max-height={fullHeight}
	></div>
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<!-- Closed, the column is inert: still in the page while the width
	     animates, but out of the Tab order, the pointer's reach and the
	     accessibility tree. -->
	<!-- Closed, the column keeps its rows and the aside narrows to the
	     glyphs: the words fade and clip, and nothing in the DOM is swapped,
	     so the glyphs never move. -->
	<div bind:this={columnEl} class={panelColumnClass} class:railed={isCollapsed} style="width: {panelWidth}px; min-width: {panelWidth}px" onkeydown={onColumnKey}>
		<nav class="workspace-nav">
			{#if !storeReady}
				<div class="loading-state">
					<Icon icon="mynaui:spinner" width="16" class="spinner" />
					<span>Loading...</span>
				</div>
			{:else}
				<!-- One list, always: the sidebar never changes. Every door opens
				     a page, and a page that has sections carries their list
				     itself (the Wiki's index, Settings' SectionNav, Setup's
				     step strip). -->
				<div class="panel-body">
					<HomePanel />
				</div>
			{/if}
		</nav>

		<SidebarFooter collapsed={isCollapsed} />
	</div>

	{#if !isCollapsed}
		<!-- The seam is the handle: the gap between the sidebar and the
		     page, with the grip centred in it, the way the gutter works
		     between two panes in split. Click hides, drag resizes. -->
		<!-- A focusable separator with a value is a widget (arrow keys resize),
		     which Svelte's rule doesn't recognise; SplitContainer's gutters
		     carry the same ignore. -->
		<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
		<div
			class="sidebar-resizer"
			class:dragging={resizing}
			class:armed={collapseArmed}
			role="separator"
			aria-orientation="vertical"
			aria-label="Resize the sidebar"
			aria-describedby={tipAt ? "sidebar-seam-tip" : undefined}
			aria-valuenow={panelWidth}
			aria-valuemin={SIDEBAR_MIN_WIDTH}
			aria-valuemax={SIDEBAR_MAX_WIDTH}
			tabindex="0"
			onpointerenter={showTipSoon}
			onpointerleave={hideTip}
			onpointerdown={onResizeStart}
			onpointermove={onResizeMove}
			onpointerup={onResizeEnd}
			onpointercancel={() => resizing && stopDrag()}
			onlostpointercapture={() => resizing && stopDrag()}
			onkeydown={onResizeKey}
		><span class="sidebar-resizer-grip" aria-hidden="true"></span></div>
	{/if}
</aside>

{#if tipAt && !isCollapsed}
	<div
		id="sidebar-seam-tip"
		class="seam-tip"
		role="tooltip"
		style="left: {tipAt.left}px; top: {tipAt.top}px"
	>
		<span class="seam-tip-action">
			Hide sidebar <kbd>{shortcuts.format("mod+b")}</kbd>
		</span>
		<span class="seam-tip-hint">Drag to resize</span>
	</div>
{/if}


<style>
	@reference "../../../app.css";
	@reference "$lib/styles/sidebar.css";


	/* The hit area. It paints nothing.

	   The page starts at the aside's right edge, and the 10px gap before it
	   is the seam. The hit area is the whole gap and 1px of the glass, and
	   runs the column's full height. */
	.sidebar-resizer {
		position: absolute;
		top: 0;
		bottom: var(--window-air);
		right: 0;
		width: 11px;
		z-index: 20;
		cursor: col-resize;
		touch-action: none;
		background: transparent;
	}

	/* The grip: a pill centred on the line, hidden until the pointer finds
	   the seam, so at rest there is only the line. It fades in and lengthens
	   on approach and takes the accent only while held. The line runs 5 to
	   6px in from the hit area's right edge, so a 4px pill 3.5px in from
	   that edge is centred on it. Same object as the split gutter's grip.

	   The pill is drawn as two halves so it can bend into a chevron. Each
	   half runs from one end to 2px past the middle, so their round ends
	   overlap exactly there and the straight pill reads as one piece. */
	.sidebar-resizer-grip {
		position: absolute;
		top: 50%;
		right: 3.5px;
		width: 4px;
		height: 32px;
		transform: translateY(-50%);
		opacity: 0;
		pointer-events: none;
		z-index: 1;
		transition:
			opacity 160ms var(--ease-premium),
			height 160ms var(--ease-premium),
			transform 160ms var(--ease-premium);
	}

	.sidebar-resizer-grip::before,
	.sidebar-resizer-grip::after {
		content: "";
		position: absolute;
		left: 0;
		width: 100%;
		height: calc(50% + 2px);
		border-radius: var(--radius-full);
		background: var(--color-foreground-subtle);
		transition:
			transform 160ms var(--ease-premium),
			background-color 160ms var(--ease-premium);
	}

	/* Each half turns about the centre of its round end at the middle. */
	.sidebar-resizer-grip::before {
		top: 0;
		transform-origin: 50% calc(100% - 2px);
	}

	.sidebar-resizer-grip::after {
		bottom: 0;
		transform-origin: 50% 2px;
	}

	.sidebar-resizer:hover .sidebar-resizer-grip,
	.sidebar-resizer:focus-visible .sidebar-resizer-grip {
		opacity: 1;
		height: 40px;
	}

	/* Held: the accent says you have it, and it changes with no easing. */
	.sidebar-resizer.dragging .sidebar-resizer-grip {
		opacity: 1;
		height: 40px;
	}

	.sidebar-resizer.dragging .sidebar-resizer-grip::before,
	.sidebar-resizer.dragging .sidebar-resizer-grip::after {
		background: var(--color-primary);
		transition: transform 160ms var(--ease-premium);
	}

	/* Armed: releasing now closes the sidebar. The grip shortens and bends
	   into a chevron pointing the way the sidebar will go. It moves 4px left
	   so the chevron's open ends stay inside the aside's clip. */
	.sidebar-resizer.armed .sidebar-resizer-grip {
		height: 24px;
		transform: translate(-4px, -50%);
	}

	.sidebar-resizer.armed .sidebar-resizer-grip::before {
		transform: rotate(35deg);
	}

	.sidebar-resizer.armed .sidebar-resizer-grip::after {
		transform: rotate(-35deg);
	}

	/* Beside the grip, vertically centred on it. A hairline card on the
	   surface, like every other card — no shadow. */
	.seam-tip {
		position: fixed;
		z-index: 1000;
		transform: translateY(-50%);
		display: flex;
		flex-direction: column;
		gap: 4px;
		padding: 8px 12px;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		background: var(--color-surface-elevated);
		color: var(--color-foreground);
		font-size: 13px;
		line-height: 1.3;
		white-space: nowrap;
		pointer-events: none;
		animation: seam-tip-in 120ms var(--ease-premium);
	}

	.seam-tip-action {
		display: flex;
		align-items: baseline;
		gap: 12px;
	}

	.seam-tip kbd {
		font-family: inherit;
		color: var(--color-foreground-subtle);
	}

	.seam-tip-hint {
		color: var(--color-foreground-subtle);
	}

	@keyframes seam-tip-in {
		from { opacity: 0; }
		to { opacity: 1; }
	}

	@media (prefers-reduced-motion: reduce) {
		.sidebar-resizer-grip,
		.sidebar-resizer-grip::before,
		.sidebar-resizer-grip::after {
			transition: none;
		}
		.seam-tip {
			animation: none;
		}
	}

	/* Closed: the same rows, narrowed to their glyphs, and every word
	   fades. Group heads keep their height, so the glyphs below them stay
	   where they were.

	   Opening, the words wait until the sidebar is about 40% open, then
	   come into focus beside their glyphs: each fades in, slides 4px from
	   the left and loses a 2px blur, a row 12ms after the one above it
	   (--i, set in document order). Closing, they go in 100ms, before the
	   sidebar's edge reaches them, so no word is seen clipped. */
	.sidebar-column :global(:is(.panel-row-text, .sidebar-label, .door-meta, .group-head, .pin-head, .recent-zone)) {
		--word-delay: calc(130ms + var(--i, 0) * 12ms);
		transition:
			opacity 240ms var(--ease-premium) var(--word-delay),
			transform 280ms var(--ease-premium) var(--word-delay),
			filter 240ms var(--ease-premium) var(--word-delay);
	}

	.railed :global(:is(.panel-row-text, .sidebar-label, .door-meta, .group-head, .pin-head, .recent-zone)) {
		transform: translateX(-4px);
		filter: blur(2px);
		transition:
			opacity 100ms ease-in,
			transform 100ms ease-in,
			filter 100ms ease-in;
	}

	.railed :global(:is(.panel-row-text, .sidebar-label, .door-meta, .row-actions, .row-unread, .group-head, .pin-head, .recent-zone)) {
		opacity: 0;
		pointer-events: none;
	}

	/* A group's label takes no room in the rail, so the pins stand the
	   same 16px below the doors as the doors stand below the ∴. Closing,
	   the label goes first and its space closes after it; opening, the
	   space opens at once, ready for the label. */
	.sidebar-column :global(.group-head) {
		transition:
			opacity 240ms var(--ease-premium) var(--word-delay),
			transform 280ms var(--ease-premium) var(--word-delay),
			filter 240ms var(--ease-premium) var(--word-delay),
			height var(--motion-sidebar) var(--ease-sidebar);
	}

	.sidebar-column.railed :global(.group-head) {
		height: 0;
		transition:
			opacity 100ms ease-in,
			transform 100ms ease-in,
			filter 100ms ease-in,
			height var(--motion-sidebar) var(--ease-sidebar) 100ms;
	}

	/* The glass (`.lq-regular`, app.css) behind the column: the opened
	   sidebar's panel, or the rail. It changes shape in two beats, so the
	   rail reads as the same piece of glass growing into the panel.
	   Opening, it runs down to full height first, quickly, and widens on a
	   spring that carries it a few pixels into the gap before it settles
	   (--ease-sidebar-glass). Closing, it narrows and draws up to the
	   rail's last glyph together, on the sidebar's clock. In the Mac app
	   the rail starts below the header, so the top edge moves too: up
	   around the ∴ with the height when opening, back down when closing.
	   It follows the pointer with no easing while the seam is dragged. */
	.sidebar-glass {
		position: fixed;
		z-index: 0;
		border-radius: var(--radius-xl);
		pointer-events: none;
		transition:
			width 560ms var(--ease-sidebar-glass),
			height 300ms var(--ease-sidebar),
			top 300ms var(--ease-sidebar);
	}

	.sidebar-glass.railed {
		transition:
			width var(--motion-sidebar) var(--ease-sidebar),
			height var(--motion-sidebar) var(--ease-sidebar),
			top var(--motion-sidebar) var(--ease-sidebar);
	}

	.sidebar-glass.resizing {
		transition: none;
	}

	.sidebar-glass.unmeasured {
		opacity: 0;
	}

	.sidebar-column {
		position: relative;
		z-index: 1;
	}

	/* Rows stand 2px apart, open or closed. */
	.sidebar-column :global(:is(.panel-doors, .sidebar-expandable-inner)) {
		gap: 2px;
	}

	/* Closed, each row stands 4px in from the rail's edges, as it does
	   from the open panel's, with its glyph in the middle, so its hover
	   tint sits inside the glass rather than filling it edge to edge. The
	   glyph is where it was in the open row (10px into the row, both ways),
	   so it does not move. The rows narrow only once the words are gone, so
	   the words fade at full width instead of being cut off by the row.
	   Opening, the rows widen at once, before the words come back. */
	.sidebar-column :global(.panel-row) {
		transition:
			background-color var(--motion-hover-out) var(--ease-premium),
			color var(--motion-hover-out) var(--ease-premium),
			opacity var(--motion-hover-out) var(--ease-premium);
	}

	.sidebar-column.railed :global(.panel-row) {
		width: calc(var(--rail-w) - 8px);
		padding-right: 0;
		overflow: hidden;
		transition:
			width 0s 100ms,
			padding-right 0s 100ms,
			background-color var(--motion-hover-out) var(--ease-premium),
			color var(--motion-hover-out) var(--ease-premium);
	}

	@media (prefers-reduced-motion: reduce) {
		.sidebar-glass,
		.sidebar-glass.railed,
		.panel-body,
		.sidebar-column :global(:is(.panel-row-text, .sidebar-label, .door-meta, .group-head, .pin-head, .recent-zone)),
		.sidebar-column :global(.group-head),
		.sidebar-column.railed :global(.group-head),
		.sidebar-column :global(.panel-row),
		.sidebar-column.railed :global(.panel-row) {
			transition: none;
		}
	}

	.workspace-nav {
		flex: 1;
		min-height: 0;
		overflow: hidden;
		/* The body below owns the inset, so the column adds none: two nested
		   paddings are how the "one left edge" rule quietly breaks. */
		padding: 0;
	}

	/* Inside the glass: the rows stand 4px in from its sides, then each
	   row's 10px, so icons start 14px in, open as in the rail, and no
	   glyph moves sideways as the sidebar opens or closes. The ∴ above
	   them stands on the same column (HeaderMark), 9px under the glass's
	   top. The first door stands 16px below the ∴'s row, the air above a
	   group (HomePanel's group head). The ∴'s row ends a control's height
	   below the header's top air, and this column starts below the
	   header. */
	.panel-body {
		height: 100%;
		min-height: 0;
		/* No scrolling: Recent shows only the rows that fit (HomePanel). */
		overflow: hidden;
		padding: calc(var(--chrome-control-top) + var(--chrome-tab-h) + 16px - var(--chrome-row-h)) 4px 12px;
		transition: padding var(--motion-sidebar) var(--ease-sidebar);
	}

	@keyframes spin {
		from { transform: rotate(0deg); }
		to { transform: rotate(360deg); }
	}

	.loading-state {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 12px;
		color: var(--color-foreground-subtle);
		font-size: 13px;
	}

	.spinner { animation: spin 1s linear infinite; }
</style>
