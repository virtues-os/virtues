<script lang="ts">
	/**
	 * The window header: one row across the whole window, over the painting.
	 *
	 * At the left is the ∴, a glass button that shows and hides the sidebar
	 * (HeaderMark), standing directly over the rail. In the Mac app, where
	 * the traffic lights take the row's start, it sits at the open sidebar's
	 * right end and beside the lights when closed. While the sidebar is open,
	 * Search (⌘K) stands at the right end of the sidebar's first row, on its
	 * glass; closed, ⌘K is the way in. Settings stands at the sidebar's foot
	 * (UnifiedSidebar). Over the panes the row holds each pane's tab strip,
	 * lined up with the pane below it, so a tab always sits on top of its own
	 * content. Back and forward are on each page's action toolbar
	 * (ActionToolbar), since they act on that page's tab.
	 */
	import HeaderMark from './HeaderMark.svelte';
	import WindowTabBar from './WindowTabBar.svelte';
	import AtlasIcon, { SHELL_GLYPH_PX } from '$lib/components/sidebar/AtlasIcon.svelte';
	import { search } from '$lib/stores/search.svelte';
	import { windowShellStore, SPLIT_GUTTER_PX } from '$lib/stores/window-shell.svelte';
	import { sidebarState, SIDEBAR_INSET, RAIL_GLASS_PX, CHROME_TAB_PX } from '$lib/stores/sidebarState.svelte';
	import { trafficLights } from '$lib/tauri/trafficLights.svelte';

	/** Air between the ∴ and the first tab when they meet. */
	const LEAD_GAP_PX = 8;
	/** Between the last traffic light and the ∴. */
	const LIGHTS_GAP_PX = 12;
	/** How far a sidebar row stands in from the glass's edge, open or closed (UnifiedSidebar). */
	const ROW_INSET_PX = 4;
	/** Between the ∴ and Search, on the sidebar's first row. */
	const SEARCH_GAP_PX = 4;
	/** The room Search takes at the end of the open sidebar's first row. */
	const SEARCH_ROOM_PX = CHROME_TAB_PX + SEARCH_GAP_PX;

	const isSplit = $derived(windowShellStore.isSplit);
	const leftWidth = $derived(isSplit ? windowShellStore.leftPane?.width || 50 : 100);
	const rightWidth = $derived(isSplit ? windowShellStore.rightPane?.width || 50 : 0);
	const gutterPx = $derived(isSplit ? SPLIT_GUTTER_PX : 0);

	const cardLeft = $derived(sidebarState.cardLeft);

	/**
	 * The ∴ is the sidebar's first line, on its glass. Closed, it is the
	 * top of the rail, as wide as a rail row. Open, it is a row as wide as
	 * the sidebar's rows, standing where they stand.
	 *
	 * In the Mac app the title bar is an overlay and the traffic lights sit
	 * in this row, on its centre line (src-tauri traffic_lights.rs), inside
	 * the open sidebar's glass. The ∴ then stands alone at the right end of
	 * that row, as a square the size of the corner buttons, without the
	 * name. Closed, the rail cannot hold the lights, so the ∴ follows them
	 * on its own glass. In full screen macOS hides the lights, and the ∴
	 * goes back to the sidebar's first row, as in a browser.
	 */
	const sidebarOpen = $derived(!sidebarState.collapsed);
	const besideLights = $derived(trafficLights.shown);
	const leadLeft = $derived(
		besideLights
			? sidebarOpen
				? SIDEBAR_INSET + sidebarState.width - ROW_INSET_PX - CHROME_TAB_PX - SEARCH_ROOM_PX
				: trafficLights.end + LIGHTS_GAP_PX
			: SIDEBAR_INSET + ROW_INSET_PX
	);
	const markWidth = $derived(
		besideLights
			? CHROME_TAB_PX
			: sidebarOpen
				? sidebarState.width - 2 * ROW_INSET_PX - SEARCH_ROOM_PX
				: RAIL_GLASS_PX - 2 * ROW_INSET_PX
	);

	let leadWidth = $state(0);
	const overlap = $derived(Math.max(0, leadLeft + leadWidth + LEAD_GAP_PX - cardLeft));

</script>

<!-- The empty parts of the row move the window in the Mac app: the
     attribute marks an element, not its children, so each layer that can be
     the one under the pointer carries it. Buttons and tabs do not. -->
<header
	data-tauri-drag-region
	class="window-header"
	class:resizing={sidebarState.resizing || windowShellStore.paneResizing}
	style:--card-left="{cardLeft}px"
