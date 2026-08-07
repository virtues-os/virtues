<script lang="ts">
	/**
	 * The phone shell's viewport: **one window, no panes, no tabs.**
	 *
	 * The desktop shell keeps every open tab mounted and hides the inactive ones
	 * with `display: none`, so a tab's chat stream, Yjs document and sockets
	 * survive being switched away from. That trade is wrong on a phone: nothing
	 * off-screen is reachable (there is no tab strip to reach it with), so the
	 * cost — memory, timers, WebSockets, a WKWebView holding N views' worth of
	 * DOM — buys nothing. Here exactly one view is mounted, and navigating
	 * replaces it.
	 *
	 * Back is therefore the whole navigation model: the single window's own
	 * history stack (the same one the desktop back button walks) — walked by
	 * the left-edge swipe, not by a chevron. There is no back button here on
	 * purpose; the gesture is the affordance, the way it is everywhere else on
	 * the phone. WKWebView's own back gesture is not available to us (see the
	 * swipe block below), so we grow our own.
	 */
	import { onMount } from "svelte";
	import Icon from "$lib/components/Icon.svelte";
	import TabContent from "$lib/components/tabs/TabContent.svelte";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { paneActions } from "$lib/stores/paneActions.svelte";
	import { contextMenu } from "$lib/stores/contextMenu.svelte";

	// The store folds itself to one window at boot; this catches the other way
	// in — a desktop browser dragged below the mobile breakpoint mid-session,
	// which is how this shell gets exercised without a phone.
	onMount(() => windowShellStore.collapseToSingleWindow());

	const tab = $derived(windowShellStore.activeTab);

	// The view's own published actions (item 5's slot). On desktop these ride in
	// the pane toolbar; with no toolbar here, the action bar is where they land.
	const viewActions = $derived(paneActions.for(tab?.id));
	const INLINE_ACTION_LIMIT = 2;
	const inlineActions = $derived(viewActions.slice(0, INLINE_ACTION_LIMIT));
	const overflowActions = $derived(viewActions.slice(INLINE_ACTION_LIMIT));

	// No bar at all unless a view has actually published something — an empty
	// strip is 44px of nothing between the status bar and the view.
	const showBar = $derived(viewActions.length > 0);

	function showActionOverflow(e: MouseEvent) {
		e.stopPropagation();
		const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
		contextMenu.show(
			{ x: rect.right, y: rect.bottom },
			overflowActions.map((a) => ({
				id: a.id,
				label: a.label,
				icon: a.icon,
				disabled: a.disabled,
				checked: a.active,
				action: a.run,
			})),
		);
	}

	// ── Edge-swipe back ──────────────────────────────────────────────────────
	// Hand-rolled because neither layer below us offers it. SvelteKit has no
	// gesture surface at all. wry does have the knob — `back_forward_navigation
	// _gestures` → `setAllowsBackForwardNavigationGestures` — but in 0.55.1 the
	// call site is inside `#[cfg(target_os = "macos")]`, so iOS never gets it,
	// and Tauri 2.11 doesn't expose the attribute to configure anyway. Even
	// wired up it would walk WKWebView's back-forward list rather than this
	// window's history stack, which is the thing we actually mean by "back".
	//
	// Start only from the left edge, and only once the drag has declared itself
	// horizontal — otherwise every vertical scroll that begins near the bezel
	// would fight the gesture.
	const EDGE_PX = 28; // how close to the left edge a touch must start
	const INTENT_PX = 8; // travel before the gesture commits to horizontal
	const COMMIT_PX = 72; // travel that actually goes back on release

	let dragX = $state(0);
	let dragging = $state(false);
	let tracking = false;
	let startX = 0;
	let startY = 0;

	/** Is the touch inside something that scrolls sideways itself? */
	function inHorizontalScroller(target: EventTarget | null): boolean {
		let el = target instanceof Element ? target : null;
		while (el && !el.classList.contains("single-window")) {
			if (el.scrollWidth > el.clientWidth + 2) {
				const ox = getComputedStyle(el).overflowX;
				if (ox === "auto" || ox === "scroll") return true;
			}
			el = el.parentElement;
		}
		return false;
	}

	function onPointerDown(e: PointerEvent) {
		if (e.pointerType === "mouse") return;
		if (!windowShellStore.canGoBack()) return;
		if (e.clientX > EDGE_PX) return;
		// A wide table, a code block, the activity heatmap: dragging one of
		// those sideways is the user scrolling it, not leaving the page. The
		// gutter is only 20px now, so these do reach the swipe zone.
		if (inHorizontalScroller(e.target)) return;
		tracking = true;
		startX = e.clientX;
		startY = e.clientY;
	}

	function onPointerMove(e: PointerEvent) {
		if (!tracking) return;
		const dx = e.clientX - startX;
		const dy = e.clientY - startY;
		if (!dragging) {
			if (Math.abs(dy) > Math.abs(dx)) {
				tracking = false; // vertical: it's a scroll, let it go
				return;
			}
			if (dx < INTENT_PX) return;
			dragging = true;
		}
		dragX = Math.max(0, dx);
	}

	function endDrag() {
		if (!tracking) return;
		const commit = dragging && dragX > COMMIT_PX;
		tracking = false;
		dragging = false;
		dragX = 0;
		if (commit) windowShellStore.goBack();
	}
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
	class="single-window"
	onpointerdown={onPointerDown}
	onpointermove={onPointerMove}
	onpointerup={endDrag}
	onpointercancel={endDrag}
