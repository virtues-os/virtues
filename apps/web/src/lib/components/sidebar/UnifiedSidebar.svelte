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
	import GettingStartedCard from "./GettingStartedCard.svelte";
	import { gettingStarted } from "$lib/stores/gettingStarted.svelte";
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
				keys: "mod+s",
				label: "Show or hide the sidebar",
				group: "Window",
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

	function handleNewChat() {
		// Always open a new chat tab (forceNew ensures we don't reuse existing)
		windowShellStore.openTabFromRoute("/", {
			label: "New Chat",
			forceNew: true,
		});
	}

	function handleNewTemporaryChat() {
		// Ghost chat — never saved to history
		windowShellStore.openTabFromRoute("/?temporary=1", {
			label: "Temporary Chat",
			forceNew: true,
		});
	}

	async function handleNewPage() {
		// Create a new page and open it in a new tab
		const { pagesStore } = await import("$lib/stores/pages.svelte");
		const page = await pagesStore.createNewPage();
		windowShellStore.openTabFromRoute(`/page/${page.id}`, {
			label: page.title,
			forceNew: true,
		});
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
			"overflow-hidden",
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
	// The rail is fixed; only the panel resizes, so the whole aside is
	// rail + gap + panelWidth.
	const RAIL_W = 72;
	const PANEL_GAP = 12;

	const panelWidth = $derived(sidebarState.width);
	const asideWidth = $derived(isCollapsed ? RAIL_W : RAIL_W + PANEL_GAP + panelWidth);

	let resizing = $state(false);
	let dragStartX = 0;
	let dragStartWidth = 0;
	// The UNCLAMPED width the pointer is asking for. The stored width is clamped
	// to [MIN, MAX], so it can't tell us the user has dragged well past the
	// floor — which is exactly the gesture that should close the sidebar.
	let dragIntent = 0;

	function onResizeStart(e: PointerEvent) {
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
		if (dragIntent < SIDEBAR_COLLAPSE_AT) {
			sidebarState.width = dragStartWidth;
			sidebarState.collapsed = true;
		}
	}

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

		{#if !isCollapsed && gettingStarted.loaded && !gettingStarted.unsupported && !gettingStarted.graduated}
			<GettingStartedCard />
		{/if}
		<SidebarFooter collapsed={isCollapsed} />
	</div>

	{#if !isCollapsed}
		<!-- The seam is the handle. It sits over the panel's right border — the
		     same line that divides the panel from the pane — so the thing you
		     grab is the thing you see. -->
		<div
			class="sidebar-resizer"
			class:dragging={resizing}
			role="separator"
			aria-orientation="vertical"
			aria-label="Resize the sidebar"
			aria-valuenow={panelWidth}
			aria-valuemin={SIDEBAR_MIN_WIDTH}
			aria-valuemax={SIDEBAR_MAX_WIDTH}
			tabindex="0"
			onpointerdown={onResizeStart}
			onpointermove={onResizeMove}
			onpointerup={onResizeEnd}
			onpointercancel={onResizeEnd}
			onkeydown={onResizeKey}
			ondblclick={() => sidebarState.resetWidth()}
		></div>
	{/if}
</aside>


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

	/* A wider hit area than the 1px line it straddles — 8px is the smallest
	   comfortable grab target, and it is invisible until you are on it. */
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

	/* The MARK: a 1px line, exactly over the panel's right border, that
	   thickens to 2px and takes the theme's accent on approach.
	
	   Filling the 8px target was the wrong read — it made the invisible hit
	   area visible, which is a band of chrome appearing out of nothing where
	   the user expected a line to respond. Lighting the line instead says the
	   same thing about the same object: this edge is the thing you can move.
	   Colour is doing work here, not decorating — it marks the one draggable
	   edge in the shell — which is what design.md asks of any colour it
	   allows. Grown from the right edge so the line never moves; only its
	   weight does. */
	.sidebar-resizer::after {
		content: "";
		position: absolute;
		top: 0;
		bottom: 0;
		right: 0;
		width: 0;
		background: var(--color-primary);
		transition:
			width 120ms var(--ease-premium),
			opacity 120ms var(--ease-premium);
		opacity: 0;
	}

	.sidebar-resizer:hover::after,
	.sidebar-resizer:focus-visible::after {
		width: 2px;
		opacity: 1;
	}

	/* Held: full weight, no fade — the line is the thing being dragged. */
	.sidebar-resizer.dragging::after {
		width: 2px;
		opacity: 1;
		transition: none;
	}

	@media (prefers-reduced-motion: reduce) {
		.sidebar-resizer::after {
			transition: none;
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
