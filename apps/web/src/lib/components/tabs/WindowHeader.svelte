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
	import { shortcuts } from '$lib/shortcuts/registry.svelte';

	/** Air between the controls and the first tab when they meet. */
	const LEAD_GAP_PX = 8;
	/** The controls' glyphs: Atlas at row size, which draws them at the
	 *  scale's one line weight (1.25px), the same as the sidebar's rows. */
	const GLYPH_PX = 16;
	/** The scale's control box: every clickable thing in the header. */
	const CONTROL_PX = 28;
	/** The ∴ glyph inside InboxMark's control box. */
	const MARK_GLYPH_PX = 17;

	const isSplit = $derived(windowShellStore.isSplit);
	const leftWidth = $derived(isSplit ? windowShellStore.leftPane?.width || 50 : 100);
	const rightWidth = $derived(isSplit ? windowShellStore.rightPane?.width || 50 : 0);

	const cardLeft = $derived(sidebarState.cardLeft);

	/**
	 * Where the sidebar's icons start: the desk's 12px, the card's 1px
	 * border, the panel body's 4px and a row's 12px. The ∴'s glyph starts on
	 * that line, so the header's controls head the sidebar's own column. They
	 * keep that place with the sidebar closed: controls that slid would move
	 * under the pointer that just closed it.
	 */
	const ICON_LINE_PX = SIDEBAR_INSET + 1 + 4 + 12;
	const leadLeft = ICON_LINE_PX - (CONTROL_PX - MARK_GLYPH_PX) / 2;

	let leadWidth = $state(0);
	const overlap = $derived(Math.max(0, leadLeft + leadWidth + LEAD_GAP_PX - cardLeft));

	const canGoBack = $derived(windowShellStore.canGoBack());
	const canGoForward = $derived(windowShellStore.canGoForward());

	const sidebarOpen = $derived(!sidebarState.collapsed);
</script>

<header
	class="window-header"
	class:resizing={sidebarState.resizing}
	style:--card-left="{cardLeft}px"
>
	<!-- Three groups set apart by air alone: the mark, history (back,
	     forward) and the window (sidebar, new chat). -->
	<div class="lead" style:left="{leadLeft}px" bind:clientWidth={leadWidth}>
		<InboxMark />
		<span class="lead-gap" aria-hidden="true"></span>
		<button
			class="lead-btn"
			onclick={() => windowShellStore.goBack()}
			disabled={!canGoBack}
			aria-label="Back"
			title="Back"
		>
			<AtlasIcon name="back" size={GLYPH_PX} bare />
		</button>
		<button
			class="lead-btn"
			onclick={() => windowShellStore.goForward()}
			disabled={!canGoForward}
			aria-label="Forward"
			title="Forward"
		>
			<AtlasIcon name="forward" size={GLYPH_PX} bare />
		</button>
		<span class="lead-gap" aria-hidden="true"></span>
		<button
			class="lead-btn"
			class:sidebar-shown={sidebarOpen}
			onclick={() => sidebarState.toggle()}
			aria-label={sidebarOpen ? 'Hide the sidebar' : 'Show the sidebar'}
			aria-expanded={sidebarOpen}
			title={`${sidebarOpen ? 'Hide' : 'Show'} the sidebar (${shortcuts.format('mod+b')})`}
		>
			<AtlasIcon name="sidebar" size={GLYPH_PX} bare />
		</button>
		<button
			class="lead-btn"
			onclick={() => windowShellStore.newChat()}
			aria-label="New chat"
			title={`New chat (${shortcuts.format('mod+n')})`}
		>
			<AtlasIcon name="new-chat" size={GLYPH_PX} bare />
		</button>
	</div>

	<div class="strips">
		<div
			class="strip"
			style:width={isSplit ? `calc(${leftWidth}% - ${SPLIT_GUTTER_PX / 2}px)` : '100%'}
			style:padding-left="{overlap}px"
		>
			<WindowTabBar paneId={isSplit ? 'left' : undefined} />
		</div>
		{#if isSplit}
			<div class="gutter" style:width="{SPLIT_GUTTER_PX}px" aria-hidden="true"></div>
			<div class="strip" style:width="calc({rightWidth}% - {SPLIT_GUTTER_PX / 2}px)">
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
		transition: left var(--motion-layout) var(--ease-premium);
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
		transition: left var(--motion-layout) var(--ease-premium);
	}

	.strip {
		min-width: 0;
		transition: padding-left var(--motion-layout) var(--ease-premium);
	}

	.resizing .strips,
	.resizing .strip,
	.resizing .lead {
		transition: none;
	}

	.gutter {
		flex: none;
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
		border-radius: 6px;
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
