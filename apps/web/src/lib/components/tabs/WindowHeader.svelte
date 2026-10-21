<script lang="ts">
	/**
	 * The window header: one row across the whole window, on the desk ground
	 * above the cards.
	 *
	 * Over the rail it holds the ∴ mark (the inbox). Over the sidebar it holds
	 * the window's own controls, in a place that never moves: back, forward,
	 * the sidebar toggle and new chat. Back, forward and new chat act on the
	 * focused pane. Right-clicking the sidebar toggle shows or hides the
	 * rail's labels. Over the cards it holds each pane's tab strip, lined up
	 * with the pane below it, so a tab always sits above its own content.
	 *
	 * With the sidebar closed the controls stay where they were, past the
	 * card's edge, so the left pane's tabs start just after them.
	 */
	import AtlasIcon from '$lib/components/sidebar/AtlasIcon.svelte';
	import InboxMark from '$lib/components/inbox/InboxMark.svelte';
	import WindowTabBar from './WindowTabBar.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { sidebarState } from '$lib/stores/sidebarState.svelte';
	import { shortcuts } from '$lib/shortcuts/registry.svelte';

	/** Air between the controls and the first tab when they meet. */
	const LEAD_GAP_PX = 8;
	/**
	 * The controls' glyphs are the rail's (Atlas), at the rail's line: the
	 * rail draws 20px glyphs at stroke 1.0 on a 16-unit box, 1.25px of ink,
	 * so 16px glyphs here take 1.25 to match it exactly.
	 */
	const GLYPH_PX = 16;
	const GLYPH_STROKE = 1.25;
	/** InboxMark's width; the mark is centred over the rail's tile column. */
	const MARK_PX = 32;

	const isSplit = $derived(windowShellStore.isSplit);
	const leftWidth = $derived(isSplit ? windowShellStore.leftPane?.width || 50 : 100);
	const rightWidth = $derived(isSplit ? windowShellStore.rightPane?.width || 50 : 0);

	const cardLeft = $derived(sidebarState.cardLeft);
	// The rail pads 12px on its left and none on its right; its tiles fill
	// the rest.
	const markInset = $derived(12 + (sidebarState.railWidth - 12 - MARK_PX) / 2);

	/**
	 * Where the sidebar's icons start: past the rail, its 12px of desk, the
	 * panel card's 1px border, the panel body's 4px and a row's 12px (the
	 * Home title starts there too). The back arrow's glyph starts on that
	 * line, so the history controls read as the sidebar's own top row. They
	 * keep that place with the sidebar closed: controls that slid toward the
	 * mark would move under the pointer that just closed it.
	 */
	const PANEL_ICON_INSET_PX = 1 + 4 + 12;
	/** A 32px control centres its 16px glyph, 8px in from its edge. */
	const GLYPH_INSET_PX = (32 - GLYPH_PX) / 2;
	const controlsLeft = $derived(sidebarState.railWidth + 12 + PANEL_ICON_INSET_PX - GLYPH_INSET_PX);

	let controlsWidth = $state(0);
	const overlap = $derived(Math.max(0, controlsLeft + controlsWidth + LEAD_GAP_PX - cardLeft));

	const canGoBack = $derived(windowShellStore.canGoBack());
	const canGoForward = $derived(windowShellStore.canGoForward());

	const sidebarOpen = $derived(!sidebarState.collapsed);
	const compact = $derived(sidebarState.railCompact);

	function toggleLabels(e: MouseEvent) {
		// The browser's own context menu has nothing to offer here.
		e.preventDefault();
		sidebarState.toggleRailCompact();
	}
</script>

<header
	class="window-header"
	class:resizing={sidebarState.resizing}
	style:--card-left="{cardLeft}px"
