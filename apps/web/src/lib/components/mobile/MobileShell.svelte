<script lang="ts">
	/**
	 * The phone shell's viewport: **one window, chat-first, drawer-only nav.**
	 *
	 * The desktop shell keeps every open tab mounted and hides the inactive ones
	 * with `display: none`, so a tab's chat stream, Yjs document and sockets
	 * survive being switched away from. That trade is wrong on a phone: nothing
	 * off-screen is reachable, so the cost — memory, timers, WebSockets, a
	 * WKWebView holding N views' worth of DOM — buys nothing. Here exactly one
	 * view is mounted, and navigating replaces it.
	 *
	 * Navigation is the drawer, and only the drawer. The app opens on chat;
	 * every other surface (a past conversation, This device, Settings) is one
	 * drawer away, and each of those is itself a root — there is no hierarchy,
	 * so there is no back. The left-edge swipe that used to walk the history
	 * stack now opens the drawer instead: with lateral navigation the stack is
	 * vestigial, and one gesture carrying two meanings depending on depth is
	 * exactly the indeterminism this shell exists to avoid.
	 *
	 * The drawer is a FULL-SCREEN takeover, not a partial panel. The chat
	 * slides all the way off to the right and the drawer stands alone — which
	 * is why it carries its own masthead and close control (see MobileDrawer)
	 * instead of leaning on a visible sliver of the page behind it. While it is
	 * up, a leftward drag anywhere brings the chat back; the drawer content
	 * makes the same journey at a third of the speed (the parallax below), so
	 * the two planes read as stacked rather than glued.
	 *
	 * Chrome above the view returned, minimally, out of necessity: with no tab
	 * bar and no back gesture, the hamburger is the only exit from any non-chat
	 * view, so it must exist everywhere. It brings the shell's one other verb —
	 * New chat — to the opposite corner, and nothing else.
	 */
	import { onMount } from "svelte";
	import Icon from "$lib/components/Icon.svelte";
	import TabContent from "$lib/components/tabs/TabContent.svelte";
	import MobileDrawer from "$lib/components/mobile/MobileDrawer.svelte";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { mobileLayout } from "$lib/stores/mobileLayout.svelte";
	import { reachability } from "$lib/stores/reachability.svelte";
	// Side-effect import: the keyboard-inset bridge acts entirely through the
	// `--keyboard-inset` custom property, so nothing imports its exports — the
	// shell loads it. (The old tab bar used to, and deleting it silently
	// orphaned the module: the phone shipped with no keyboard inset at all.)
	import "$lib/stores/keyboard.svelte";

	// The store folds itself to one window at boot; this catches the other way
	// in — a desktop browser dragged below the mobile breakpoint mid-session,
	// which is how this shell gets exercised without a phone.
	onMount(() => windowShellStore.collapseToSingleWindow());

	const tab = $derived(windowShellStore.activeTab);
	const open = $derived(mobileLayout.drawerOpen);

	// Full-screen: the travel distance is the viewport's own width.
	let travel = $state(typeof window === "undefined" ? 390 : window.innerWidth);
	onMount(() => {
		const onResize = () => (travel = window.innerWidth);
		onResize();
		window.addEventListener("resize", onResize);
		return () => window.removeEventListener("resize", onResize);
	});

	// ── The drawer gesture ───────────────────────────────────────────────────
	// One tracker serves both directions: from the left edge while closed it
	// opens, from anywhere while open it closes (the drawer is full-screen, so
	// "anywhere" is the drawer — its list scrolls vertically, and the same
	// intent test that kept vertical scrolls from opening keeps them from
	// closing). The offset follows the finger; release commits by position or
	// by flick, whichever the finger declared louder.
	const EDGE_PX = 28; // how close to the left edge an opening touch must start
	const INTENT_PX = 8; // travel before the gesture commits to horizontal
	const FLICK_PX_PER_MS = 0.35; // release speed that overrides position

	let dragX = $state(0); // viewport offset while a drag is in flight
	let dragging = $state(false);
	let tracking = false;
	let startX = 0;
	let startY = 0;
	let startOffset = 0; // where the viewport sat when the touch began
	let lastX = 0;
	let lastT = 0;
	let velocity = 0; // px/ms, rightward positive
	let viewportEl = $state<HTMLElement | null>(null);

	/**
	 * Where the viewport actually IS, not where its state says it is going.
	 * During the settle animation the two differ, and a grab must catch the
	 * plane at its painted position — starting the drag from the resting
	 * offset instead is the visible snap that separates a native drawer from
	 * a web one.
	 */
	function paintedOffset(): number {
		if (!viewportEl) return open ? travel : 0;
		const t = getComputedStyle(viewportEl).transform;
		if (t === "none") return 0;
		return new DOMMatrixReadOnly(t).m41;
	}

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
		const painted = paintedOffset();
		const midFlight = painted > 0.5 && painted < travel - 0.5;
		if (!midFlight && !open) {
			if (e.clientX > EDGE_PX) return;
			// A wide table, a code block, the activity heatmap: dragging one of
			// those sideways is the user scrolling it, not opening the drawer.
			if (inHorizontalScroller(e.target)) return;
		}
		tracking = true;
		startX = e.clientX;
		startY = e.clientY;
		startOffset = midFlight ? painted : open ? travel : 0;
		lastX = e.clientX;
		lastT = e.timeStamp;
		velocity = 0;
		// Mid-settle, the plane is already moving: the grab IS the gesture, so
		// it skips the intent gate and freezes the plane under the finger.
		if (midFlight) {
			dragging = true;
			dragX = painted;
		}
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
			if (Math.abs(dx) < INTENT_PX) return;
			dragging = true;
			dragX = startOffset;
		}
		const dt = e.timeStamp - lastT;
		if (dt > 0) velocity = (e.clientX - lastX) / dt;
		lastX = e.clientX;
		lastT = e.timeStamp;
		dragX = Math.max(0, Math.min(travel, startOffset + dx));
	}

	function endDrag() {
		if (!tracking) return;
		const wasDragging = dragging;
		tracking = false;
		dragging = false;
		if (!wasDragging) return;
		// A flick states a direction; a slow release states a position.
		let shouldOpen: boolean;
		if (Math.abs(velocity) > FLICK_PX_PER_MS) {
			shouldOpen = velocity > 0;
		} else {
			shouldOpen = dragX > travel / 2;
		}
		dragX = 0;
		if (shouldOpen) mobileLayout.openDrawer();
		else mobileLayout.closeDrawer();
	}

	// Resting offset comes from state; a drag in flight overrides it.
	const offset = $derived(dragging ? dragX : open ? travel : 0);

	// Parked (closed, settled) the drawer is INVISIBLE — belt to the z-order's
	// suspenders, so no engine quirk can ever show it bleeding through the
	// chat. Driven by a timer, not a CSS visibility transition: flipping
	// `visibility` in the same frame as the transform cancels the settle
	// animation outright, so the hide waits for the settle in JS instead.
	let parked = $state(!mobileLayout.drawerOpen);
	$effect(() => {
		if (open || dragging) {
			parked = false;
			return;
		}
		const t = setTimeout(() => (parked = true), 360);
		return () => clearTimeout(t);
	});
	// The drawer travels a third of the viewport's journey, arriving at 0 as
	// the viewport clears — the classic under-plane parallax. Negative while
	// anything is still covering it.
	const drawerShift = $derived((offset - travel) * 0.3);

	function newChat() {
		mobileLayout.closeDrawer();
		windowShellStore.openTabFromRoute("/chat", { label: "Chat" });
	}

	// The top-right slot is modal (see ChatChrome in mobileLayout): an empty
	// chat is already new, so the slot carries the temporary-chat toggle there;
	// everywhere else it composes.
	const chrome = $derived(mobileLayout.chatChrome);
	const showGhostToggle = $derived(chrome?.empty === true);

	// ── Scroll-edge hairline ─────────────────────────────────────────────────
	// The bar is transparent chrome over the view; the hairline appears only
	// once content has actually passed beneath it, so a page at rest keeps its
	// clean top. Every view owns its own scroller and scroll doesn't bubble,
	// so this is a capture-phase listener on window (the old tab bar's trick),
	// filtered to scrollers inside this shell's view region.
	let viewEl = $state<HTMLElement | null>(null);
	let scrolledUnderBar = $state(false);

	$effect(() => {
		function onScroll(e: Event) {
			const t = e.target;
			if (!(t instanceof Element) || !viewEl?.contains(t)) return;
			const el = t as HTMLElement;
			if (typeof el.scrollTop !== "number") return;
			scrolledUnderBar = el.scrollTop > 2;
		}
		window.addEventListener("scroll", onScroll, { capture: true, passive: true });
		return () => window.removeEventListener("scroll", onScroll, true);
	});

	// A fresh view starts at its own top — the hairline belongs to a scroll
	// position this view hasn't reached yet.
	$effect(() => {
		void tab?.route;
		scrolledUnderBar = false;
	});
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
	class="single-window"
	onpointerdown={onPointerDown}
	onpointermove={onPointerMove}
	onpointerup={endDrag}
	onpointercancel={endDrag}
