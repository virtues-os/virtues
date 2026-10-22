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
	} from "$lib/stores/sidebarState.svelte";
	import { search } from "$lib/stores/search.svelte";
	import SidebarFooter from "./SidebarFooter.svelte";
	import OfflinePill from "./OfflinePill.svelte";
	import AtlasIcon from "./AtlasIcon.svelte";
	import { SHELL_GLYPH_PX } from "$lib/components/sidebar/AtlasIcon.svelte";
	import { SETTINGS_MODE, rowForRoute } from "$lib/sidebar/modes";
	import HomePanel from "./panels/HomePanel.svelte";
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
		windowShellStore.openTabFromRoute("/?temporary=1", { label: "Temporary Chat" });
	}

	async function handleNewPage() {
		const { pagesStore } = await import("$lib/stores/pages.svelte");
		const page = await pagesStore.createNewPage();
		windowShellStore.openTabFromRoute(`/page/${page.id}`, { label: page.title });
	}

	async function handleNewTask() {
		const { tasksStore } = await import("$lib/stores/tasks.svelte");
		tasksStore.requestAdd();
		windowShellStore.openTabFromRoute("/tasks", { label: "Tasks", focusExisting: true });
	}

	function toggleCollapse() {
		sidebarState.toggle();
	}

	// The sidebar sits on the desk; the aside carries no inset of its own,
	// and the column supplies its margin (the desk gap).
	// NOT a springy curve: an overshoot grows the aside past its final width
	// for a few frames, so the pane beside it jumps out and back and the
	// whole window seems to rubber-band. The aside decelerates into its width
	// and stops there.
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

	// ── Resizing the seam ─────────────────────────────────────
	// The aside is the desk gap plus the card. Closed, it is nothing: the
	// header's sidebar button is the way back.
	const panelWidth = $derived(sidebarState.width);
	const asideWidth = $derived(isCollapsed ? 0 : SIDEBAR_INSET + panelWidth);
	// Settings is a door: it opens the Settings page, which carries its own
	// section list (SectionNav). Lit while the focused pane shows a Settings
	// page, the way any row says where you are.
	const inSettings = $derived.by(() => {
		const pane = windowShellStore.activePane;
		const route = pane?.tabs.find((t) => t.id === pane.activeTabId)?.route;
		return !!rowForRoute(SETTINGS_MODE, route);
	});

	let resizing = $state(false);
	$effect(() => {
		sidebarState.resizing = resizing;
	});
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

	// ── Moving through the column by keyboard ─────────────────────────────
	// Up and Down step through the rows and group heads, Home and End go to
	// the first and last. Tab still moves on to the next control as usual.
	// Alt-arrows belong to Pinned (they move a pin), so only bare arrows count.
	const NAV_ROWS =
		'.panel-row:not(.panel-empty):not(.pin-drop-empty), .group-toggle, .panel-foot button';

	function onColumnKey(e: KeyboardEvent) {
		if (e.altKey || e.metaKey || e.ctrlKey || e.shiftKey) return;
		if (!['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(e.key)) return;
		const target = e.target as HTMLElement;
		if (target.closest('input, textarea, [contenteditable="true"]')) return;
		const column = e.currentTarget as HTMLElement;
		const rows = [...column.querySelectorAll<HTMLElement>(NAV_ROWS)].filter((r) => r.offsetParent !== null);
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
			// On the desk, not a card: only the work is raised. The sidebar
			// is chrome, and chrome paints nothing of its own (design.md).
			"mb-2 ml-2 overflow-hidden",
			"sidebar-column",
			isCollapsed ? "pointer-events-none" : "",
		].join(" "),
	);
</script>

<aside class={sidebarClass} style="width: {asideWidth}px">
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class={panelColumnClass} style="width: {panelWidth}px; min-width: {panelWidth}px" onkeydown={onColumnKey}>
		<nav class="workspace-nav">
			{#if !storeReady}
				<div class="loading-state">
					<Icon icon="ri:loader-4-line" width="16" class="spinner" />
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

		<!-- Settings stands at the foot: something you do to the server and
		     leave, apart from the places above. -->
		<div class="panel-foot">
			<OfflinePill collapsed={isCollapsed} />
			<button
				type="button"
				class="sidebar-interactive"
				class:active={inSettings}
				onclick={() => windowShellStore.openTabFromRoute("/virtues/you", { label: "Settings", focusExisting: true })}
			>
				<AtlasIcon name="settings" size={SHELL_GLYPH_PX} bare />
				<span class="sidebar-label">Settings</span>
			</button>
		</div>

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


	/* The hit area. Always transparent — it is a target, never a mark.

	   Inset by the desk gap at the bottom to match the column (`mb-2`); the
	   card's top is the aside's top, under the window header. Running past
	   the card put its hover fill on the desk ground beside it. A seam cannot
	   start above the thing it divides. */
	.sidebar-resizer {
		position: absolute;
		top: 0;
		bottom: 8px;
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

	.panel-foot {
		flex: none;
		padding: 0 4px 8px;
	}

	/* Settings is a door like the ones at the top of the panel, so it takes
	   their glyph tone and their line height. */
	.panel-foot :global(.atlas-icon) {
		color: var(--color-foreground-muted);
		transition: color var(--motion-hover-out) var(--ease-premium);
	}

	.panel-foot button:is(:hover, .active) :global(.atlas-icon) {
		color: var(--color-foreground);
	}

	.panel-foot .sidebar-label {
		line-height: normal;
	}

	/* As the panel's rows: Avenir's medium on a 1x display. */
	@media (max-resolution: 1.5dppx) {
		.panel-foot .sidebar-label {
			font-weight: 500;
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

	/* The rows' inset: 4px, then each row's 12px, so icons start 16px in.
	   The first door starts 8px down. */
	.panel-body {
		height: 100%;
		min-height: 0;
		overflow-y: auto;
		overflow-x: hidden;
		padding: 8px 4px 12px;
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
