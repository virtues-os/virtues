<script lang="ts">
	import { onMount } from "svelte";
	import { fly } from "svelte/transition";
	import { cubicIn, cubicOut } from "svelte/easing";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import Icon from "$lib/components/Icon.svelte";
	import {
		sidebarState,
		SIDEBAR_COLLAPSE_AT,
		SIDEBAR_MIN_WIDTH,
		SIDEBAR_MAX_WIDTH,
	} from "$lib/stores/sidebarState.svelte";
	import { search } from "$lib/stores/search.svelte";
	import SidebarFooter from "./SidebarFooter.svelte";
	import SidebarRail from "./SidebarRail.svelte";
	import SidebarPanel from "./SidebarPanel.svelte";
	import { sidebarRoom } from "$lib/stores/sidebarRoom.svelte";
	import { shortcuts } from "$lib/shortcuts/registry.svelte";
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
				keys: "mod+shift+t",
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
		windowShellStore.openTabFromRoute("/wiki", {
			label: "Wiki",
			preferEmptyPane: true,
		});
	}

	// New chat / page navigate the window you are in. Only "+" and "open
	// beside" make windows — see openTabFromRoute in the window shell store.
	function handleNewChat() {
		windowShellStore.openTabFromRoute("/", { label: "New Chat" });
	}

	function handleNewTemporaryChat() {
		// Ghost chat — never saved to history
		windowShellStore.openTabFromRoute("/?temporary=1", { label: "Temporary Chat" });
	}

	async function handleNewPage() {
		const { pagesStore } = await import("$lib/stores/pages.svelte");
		const page = await pagesStore.createNewPage();
		windowShellStore.openTabFromRoute(`/page/${page.id}`, { label: page.title });
	}

	function toggleCollapse() {
		sidebarState.toggle();
	}

	// The panel swaps as one object, not as a cascade of rows. Sequential, not
	// concurrent: Svelte runs `in:` and `out:` together by default, which shows
	// two half-transparent copies of a list sliding through each other and reads
	// as smeared. So the old panel leaves first (110ms, short travel), and the
	// new one waits for it before arriving (210ms over 18px). The grid cell holds
	// the height throughout, so nothing collapses in the gap.
	const swapKey = $derived(sidebarRoom.selectedId);

	// The rail is the desk-ground strip; the panel is a card floating on it, so
	// the aside itself carries no inset — the rail flushes left on the ground,
	// the panel supplies its own card margins.
	// NOT a springy curve. The aside used to open on cubic-bezier(0.34, 1.56,
	// 0.64, 1) — the 1.56 is an overshoot — and an overshoot is incompatible
	// with a merged seam: the aside momentarily grew WIDER than rail + gap +
	// panel, so for a few frames a strip of desk ground opened between the
	// panel's right edge and the pane it is supposed to be joined to, and the
	// whole card appeared to rubber-band. A seam can only stay shut if the
	// thing carrying it decelerates into its final width and stops there.
	const sidebarClass = $derived.by(() =>
		[
			"sidebar-container relative flex h-full bg-transparent",
			resizing ? "" : "transition-[width] duration-300 ease-[var(--ease-premium)]",
			// Clips like overflow-hidden (the panel must not spill while the
			// width animates) but lets the seam's grip, which is centred on the
			// panel's right border, overhang the edge by its own half-width.
			// min-w-0 keeps the flex sizing overflow-hidden used to imply; z-[1]
			// keeps the pane's positioned content from painting over that
			// overhang, since clip-path makes the aside its own stacking layer.
			"z-[1] min-w-0 [clip-path:inset(0_-3px_0_0)]",
		].join(" "),
	);

	// Merged with the pane whenever the panel is open — split included. Then
	// panel + pane(s) read as one white card split by lines: the panel rounds
	// only its left corners and its RIGHT border is the first divider; the pane
	// (in +layout) rounds only right and drops its left border and left margin
	// so the two abut with no desk gap. In split, the pane resizer draws the
	// second divider inside that same card.
	const merged = $derived(!isCollapsed);

	// ── Resizing the seam ─────────────────────────────────────
	// Only the panel resizes by drag; the rail switches between two widths
	// (labels on/off). The whole aside is rail + gap + panelWidth.
	const PANEL_GAP = 12;
	const railWidth = $derived(sidebarState.railWidth);

	const panelWidth = $derived(sidebarState.width);
	const asideWidth = $derived(isCollapsed ? railWidth : railWidth + PANEL_GAP + panelWidth);

	let resizing = $state(false);
	let dragStartX = 0;
	let dragStartWidth = 0;
	// The UNCLAMPED width the pointer is asking for. The stored width is clamped
	// to [MIN, MAX], so it can't tell us the user has dragged well past the
	// floor — which is exactly the gesture that should close the sidebar.
	let dragIntent = 0;
	// Under this much travel, a press on the seam is a click.
	const CLICK_SLOP_PX = 3;

	function onResizeStart(e: PointerEvent) {
		hideTip();
		resizing = true;
		dragStartX = e.clientX;
		dragStartWidth = sidebarState.width;
		dragIntent = dragStartWidth;
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
		dragIntent = dragStartWidth + (e.clientX - dragStartX);
		// The setter clamps, so the panel stops at the floor while the pointer
		// keeps going — the drag feels like it hit a wall rather than jittering.
		sidebarState.width = dragIntent;
	}

	function onResizeEnd(e: PointerEvent) {
		if (!resizing) return;
		resizing = false;
		try {
			(e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
		} catch {
			// Capture can already be gone if the pointer left the window.
		}
		// Decided on release, not mid-drag: collapsing while the pointer is still
		// down would yank the handle out from under it and drop the capture.
		// A press that barely moved is a click, and the tooltip promises a click
		// hides the sidebar.
		const clicked = Math.abs(e.clientX - dragStartX) < CLICK_SLOP_PX;
		if (clicked || dragIntent < SIDEBAR_COLLAPSE_AT) {
			sidebarState.width = dragStartWidth;
			sidebarState.collapsed = true;
		}
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

	// Collapsing unmounts the seam before pointerleave can fire.
	$effect(() => {
		if (isCollapsed) hideTip();
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

	const panelColumnClass = $derived.by(() =>
		[
			"flex flex-col",
			"my-3 ml-3 overflow-hidden border border-border bg-surface",
			merged ? "panel-card-left" : "panel-card",
			isCollapsed ? "pointer-events-none" : "",
		].join(" "),
	);
</script>

<aside class={sidebarClass} style="width: {asideWidth}px">
	<SidebarRail />

	<div class={panelColumnClass} style="width: {panelWidth}px; min-width: {panelWidth}px">
		<nav class="workspace-nav">
			{#if !storeReady}
				<div class="loading-state">
					<Icon icon="ri:loader-4-line" width="16" class="spinner" />
					<span>Loading...</span>
				</div>
			{:else}
				<div class="nav-swap">
					{#key swapKey}
						<div
							class="nav-layer"
							in:fly={{ x: 18, duration: 210, delay: 110, easing: cubicOut, opacity: 0 }}
							out:fly={{ x: -10, duration: 110, easing: cubicIn, opacity: 0 }}
						>
							<SidebarPanel room={sidebarRoom.selected} />
						</div>
					{/key}
				</div>
			{/if}
		</nav>

		<SidebarFooter collapsed={isCollapsed} />
	</div>

	{#if !isCollapsed}
		<!-- The seam is the handle. It sits over the panel's right border — the
		     same line that divides the panel from the pane — so the thing you
		     grab is the thing you see. Click hides, drag resizes. -->
		<div
			class="sidebar-resizer"
			class:dragging={resizing}
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
			onpointercancel={onResizeEnd}
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

	/* Same --card-radius as the pane, so the sidebar is never rounded
	   differently from the pages beside it. Merged, it rounds only on the desk
	   side and stays square at the divider. */
	:global(.panel-card) { border-radius: var(--card-radius); }
	:global(.panel-card-left) {
		border-radius: var(--card-radius) 0 0 var(--card-radius);
	}

	/* The hit area. Always transparent — it is a target, never a mark.
	
	   Inset by 12px top and bottom to sit inside the panel card (`my-3`). It
	   used to run top:0 to bottom:0 of the full-height aside, so its hover
	   fill spilled 12px ABOVE the card's rounded top corner and 12px below it:
	   a grey band standing on the desk ground, next to the card rather than on
	   it. A seam cannot start above the thing it divides. */
	.sidebar-resizer {
		position: absolute;
		top: 12px;
		bottom: 12px;
		right: 0;
		width: 8px;
		z-index: 20;
		cursor: col-resize;
		touch-action: none;
		background: transparent;
	}

	/* The grip: a pill centred on the panel's right border, hidden until the
	   pointer finds the seam, so the resting panel is just a clean line. The
	   border itself never changes; the grip is the one thing that responds.
	   It fades in and lengthens on approach and takes the accent only while
	   held. The border's centre is 0.5px in from the right edge; the 4px pill
	   is centred there, overhanging by 1.5px (the aside's clip allows it). */
	.sidebar-resizer-grip {
		position: absolute;
		top: 50%;
		right: -1.5px;
		width: 4px;
		height: 32px;
		transform: translateY(-50%);
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

	.sidebar-resizer:hover .sidebar-resizer-grip,
	.sidebar-resizer:focus-visible .sidebar-resizer-grip {
		opacity: 1;
		height: 40px;
	}

	/* Held: the accent says you have it. No easing — it follows the pointer. */
	.sidebar-resizer.dragging .sidebar-resizer-grip {
		background: var(--color-primary);
		opacity: 1;
		height: 40px;
		transition: none;
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
		border-radius: 6px;
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
		.sidebar-resizer-grip {
			transition: none;
		}
		.seam-tip {
			animation: none;
		}
	}

	.workspace-nav {
		flex: 1;
		min-height: 0;
		overflow: hidden;
		/* SidebarPanel owns its own head row and body padding, so the column does
		   not add a second inset on top of it — two nested paddings are how the
		   "one left edge" rule quietly breaks. */
		padding: 0;
	}

	/* Both layers share one grid cell, so the leaving panel doesn't push the
	   arriving one around while they overlap. */
	.nav-swap {
		display: grid;
		height: 100%;
		min-height: 0;
	}

	.nav-layer {
		grid-area: 1 / 1;
		min-width: 0;
		min-height: 0;
		overflow: hidden;
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