>
	<!-- The mark sits over the rail's column, the controls over the sidebar's:
	     each group heads the column it belongs to. Within the controls, air
	     alone sets history (back, forward) apart from the window (sidebar,
	     new chat). -->
	<div class="lead mark-slot" style:left="{markInset}px">
		<InboxMark />
	</div>
	<div class="lead" style:left="{controlsLeft}px" bind:clientWidth={controlsWidth}>
		<button
			class="lead-btn"
			onclick={() => windowShellStore.goBack()}
			disabled={!canGoBack}
			aria-label="Back"
			title="Back"
		>
			<AtlasIcon name="back" size={GLYPH_PX} stroke={GLYPH_STROKE} bare />
		</button>
		<button
			class="lead-btn"
			onclick={() => windowShellStore.goForward()}
			disabled={!canGoForward}
			aria-label="Forward"
			title="Forward"
		>
			<AtlasIcon name="forward" size={GLYPH_PX} stroke={GLYPH_STROKE} bare />
		</button>
		<span class="lead-gap" aria-hidden="true"></span>
		<button
			class="lead-btn"
			onclick={() => sidebarState.toggle()}
			oncontextmenu={toggleLabels}
			aria-label={sidebarOpen ? 'Hide the sidebar' : 'Show the sidebar'}
			aria-expanded={sidebarOpen}
			title={`${sidebarOpen ? 'Hide' : 'Show'} the sidebar (${shortcuts.format('mod+b')}) · Right-click to ${compact ? 'show' : 'hide'} labels`}
		>
			<AtlasIcon name={sidebarOpen ? 'sidebar-open' : 'sidebar'} size={GLYPH_PX} stroke={GLYPH_STROKE} bare />
		</button>
		<button
			class="lead-btn"
			onclick={() => windowShellStore.newChat()}
			aria-label="New chat"
			title={`New chat (${shortcuts.format('mod+n')})`}
		>
			<AtlasIcon name="new-chat" size={GLYPH_PX} stroke={GLYPH_STROKE} bare />
		</button>
	</div>

	<div class="strips">
		<div
			class="strip"
			style:width={isSplit ? `calc(${leftWidth}% - 0.5px)` : '100%'}
			style:padding-left="{overlap}px"
		>
			<WindowTabBar paneId={isSplit ? 'left' : undefined} />
		</div>
		{#if isSplit}
			<div class="gutter" aria-hidden="true"></div>
			<div class="strip" style:width="calc({rightWidth}% - 0.5px)">
				<WindowTabBar paneId="right" />
			</div>
		{/if}
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
		transition: left 300ms var(--ease-premium);
	}

	/* The strips span exactly the cards below them: from the card's left edge
	   to its 12px right margin. They move on the sidebar's own clock, like the
	   pane does, so a tab never slides out from over its content. */
	.strips {
		position: absolute;
		top: 0;
		bottom: 0;
		left: var(--card-left);
		right: 12px;
		display: flex;
		transition: left 300ms var(--ease-premium);
	}

	.strip {
		min-width: 0;
		transition: padding-left 300ms var(--ease-premium);
	}

	.resizing .strips,
	.resizing .strip,
	.resizing .lead {
		transition: none;
	}

	.gutter {
		width: 1px;
		flex: none;
	}

	.lead-gap {
		width: 8px;
		flex: none;
	}

	/* 32px squares on the 8pt grid, the same box as the mark, butted
	   together: the 16px glyphs leave 16px of air between neighbours, so no
	   gap is needed inside a group. */
	.lead-btn {
		display: grid;
		place-items: center;
		width: 32px;
		height: 32px;
		padding: 0;
		border: none;
		border-radius: 6px;
		background: transparent;
		color: var(--color-foreground-muted);
		cursor: pointer;
		flex-shrink: 0;
		transition:
			background-color 150ms ease,
			color 150ms ease;
	}

	.lead-btn:hover:not(:disabled) {
		background: var(--hover-bg);
		color: var(--color-foreground);
	}

	.lead-btn:active:not(:disabled) {
		background: var(--press-bg);
	}

	/* Present but spent: the history arrows stay put at the ends of the
	   stack so the cluster never shifts. */
	.lead-btn:disabled {
		opacity: 0.35;
		cursor: default;
	}

	.lead-btn:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: -2px;
		color: var(--color-foreground);
	}

	@media (prefers-reduced-motion: reduce) {
		.lead,
		.lead-btn,
		.strips,
		.strip {
			transition: none;
		}
	}
</style>
