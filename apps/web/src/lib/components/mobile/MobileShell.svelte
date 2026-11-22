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
	 * Glass pills float over the view. The left one opens the drawer, so it is
	 * the way out of any non-chat view and must exist everywhere. The right one
	 * is the shell's one other verb, New chat. Between them, a chat that is in
	 * a project names it: the phone's title slot, and the way to its menu.
	 */
	import { onMount } from "svelte";
	import { goto } from "$app/navigation";
	import Icon from "$lib/components/Icon.svelte";
	import AtlasIcon from "$lib/components/sidebar/AtlasIcon.svelte";
	import { search } from "$lib/stores/search.svelte";
	import TabContent from "$lib/components/tabs/TabContent.svelte";
	import MobileDrawer from "$lib/components/mobile/MobileDrawer.svelte";
	import ProjectChip from "$lib/components/ProjectChip.svelte";
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

	// An empty chat is already new, so its bar has no new-chat button.
	const chrome = $derived(mobileLayout.chatChrome);
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
		<!-- Glass pills over the view, so the page scrolls under them: the way
		     back to your chats on the left, the project in the middle, and new
		     chat (or the temporary-chat switch on an empty chat) on the right. -->
		{#if chrome && !chrome.empty}
			<div class="bar-fade" aria-hidden="true"></div>
		{/if}
		<header class="topbar">
			<!-- The ∴ opens the drawer, the way the mark opens the desktop's rail. -->
			<button class="bar-btn lq" onclick={() => mobileLayout.openDrawer()} aria-label="Threads and chats">
				<svg viewBox="0 0 12 10.5" width="15" height="13" fill="currentColor" aria-hidden="true">
					<circle cx="6" cy="2.4" r="1.5" />
					<circle cx="2.6" cy="8.1" r="1.5" />
					<circle cx="9.4" cy="8.1" r="1.5" />
				</svg>
			</button>
			{#if chrome?.project}
				<!-- A tap is the project's menu (open it, move the chat, take it
				     out); there is no right-click here. -->
				<div class="bar-title lq">
					<ProjectChip
						bar
						project={chrome.project}
						title={`${chrome.project.name}: project options`}
						onclick={(e) => chrome?.showProjectMenu?.(e)}
					/>
				</div>
			{/if}
			<div class="bar-group" class:alone={!chrome?.project}>
				<button class="bar-btn lq" onclick={() => search.show()} aria-label="Search">
					<AtlasIcon name="search" size={17} bare />
				</button>
				{#if chrome?.showChatMenu}
					<button class="bar-btn lq" onclick={(e) => chrome?.showChatMenu?.(e)} aria-haspopup="menu" aria-label="Chat options">
						<Icon icon="mynaui:dots" width={18} />
					</button>
				{/if}
				{#if !chrome?.empty}
					<button class="bar-btn lq" onclick={newChat} aria-label="New chat">
						<AtlasIcon name="plus" size={17} bare />
					</button>
				{/if}
			</div>
		</header>

		{#if reachability.unreachable}
			<!-- The failure state is the home screen now, so it gets a designed
			     answer: what's wrong, and the door to the diagnosis. Calm, not
			     red — the box being asleep is an ordinary morning, not an alarm.
			     The door is /reconnect (2026-09-28), which tells this phone
			     being offline from the server being off, refused, or moved. -->
			<div class="unreachable" role="status">
				<span class="unreachable-text">Can't reach your server</span>
				<button class="unreachable-door" onclick={() => void goto("/reconnect")}>Fix it</button>
			</div>
		{/if}

		<div class="view">
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
		/* Opaque on its own, the page like the desktop's main card: the drawer
		   sits beneath this plane, and a transparent viewport would show it
		   through the chat at rest. */
		background-color: var(--color-page);
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

	/* Floats over the view: the page, and the painting on an empty chat,
	   run up under the pills. The view tells its scrollers how much room the
	   bar takes through --shell-bar-inset. */
	.topbar {
		position: absolute;
		top: 0;
		left: 0;
		right: 0;
		z-index: 2;
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
		height: 40px;
		margin: 12px 12px 0;
		pointer-events: none;
	}
	.topbar > :global(*) {
		pointer-events: auto;
	}

	/* Beside the ∴, like the desktop's breadcrumb, so it never runs into the
	   buttons on the right however long the name is. */
	.bar-title {
		flex: 0 1 auto;
		min-width: 0;
		margin-right: auto;
		display: flex;
		align-items: center;
		height: 40px;
		padding: 0 8px;
		border-radius: var(--radius-full);
		overflow: hidden;
	}

	/* Under a saved chat the page fades out behind the bar, so reply text
	   does not show through the glass. */
	.bar-fade {
		position: absolute;
		top: 0;
		left: 0;
		right: 0;
		z-index: 2;
		height: 88px;
		background: linear-gradient(var(--color-page) 50%, transparent);
		pointer-events: none;
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
		margin: 68px 10px 6px;
		padding: 4px 6px 4px 12px;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-xl);
		background: color-mix(in srgb, var(--wash-ink) 4%, transparent);
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
		border-radius: var(--radius-md);
		background: color-mix(in srgb, var(--wash-ink) 7%, transparent);
		color: var(--color-foreground);
		font-size: 13px;
		font-weight: 550;
		cursor: pointer;
		-webkit-tap-highlight-color: transparent;
		transition: background-color 0.25s ease-out;
	}
	.unreachable-door:active {
		background: color-mix(in srgb, var(--wash-ink) 12%, transparent);
		transition-duration: 0s;
	}

	.bar-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 40px;
		height: 40px;
		border: 0;
		border-radius: var(--radius-full);
		color: var(--color-foreground);
		cursor: pointer;
		-webkit-tap-highlight-color: transparent;
	}

	.bar-group {
		flex: none;
		display: flex;
		gap: 8px;
	}
	.bar-group.alone {
		margin-left: auto;
	}

	/* The press: a quick shrink under the finger and a spring back. */
	.bar-btn {
		transition: transform 0.32s cubic-bezier(0.34, 1.56, 0.64, 1);
	}
	.bar-btn:active {
		transform: scale(0.92);
		transition-duration: 0.08s;
	}
	@media (prefers-reduced-motion: reduce) {
		.bar-btn,
		.bar-btn:active {
			transition: none;
			transform: none;
		}
	}

	.view {
		--shell-bar-inset: 56px;
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
