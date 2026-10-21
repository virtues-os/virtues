<script lang="ts">
	/**
	 * The window header: one row across the whole window, on the desk ground
	 * above the cards.
	 *
	 * Over the rail and the sidebar it holds the ∴ mark and the window's own
	 * controls, in a place that never moves: back, forward, the sidebar
	 * toggle and new chat. Back, forward and new chat act on the focused
	 * pane. Right-clicking the sidebar toggle shows or hides the rail's labels. Over
	 * the cards it holds each pane's tab strip, lined up with the pane below
	 * it, so a tab always sits above its own content.
	 *
	 * With the sidebar closed the controls are wider than the rail, so the
	 * left pane's tabs start just after them instead of at the card's edge.
	 */
	import Icon from '$lib/components/Icon.svelte';
	import HeaderMark from '$lib/components/sidebar/HeaderMark.svelte';
	import WindowTabBar from './WindowTabBar.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { sidebarState } from '$lib/stores/sidebarState.svelte';
	import { shortcuts } from '$lib/shortcuts/registry.svelte';

	/** Air between the controls and the first tab when they meet. */
	const LEAD_GAP_PX = 8;
	/** HeaderMark's width; the mark is centred over the rail's tile column. */
	const MARK_PX = 32;

	const isSplit = $derived(windowShellStore.isSplit);
	const leftWidth = $derived(isSplit ? windowShellStore.leftPane?.width || 50 : 100);
	const rightWidth = $derived(isSplit ? windowShellStore.rightPane?.width || 50 : 0);

	const cardLeft = $derived(sidebarState.cardLeft);
	// The rail pads 12px on its left and none on its right; its tiles fill
	// the rest.
	const markInset = $derived(12 + (sidebarState.railWidth - 12 - MARK_PX) / 2);

	let leadWidth = $state(0);
	const overlap = $derived(Math.max(0, leadWidth + LEAD_GAP_PX - cardLeft));

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
	<div class="lead" style:padding-left="{markInset}px" bind:clientWidth={leadWidth}>
		<HeaderMark />
		<button
			class="lead-btn"
			onclick={() => windowShellStore.goBack()}
			disabled={!canGoBack}
			aria-label="Back"
			title="Back"
		>
			<Icon icon="ri:arrow-left-s-line" />
		</button>
		<button
			class="lead-btn"
			onclick={() => windowShellStore.goForward()}
			disabled={!canGoForward}
			aria-label="Forward"
			title="Forward"
		>
			<Icon icon="ri:arrow-right-s-line" />
		</button>
		<button
			class="lead-btn"
			onclick={() => sidebarState.toggle()}
			oncontextmenu={toggleLabels}
			aria-label={sidebarOpen ? 'Hide the sidebar' : 'Show the sidebar'}
			aria-expanded={sidebarOpen}
			title={`${sidebarOpen ? 'Hide' : 'Show'} the sidebar (${shortcuts.format('mod+b')}) · Right-click to ${compact ? 'show' : 'hide'} labels`}
		>
			<Icon icon={sidebarOpen ? 'ri:sidebar-fold-line' : 'ri:sidebar-unfold-line'} />
		</button>
		<button
			class="lead-btn"
			onclick={() => windowShellStore.newChat()}
			aria-label="New chat"
			title={`New chat (${shortcuts.format('mod+n')})`}
		>
			<Icon icon="ri:edit-box-line" />
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
		flex: none;
		height: var(--chrome-row-h);
	}

	.lead {
		position: absolute;
		top: 0;
		bottom: 0;
		left: 0;
		z-index: 1;
		display: flex;
		align-items: center;
		gap: 2px;
		transition: padding-left 300ms var(--ease-premium);
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

	.lead-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 26px;
		height: 26px;
		padding: 0;
		border: none;
		border-radius: 6px;
		background: transparent;
		color: var(--color-foreground-muted);
		font-size: 16px;
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

	.lead-btn:disabled {
		opacity: 0.3;
		cursor: default;
	}

	.lead-btn:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: -2px;
		color: var(--color-foreground);
	}

	@media (prefers-reduced-motion: reduce) {
		.lead,
		.strips,
		.strip {
			transition: none;
		}
	}
</style>