>
	<div
		class="lead"
		data-tauri-drag-region
		style:left="{leadLeft}px"
		bind:clientWidth={leadWidth}
	>
		<HeaderMark width={markWidth} standalone={besideLights && !sidebarOpen} named={!besideLights} />
		<!-- Search is part of the open sidebar. It folds away with it, staying
		     mounted so it can fade on the sidebar's clock. -->
		<button
			type="button"
			class="sidebar-search"
			class:shown={sidebarOpen}
			style:--search-gap="{SEARCH_GAP_PX}px"
			onclick={() => search.show()}
			aria-label="Search"
			aria-hidden={!sidebarOpen}
			tabindex={sidebarOpen ? 0 : -1}
			data-tip="Search"
			data-tip-key="mod+k"
		>
			<AtlasIcon name="search" size={SHELL_GLYPH_PX} bare />
		</button>
	</div>

	<div class="strips" data-tauri-drag-region>
		<div
			data-tauri-drag-region
			class="strip"
			style:width="calc({leftWidth}% - {gutterPx / 2}px)"
			style:padding-left="{overlap}px"
		>
			<WindowTabBar paneId={isSplit ? 'left' : undefined} />
		</div>
		<!-- The gutter and the right strip stay mounted at zero width with one
		     pane, so they open and close on the cards' clock. -->
		<div class="gutter" style:width="{gutterPx}px" aria-hidden="true"></div>
		<div class="strip" data-tauri-drag-region style:width="calc({rightWidth}% - {gutterPx / 2}px)">
			{#if isSplit}
				<WindowTabBar paneId="right" />
			{/if}
		</div>
	</div>
</header>

<style>
	.window-header {
		position: relative;
		/* Above the sidebar and the cards, which come later in the page and
		   would otherwise paint over what opens from here (the inbox, a
		   tab's menu). */
		z-index: var(--z-overlay);
		flex: none;
		height: var(--chrome-row-h);
	}

	.lead {
		position: absolute;
		top: 0;
		bottom: 0;
		z-index: 1;
		display: flex;
		align-items: center;
		transition: left var(--motion-sidebar) var(--ease-sidebar);
	}

	/* The strips span exactly the panes below them: from the pane's left edge
	   to the window's right edge. They move on the sidebar's own clock, like the
	   pane does, so a tab never slides out from over its content. */
	.strips {
		position: absolute;
		top: 0;
		bottom: 0;
		left: var(--card-left);
		right: var(--window-air);
		display: flex;
		transition: left var(--motion-sidebar) var(--ease-sidebar);
	}

	/* The strips open and close with the cards below them (SplitContainer),
	   on the same clock, so a tab never runs ahead of its pane. */
	.strip {
		min-width: 0;
		/* The left padding clears the header's controls, which only changes
		   as the sidebar opens or closes, so it moves on the sidebar's clock. */
		transition:
			padding-left var(--motion-sidebar) var(--ease-sidebar),
			width var(--motion-layout) var(--ease-premium);
	}

	/* Search: a square the size of a tab at the end of the open sidebar's
	   first row. It has no glass of its own, since it sits on the sidebar's,
	   and it tints under the pointer as a row's own button does. Closed, it
	   has no width, so the ∴ stands alone on the rail. */
	.sidebar-search {
		flex: none;
		width: 0;
		height: var(--chrome-tab-h);
		margin-left: 0;
		padding: 0;
		overflow: hidden;
		border: none;
		border-radius: var(--sidebar-interactive-radius);
		background: transparent;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		color: var(--color-foreground-muted);
		cursor: pointer;
		opacity: 0;
		pointer-events: none;
		transition:
			width var(--motion-sidebar) var(--ease-sidebar),
			margin-left var(--motion-sidebar) var(--ease-sidebar),
			opacity 100ms ease-in,
			background-color var(--motion-hover-out) var(--ease-premium),
			color var(--motion-hover-out) var(--ease-premium);
	}

	.sidebar-search.shown {
		width: var(--chrome-tab-h);
		margin-left: var(--search-gap);
		opacity: 1;
		pointer-events: auto;
		/* In with the sidebar's words, after the glass has partly opened. */
		transition:
			width var(--motion-sidebar) var(--ease-sidebar),
			margin-left var(--motion-sidebar) var(--ease-sidebar),
			opacity 240ms var(--ease-premium) 130ms,
			background-color var(--motion-hover-out) var(--ease-premium),
			color var(--motion-hover-out) var(--ease-premium);
	}

	.sidebar-search.shown:hover {
		color: var(--color-foreground);
		background: var(--hover-bg);
		transition-duration: var(--motion-sidebar), var(--motion-sidebar), 240ms, var(--motion-hover-in), var(--motion-hover-in);
	}

	.resizing .strips,
	.resizing .strip,
	.resizing .gutter,
	.resizing .lead,
	.resizing .sidebar-search {
		transition: none;
	}

	.gutter {
		flex: none;
		transition: width var(--motion-layout) var(--ease-premium);
	}

	@media (prefers-reduced-motion: reduce) {
		.lead,
		.strips,
		.strip,
		.sidebar-search {
			transition: none;
		}
	}
</style>
