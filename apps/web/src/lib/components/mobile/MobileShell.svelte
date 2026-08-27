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
	 * The slide idiom is the viewport moving, not the drawer: the drawer sits
	 * parked under the left edge and the chat slides right off it, staying
	 * visible as a sliver that is also the way back. Hand-rolled, because
	 * neither layer below us offers a drawer gesture — SvelteKit has no gesture
	 * surface at all, and wry/Tauri expose nothing useful on iOS (see the note
	 * on `back_forward_navigation_gestures` in git history).
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

	// The store folds itself to one window at boot; this catches the other way
	// in — a desktop browser dragged below the mobile breakpoint mid-session,
	// which is how this shell gets exercised without a phone.
	onMount(() => windowShellStore.collapseToSingleWindow());

	const tab = $derived(windowShellStore.activeTab);
	const open = $derived(mobileLayout.drawerOpen);

	// ── Drawer geometry ──────────────────────────────────────────────────────
	// Wide enough for titles, never edge-to-edge: the visible sliver of chat is
	// the close affordance, and it has to survive on the narrowest phone.
	function measureDrawer(): number {
		if (typeof window === "undefined") return 300;
		return Math.min(Math.round(window.innerWidth * 0.84), 320);
	}
	let drawerWidth = $state(measureDrawer());
	onMount(() => {
		const onResize = () => (drawerWidth = measureDrawer());
		window.addEventListener("resize", onResize);
		return () => window.removeEventListener("resize", onResize);
	});

	// ── The drawer gesture ───────────────────────────────────────────────────
	// One tracker serves both directions: from the left edge while closed it
	// opens, from anywhere on the exposed viewport while open it closes. The
	// viewport's offset follows the finger; release commits by position or by
	// flick, whichever the finger declared louder.
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
		if (open) {
			// Closing drag: only from the slid-out viewport (the sliver and the
			// scrim). Touches left of the offset are on the drawer's own rows and
			// belong to it.
			if (e.clientX < drawerWidth) return;
		} else {
			if (e.clientX > EDGE_PX) return;
			// A wide table, a code block, the activity heatmap: dragging one of
			// those sideways is the user scrolling it, not opening the drawer.
			if (inHorizontalScroller(e.target)) return;
		}
		tracking = true;
		startX = e.clientX;
		startY = e.clientY;
		startOffset = open ? drawerWidth : 0;
		lastX = e.clientX;
		lastT = e.timeStamp;
		velocity = 0;
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
		dragX = Math.max(0, Math.min(drawerWidth, startOffset + dx));
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
			shouldOpen = dragX > drawerWidth / 2;
		}
		dragX = 0;
		if (shouldOpen) mobileLayout.openDrawer();
		else mobileLayout.closeDrawer();
	}

	// Resting offset comes from state; a drag in flight overrides it.
	const offset = $derived(dragging ? dragX : open ? drawerWidth : 0);

	function newChat() {
		mobileLayout.closeDrawer();
		windowShellStore.openTabFromRoute("/chat", { label: "Chat" });
	}

	// The top-right slot is modal (see ChatChrome in mobileLayout): an empty
	// chat is already new, so the slot carries the temporary-chat toggle there;
	// everywhere else it composes.
	const chrome = $derived(mobileLayout.chatChrome);
	const showGhostToggle = $derived(chrome?.empty === true);
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
	class="single-window"
	onpointerdown={onPointerDown}
	onpointermove={onPointerMove}
	onpointerup={endDrag}
	onpointercancel={endDrag}
>
	<!-- Parked under the viewport; it never moves — the viewport slides off it. -->
	<div class="drawer-slot" style:width="{drawerWidth}px" inert={!open && !dragging}>
		<MobileDrawer />
	</div>

	<!-- One view. Not one of many with the rest hidden — one. -->
	<div
		class="viewport"
		class:dragging
		class:offset={offset > 0}
		style:transform={offset ? `translateX(${offset}px)` : undefined}
	>
		<header class="topbar">
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

		<div class="view">
			{#if tab}
				<TabContent {tab} active={true} />
			{/if}
		</div>

		{#if open || dragging}
			<!-- The slid-out viewport is a door, not a page: one tap closes the
			     drawer, and nothing underneath is reachable until it does. -->
			<button class="scrim" aria-label="Close menu" onclick={() => mobileLayout.closeDrawer()}
			></button>
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

	.drawer-slot {
		position: absolute;
		top: 0;
		bottom: 0;
		left: 0;
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

	/* The drag follows the finger; the release snaps. `transform` here makes
	   this element a containing block for fixed-position descendants, so the
	   transition only ever animates between offsets the gesture produced. */
	.viewport:not(.dragging) {
		transition: transform 0.28s cubic-bezier(0.32, 0.72, 0, 1);
	}

	.topbar {
		flex: none;
		display: flex;
		align-items: center;
		justify-content: space-between;
		height: 44px;
		padding: 0 6px;
	}

	.bar-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 40px;
		height: 40px;
		border: 0;
		border-radius: 10px;
		background: transparent;
		color: var(--color-foreground);
		cursor: pointer;
		-webkit-tap-highlight-color: transparent;
	}
	.bar-btn:active {
		background: color-mix(in srgb, var(--color-foreground) 8%, transparent);
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

	.scrim {
		position: absolute;
		inset: 0;
		z-index: 2;
		border: 0;
		padding: 0;
		background: transparent;
		cursor: pointer;
	}

	@media (prefers-reduced-motion: reduce) {
		.viewport:not(.dragging) {
			transition-duration: 0.12s;
			transition-timing-function: ease;
		}
	}
</style>