>
	<!-- The under-plane. Full width; the viewport slides off it entirely.
	     Parked and settled, it is also invisible — see the `parked` state. -->
	<div
		class="drawer-slot"
		class:dragging
		class:parked
		style:transform="translateX({drawerShift}px)"
		inert={!open && !dragging}
	>
		<MobileDrawer />
	</div>

	<!-- One view. Not one of many with the rest hidden — one. -->
	<div
		class="viewport"
		class:dragging
		class:offset={offset > 0}
		style:transform={offset ? `translateX(${offset}px)` : undefined}
		inert={open}
		bind:this={viewportEl}
	>
		<header class="topbar" class:edge={scrolledUnderBar}>
			<button class="bar-btn" onclick={() => mobileLayout.openDrawer()} aria-label="Menu">
				<Icon icon="ri:menu-line" width={22} />
			</button>
			{#if showGhostToggle}
				<button
					class="bar-btn"
					class:ghost-active={chrome?.ghost}
					onclick={() => chrome?.toggleGhost()}
					aria-pressed={chrome?.ghost}
					aria-label={chrome?.ghost
						? "Temporary chat — won't be saved"
						: "Start a temporary chat"}
				>
					<Icon icon="ri:ghost-line" width={22} />
				</button>
			{:else}
				<button class="bar-btn" onclick={newChat} aria-label="New chat">
					<Icon icon="ri:chat-new-line" width={22} />
				</button>
			{/if}
		</header>

		{#if reachability.unreachable}
			<!-- The failure state is the home screen now, so it gets a designed
			     answer: what's wrong, and the door to the diagnosis. Calm, not
			     red — the box being asleep is an ordinary morning, not an alarm. -->
			<div class="unreachable" role="status">
				<span class="unreachable-text">Can't reach your server</span>
				<button
					class="unreachable-door"
					onclick={() =>
						windowShellStore.openTabFromRoute("/virtues/devices/this", {
							label: "This device",
						})}
				>
					This device
				</button>
			</div>
		{/if}

		<div class="view" bind:this={viewEl}>
			{#if tab}
				<TabContent {tab} active={true} />
			{/if}
		</div>
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
		/* The two planes' stacking resolves in here and nowhere else. */
		isolation: isolate;
	}

	.drawer-slot {
		position: absolute;
		inset: 0;
		z-index: 0;
	}

	.viewport {
		position: relative;
		z-index: 1; /* above the drawer slot */
		display: flex;
		flex-direction: column;
		flex: 1;
		min-height: 0;
		overflow: hidden;
		/* Opaque on its own, matching the app shell's treatment: the drawer sits
		   beneath this plane, and a transparent viewport would show it through
		   the chat at rest. */
		background-color: var(--color-surface);
		background-image: var(--background-image);
		background-blend-mode: multiply;
	}

	/* The moving plane casts on the one it exposes. Only while displaced — a
	   permanent shadow on the resting viewport would read as a seam. */
	.viewport.offset {
		box-shadow: -12px 0 32px rgb(0 0 0 / 0.18);
	}

	/* The drag follows the finger; the release snaps. Both planes share one
	   clock so the parallax holds through the settle, not just the drag. */
	.viewport:not(.dragging),
	.drawer-slot:not(.dragging) {
		transition: transform 0.3s cubic-bezier(0.32, 0.72, 0, 1);
	}

	/* Applied by the `parked` timer, 360ms after the close settles. */
	.drawer-slot.parked {
		visibility: hidden;
	}

	.topbar {
		position: relative;
		flex: none;
		display: flex;
		align-items: center;
		justify-content: space-between;
		height: 48px;
		padding: 0 6px;
	}

	/* The scroll-edge hairline: present only while content is under the bar. */
	.topbar::after {
		content: "";
		position: absolute;
		left: 0;
		right: 0;
		bottom: 0;
		height: 0.5px;
		background: var(--color-border);
		opacity: 0;
		transition: opacity 0.2s ease;
		pointer-events: none;
	}
	.topbar.edge::after {
		opacity: 1;
	}

	/* Calm and factual: a strip, not a dialog. Warm-muted, never red — see the
	   comment at the render site. */
	.unreachable {
		flex: none;
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 10px;
		min-height: 36px;
		margin: 0 10px 6px;
		padding: 4px 6px 4px 12px;
		border: 1px solid var(--color-border);
		border-radius: 10px;
		background: color-mix(in srgb, var(--color-foreground) 4%, transparent);
	}

	.unreachable-text {
		font-size: 13px;
		color: var(--color-foreground-muted);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.unreachable-door {
		flex: none;
		min-height: 28px;
		padding: 0 10px;
		border: 0;
		border-radius: 7px;
		background: color-mix(in srgb, var(--color-foreground) 7%, transparent);
		color: var(--color-foreground);
		font-size: 13px;
		font-weight: 550;
		cursor: pointer;
		-webkit-tap-highlight-color: transparent;
		transition: background-color 0.25s ease-out;
	}
	.unreachable-door:active {
		background: color-mix(in srgb, var(--color-foreground) 12%, transparent);
		transition-duration: 0s;
	}

	.bar-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 44px;
		height: 44px;
		border: 0;
		border-radius: 10px;
		background: transparent;
		color: var(--color-foreground);
		cursor: pointer;
		-webkit-tap-highlight-color: transparent;
		/* Instant on press, fade on release — the native rhythm. */
		transition: background-color 0.25s ease-out;
	}
	.bar-btn:active {
		background: color-mix(in srgb, var(--color-foreground) 8%, transparent);
		transition-duration: 0s;
	}

	/* Same voice as ChatView's desktop ghost toggle: the mode is on. */
	.bar-btn.ghost-active {
		color: var(--color-primary);
		background: color-mix(in srgb, var(--color-primary) 14%, transparent);
	}

	.view {
		position: relative;
		flex: 1;
		min-height: 0;
		overflow: hidden;
	}

	@media (prefers-reduced-motion: reduce) {
		.viewport:not(.dragging),
		.drawer-slot:not(.dragging) {
			transition-duration: 0.12s;
			transition-timing-function: ease;
		}
	}
</style>
