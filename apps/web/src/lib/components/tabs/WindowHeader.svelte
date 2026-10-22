<script lang="ts">
	/**
	 * The window header: one row across the whole window, on the desk ground
	 * above the cards.
	 *
	 * Over the sidebar it holds the ∴ mark (the inbox) and the window's own
	 * controls, in a place that never moves: back, forward, the sidebar
	 * toggle and new chat. Back, forward and new chat act on the focused
	 * pane. Over the cards it holds each pane's tab strip, lined up with the
	 * pane below it, so a tab always sits on top of its own content.
	 *
	 * With the sidebar closed the controls stay where they were, so the left
	 * pane's tabs start just after them.
	 */
	import AtlasIcon from '$lib/components/sidebar/AtlasIcon.svelte';
	import InboxMark from '$lib/components/inbox/InboxMark.svelte';
	import WindowTabBar from './WindowTabBar.svelte';
	import { windowShellStore, SPLIT_GUTTER_PX } from '$lib/stores/window-shell.svelte';
	import { sidebarState, SIDEBAR_INSET } from '$lib/stores/sidebarState.svelte';
	import { isMacOS } from '$lib/utils/platform';
	import { SHELL_GLYPH_PX } from '$lib/components/sidebar/AtlasIcon.svelte';

	/** Air between the controls and the first tab when they meet. */
	const LEAD_GAP_PX = 8;
	/** The controls' glyphs, the same size as the sidebar's rows. */
	const GLYPH_PX = SHELL_GLYPH_PX;
	/** The scale's control box: every clickable thing in the header. */
	const CONTROL_PX = 28;
	/** The ∴ glyph inside InboxMark's control box. */
	const MARK_GLYPH_PX = 17;

	const isSplit = $derived(windowShellStore.isSplit);
	const leftWidth = $derived(isSplit ? windowShellStore.leftPane?.width || 50 : 100);
	const rightWidth = $derived(isSplit ? windowShellStore.rightPane?.width || 50 : 0);
	const gutterPx = $derived(isSplit ? SPLIT_GUTTER_PX : 0);

	const cardLeft = $derived(sidebarState.cardLeft);

	/**
	 * Where the sidebar's icons start: the desk gap, the panel body's 4px
	 * and a row's 12px. The ∴'s glyph starts on
	 * that line, so the header's controls head the sidebar's own column. They
	 * keep that place with the sidebar closed: controls that slid would move
	 * under the pointer that just closed it.
	 */
	const ICON_LINE_PX = SIDEBAR_INSET + 4 + 12;
	/**
	 * In the Mac app the title bar is an overlay (src-tauri main.rs) and the
	 * traffic lights sit in this row, from 16pt to 70pt: three 14pt buttons
	 * 20pt apart. The mark and the controls follow them after 12pt of air,
	 * as in a native Mac app, rather than on the sidebar's icon line.
	 */
	const TRAFFIC_LIGHTS_END_PX = 16 + 2 * 20 + 14;
	const leadLeft = isMacOS
		? TRAFFIC_LIGHTS_END_PX + 12
		: ICON_LINE_PX - (CONTROL_PX - MARK_GLYPH_PX) / 2;

	let leadWidth = $state(0);
	const overlap = $derived(Math.max(0, leadLeft + leadWidth + LEAD_GAP_PX - cardLeft));

	const canGoBack = $derived(windowShellStore.canGoBack());
	const canGoForward = $derived(windowShellStore.canGoForward());

	const sidebarOpen = $derived(!sidebarState.collapsed);
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
	<!-- Three groups set apart by air alone: the mark, history (back,
	     forward) and the window (sidebar, new chat). -->
	<div class="lead" data-tauri-drag-region style:left="{leadLeft}px" bind:clientWidth={leadWidth}>
		<InboxMark />
		<span class="lead-gap" data-tauri-drag-region aria-hidden="true"></span>
		<button
			class="lead-btn"
			onclick={() => windowShellStore.goBack()}
			disabled={!canGoBack}
			aria-label="Back"
			data-tip="Back"
		>
			<AtlasIcon name="back" size={GLYPH_PX} bare />
		</button>
		<button
			class="lead-btn"
			onclick={() => windowShellStore.goForward()}
			disabled={!canGoForward}
			aria-label="Forward"
			data-tip="Forward"
		>
			<AtlasIcon name="forward" size={GLYPH_PX} bare />
		</button>
		<span class="lead-gap" data-tauri-drag-region aria-hidden="true"></span>
		<button
			class="lead-btn"
			class:sidebar-shown={sidebarOpen}
			onclick={() => sidebarState.toggle()}
			aria-label={sidebarOpen ? 'Hide the sidebar' : 'Show the sidebar'}
			aria-expanded={sidebarOpen}
			data-tip={`${sidebarOpen ? 'Hide' : 'Show'} the sidebar`}
			data-tip-key="mod+b"
		>
			<AtlasIcon name="sidebar" size={GLYPH_PX} bare />
		</button>
		<!-- Only while the sidebar is hidden: open, its own New chat row sits
		     just below, and two doors to one place read as two places. -->
		{#if !sidebarOpen}
			<button
				class="lead-btn"
				onclick={() => windowShellStore.newChat()}
				aria-label="New chat"
				data-tip="New chat"
				data-tip-key="mod+n"
			>
				<AtlasIcon name="new-chat" size={GLYPH_PX} bare />
			</button>
		{/if}
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
		transition: left var(--motion-layout) var(--ease-premium);
	}

	/* The strips span exactly the cards below them: from the card's left edge
	   to its right margin (the desk gap). They move on the sidebar's own clock, like the
	   pane does, so a tab never slides out from over its content. */
	.strips {
		position: absolute;
		top: 0;
		bottom: 0;
		left: var(--card-left);
		right: var(--pane-inset);
		display: flex;
		transition: left var(--motion-layout) var(--ease-premium);
	}

	/* The strips open and close with the cards below them (SplitContainer),
	   on the same clock, so a tab never runs ahead of its pane. */
	.strip {
		min-width: 0;
		transition:
			padding-left var(--motion-layout) var(--ease-premium),
			width var(--motion-layout) var(--ease-premium);
	}

	.resizing .strips,
	.resizing .strip,
	.resizing .gutter,
	.resizing .lead {
		transition: none;
	}

	.gutter {
		flex: none;
		transition: width var(--motion-layout) var(--ease-premium);
	}

	.lead-gap {
		width: 8px;
		flex: none;
	}

	/* The scale's 28px control (design.md, "The scale"), the same box as the
	   mark and the tabs, butted together: the 16px glyphs leave 12px of air
	   between neighbours, so no gap is needed inside a group. Motion is the
	   scale's too: hover in fast and out slow, and a press that sinks to 94%
	   and springs back. */
	.lead-btn {
		display: grid;
		place-items: center;
		width: 28px;
		height: 28px;
		padding: 0;
		border: none;
		border-radius: var(--radius-md);
		background: transparent;
		color: var(--color-foreground-muted);
		cursor: pointer;
		flex-shrink: 0;
		transition:
			background-color var(--motion-hover-out) var(--ease-premium),
			color var(--motion-hover-out) var(--ease-premium),
			opacity var(--motion-hover-out) var(--ease-premium),
			transform var(--motion-release) var(--ease-premium);
	}

	.lead-btn:hover:not(:disabled) {
		background: var(--hover-bg);
		color: var(--color-foreground);
		transition-duration: var(--motion-hover-in), var(--motion-hover-in), var(--motion-hover-in), var(--motion-release);
	}

	.lead-btn:active:not(:disabled) {
		background: var(--press-bg);
		transform: scale(0.94);
		transition-duration: var(--motion-press);
	}

	/* The sidebar glyph's column fills while the sidebar is shown, on the
	   sidebar's own layout timing, so the glyph and the panel arrive
	   together. */
	.lead-btn :global(.atlas-fill) {
		transition: fill-opacity var(--motion-layout) var(--ease-premium);
	}

	.lead-btn.sidebar-shown :global(.atlas-fill) {
		fill-opacity: 0.22;
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
		.lead-btn :global(.atlas-fill),
		.strips,
		.strip {
			transition: none;
		}

		.lead-btn:active:not(:disabled) {
			transform: none;
		}
	}
</style>