>
	{#if showBar}
		<div class="action-bar">
			{#each inlineActions as action (action.id)}
				<button
					class="action"
					class:primary={action.primary}
					class:toggled={action.active}
					aria-pressed={action.active !== undefined ? action.active : undefined}
					disabled={action.disabled}
					onclick={action.run}
					aria-label={action.label}
				>
					<Icon icon={action.icon} width="20" />
				</button>
			{/each}

			{#if overflowActions.length > 0}
				<button class="action" onclick={showActionOverflow} aria-label="More actions">
					<Icon icon="ri:more-line" width="20" />
				</button>
			{/if}
		</div>
	{/if}

	<!-- One view. Not one of many with the rest hidden — one. -->
	<div
		class="viewport"
		class:dragging
		style:transform={dragX ? `translateX(${dragX}px)` : undefined}
	>
		{#if tab}
			<TabContent {tab} active={true} />
		{/if}
	</div>
</div>

<style>
	/* Not `.mobile-shell` — the app shell in `(app)/+layout.svelte` already
	   owns that name, and two elements answering to it is a trap for anything
	   reaching in from outside Svelte's scoped styles. */
	.single-window {
		display: flex;
		flex-direction: column;
		flex: 1;
		min-height: 0;
		position: relative;
		overflow: hidden;
	}

	/* Slim, quiet, right-aligned, and only present when a view has published
	   something into it. */
	.action-bar {
		display: flex;
		align-items: center;
		justify-content: flex-end;
		gap: 2px;
		height: 44px;
		flex-shrink: 0;
		padding: 0 6px;
		background: var(--color-surface);
		border-bottom: 0.5px solid
			color-mix(in srgb, var(--color-border) 90%, transparent);
	}

	.action {
		display: flex;
		align-items: center;
		height: 34px;
		padding: 0 8px;
		border: 0;
		border-radius: 8px;
		background: transparent;
		color: var(--color-foreground);
		cursor: pointer;
		-webkit-tap-highlight-color: transparent;
		transition:
			opacity 0.12s ease,
			background-color 0.12s ease;
	}

	.action:active {
		opacity: 0.5;
	}

	.action.toggled {
		background: var(--hover-bg);
	}

	.action:disabled {
		opacity: 0.35;
		cursor: default;
	}

	.viewport {
		position: relative;
		flex: 1;
		min-height: 0;
		overflow: hidden;
	}

	/* The drag follows the finger; the release snaps. `transform` here makes
	   this element a containing block for fixed-position descendants, so it is
	   only ever set while a swipe is in flight. */
	.viewport:not(.dragging) {
		transition: transform 0.22s cubic-bezier(0.32, 0.72, 0, 1);
	}
</style>
