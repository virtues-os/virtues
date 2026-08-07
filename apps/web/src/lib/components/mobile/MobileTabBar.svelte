<script lang="ts">
	/**
	 * The phone's bottom bar: a floating glass capsule that minimizes on scroll.
	 *
	 *   Home · Chat · Search · Pages · More
	 *
	 * It floats rather than being welded to the bottom edge, which is the whole
	 * point: content passes under it and reads through it, so the screen belongs
	 * to what you are looking at and the chrome is a slab hovering over it. That
	 * agrees with the rest of the phone shell — one window, no tabs, swipe to go
	 * back — where an edge-to-edge bar argued with it.
	 *
	 * Two states, and the difference is inset and opacity rather than content:
	 * at rest it is nearly full width, softly rounded and fairly solid, with
	 * labels; scrolling down it draws in to a narrower capsule, more transparent,
	 * icons only. Nothing leaves the row — the same five destinations are there
	 * in both, so the bar never has to be re-learned.
	 *
	 * The selection mechanic is a filled capsule *behind* the icon rather than a
	 * color change: it survives being read through glass, which a tint does not.
	 *
	 * There is no raised capture button any more. New chat is now published by
	 * ChatView as a view action, which puts it in the top action bar here and in
	 * the pane toolbar on the desktop: destinations belong in the bar, actions
	 * belong to the view.
	 *
	 * On the material — this is a WKWebView, so it is `backdrop-filter` and a
	 * hairline, not Apple's Liquid Glass. It cannot refract what is behind it or
	 * track device motion. Static, it is close; in motion it is an impression.
	 */
	import Icon from "$lib/components/Icon.svelte";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { mobileLayout } from "$lib/stores/mobileLayout.svelte";
	import { keyboard } from "$lib/stores/keyboard.svelte";
	import { search } from "$lib/stores/search.svelte";

	interface Tab {
		id: string;
		label: string;
		icon: string; // outline (inactive)
		iconActive: string; // filled (active)
		/** Route prefixes that mark this tab active. */
		match: string[];
		activate: () => void;
	}

	// Navigating to any content tab also leaves the Settings view (tap-away is
	// how you dismiss it — there's no modal to close).
	function go(route: string, label: string) {
		mobileLayout.closeMenu();
		windowShellStore.openTabFromRoute(route, { label });
	}

	const tabs: Tab[] = [
		{
			id: "home",
			label: "Home",
			icon: "ri:home-5-line",
			iconActive: "ri:home-5-fill",
			match: ["/home"],
			activate: () => go("/home", "Home"),
		},
		// Chat is a destination, not a creation. `/chat-history` does not match
		// it: the test is exact-or-followed-by-slash, so the two stay distinct.
		{
			id: "chat",
			label: "Chat",
			icon: "ri:chat-1-line",
			iconActive: "ri:chat-1-fill",
			match: ["/chat"],
			activate: () => go("/chat", "Chat"),
		},
		// Search had no primary door on the phone at all before this.
		{
			id: "search",
			label: "Search",
			icon: "ri:search-line",
			iconActive: "ri:search-line",
			match: [],
			activate: () => {
				mobileLayout.closeMenu();
				search.show();
			},
		},
		{
			id: "pages",
			label: "Pages",
			icon: "ri:file-text-line",
			iconActive: "ri:file-text-fill",
			match: ["/page"],
			activate: () => go("/page", "Pages"),
		},
		{
			id: "more",
			label: "More",
			icon: "ri:more-2-line",
			iconActive: "ri:more-2-fill",
			match: [],
			activate: () => (mobileLayout.menuOpen ? mobileLayout.closeMenu() : mobileLayout.openMenu()),
		},
	];

	const activeRoute = $derived(windowShellStore.activeTab?.route ?? "");

	function isActive(tab: Tab): boolean {
		if (tab.id === "more") return mobileLayout.menuOpen;
		// Search is a state you are in, not a place you went — it lights up while
		// the palette is up and goes out with it. `match` can't express that,
		// because the route underneath never changed.
		if (tab.id === "search") return search.open;
		if (mobileLayout.menuOpen || search.open) return false;
		return tab.match.some((p) => activeRoute === p || activeRoute.startsWith(p + "/"));
	}

	// ── Minimize on scroll ───────────────────────────────────────────────────
	// Every view owns its own scroller — the shell is fixed and the document
	// never scrolls — so there is no one element to listen to. A capture-phase
	// listener on `window` sees scroll events from any descendant (scroll does
	// not bubble, but capture still descends to the target), which means this
	// works for whatever view happens to be mounted without any of them knowing.
	//
	// Thresholds are asymmetric on purpose: it takes little downward movement to
	// tuck the bar away and rather more upward movement to bring it back, so a
	// finger wobbling mid-scroll doesn't make it flicker.
	const TOP_ZONE = 12; // within this of the top, always at rest
	const DOWN_TO_MIN = 6;
	const UP_TO_REST = 12;

	let minimized = $state(false);

	// The sheet is not scrolled content — if you have opened More, the bar should
	// be its full self, whatever the view behind it was doing when you left it.
	const collapsed = $derived(minimized && !mobileLayout.menuOpen);

	$effect(() => {
		const seen = new WeakMap<object, number>();

		function onScroll(e: Event) {
			const t = e.target as unknown;
			const el = (t === document ? document.scrollingElement : t) as HTMLElement | null;
			if (!el || typeof el.scrollTop !== "number") return;

			const y = el.scrollTop;
			const prev = seen.get(el) ?? 0;
			seen.set(el, y);

			if (y <= TOP_ZONE) {
				minimized = false;
				return;
			}
			const dy = y - prev;
			if (dy > DOWN_TO_MIN) minimized = true;
			else if (dy < -UP_TO_REST) minimized = false;
		}

		window.addEventListener("scroll", onScroll, { capture: true, passive: true });
		return () => window.removeEventListener("scroll", onScroll, true);
	});
</script>

<!-- While the keyboard is up the bar steps aside: on iOS the composer owns the
     bottom edge, and a tab bar wedged between the text you are writing and the
     keyboard you are writing it with is the thing that reads as broken. It
     slides rather than vanishes, so dismissing the keyboard brings it back the
     way it left. Stowing beats minimizing — a bar that is gone has no state. -->
<nav class="tabbar" class:min={collapsed} class:stowed={keyboard.open} aria-label="Main">
	<div class="glass">
		{#each tabs as tab (tab.id)}
			{@const active = isActive(tab)}
			<button
				class="tab"
				class:active
				onclick={tab.activate}
				aria-label={tab.label}
				aria-current={active ? "page" : undefined}
			>
				<span class="glyph">
					<Icon icon={active ? tab.iconActive : tab.icon} width={23} />
				</span>
				<span class="label">{tab.label}</span>
			</button>
		{/each}
	</div>
</nav>

<style>
	/* The frame: full width, pinned above the home indicator. Only its inline
	   padding animates, which is what draws the capsule in and out. */
	.tabbar {
		position: fixed;
		left: 0;
		right: 0;
		bottom: calc(env(safe-area-inset-bottom) + var(--tabbar-gap));
		z-index: var(--z-sticky);
		display: flex;
		justify-content: center;
		padding-inline: 10px;
		transition:
			padding-inline 0.32s cubic-bezier(0.32, 0.72, 0, 1),
			transform 0.24s cubic-bezier(0.32, 0.72, 0, 1);
	}

	/* Drawn in and centred. */
	.tabbar.min {
		padding-inline: 42px;
	}

	.tabbar.stowed {
		/* Further than 100%: the bar floats clear of the edge, so its own height
		   is not enough to put it off screen. */
		transform: translateY(180%);
		pointer-events: none;
	}

	/* The material. Theme-derived rather than a fixed dark, because a theme here
	   can be paper-white — a hardcoded smoked glass would be a hole in it. */
	.glass {
		display: flex;
		align-items: stretch;
		justify-content: space-around;
		width: 100%;
		height: var(--tabbar-h);
		border-radius: 26px;
		background: color-mix(in srgb, var(--color-surface) 76%, transparent);
		backdrop-filter: blur(28px) saturate(180%);
		-webkit-backdrop-filter: blur(28px) saturate(180%);
		border: 0.5px solid color-mix(in srgb, var(--color-border) 70%, transparent);
		/* Two shadows doing different jobs: an inset hairline along the top edge
		   for the lit rim that reads as thickness, and a soft drop that lifts the
		   whole thing off the content. */
		box-shadow:
			inset 0 0.5px 0 color-mix(in srgb, var(--color-foreground) 10%, transparent),
			0 8px 28px color-mix(in srgb, var(--color-foreground) 14%, transparent);
		overflow: hidden;
		transition:
			height 0.32s cubic-bezier(0.32, 0.72, 0, 1),
			border-radius 0.32s cubic-bezier(0.32, 0.72, 0, 1),
			background-color 0.32s ease;
	}

	/* Minimized: shorter, fully capsule, and thinner — you should be able to
	   read the content through it, which is the tell that it is floating. */
	.tabbar.min .glass {
		height: var(--tabbar-h-min);
		border-radius: 999px;
		background: color-mix(in srgb, var(--color-surface) 58%, transparent);
	}

	.tab {
		flex: 1;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 3px;
		min-width: 0;
		border: 0;
		background: transparent;
		color: var(--color-foreground-muted);
		cursor: pointer;
		-webkit-tap-highlight-color: transparent;
		transition: color 0.18s ease;
	}

	.tab.active {
		color: var(--color-foreground);
	}

	/* The selection pill lives here, and the padding is always applied so that
	   activating a tab changes a color and nothing else — if the padding
	   appeared with the background, every tap would nudge the row. */
	.glyph {
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 4px 16px;
		border-radius: 999px;
		background: transparent;
		transition:
			background-color 0.2s ease,
			transform 0.12s cubic-bezier(0.32, 0.72, 0, 1);
	}

	.tab.active .glyph {
		background: color-mix(in srgb, var(--color-foreground) 12%, transparent);
	}

	/* Physical tap feedback. */
	.tab:active .glyph {
		transform: scale(0.88);
	}

	/* Labels are the one thing the two states disagree about: they are what the
	   bar can afford at rest and what it gives up to get out of the way. */
	.label {
		font-size: 10px;
		line-height: 1;
		font-weight: 500;
		letter-spacing: 0.01em;
		max-height: 12px;
		opacity: 1;
		overflow: hidden;
		transition:
			max-height 0.28s cubic-bezier(0.32, 0.72, 0, 1),
			opacity 0.18s ease;
	}

	.tab.active .label {
		font-weight: 650;
	}

	.tabbar.min .label {
		max-height: 0;
		opacity: 0;
	}

	/* Reduced motion: the states still differ, they just arrive rather than
	   travel. The bar changing size under a finger is exactly the kind of
	   movement this setting is asking us not to make. */
	@media (prefers-reduced-motion: reduce) {
		.tabbar,
		.glass,
		.glyph,
		.label {
			transition: none;
		}
	}
</style>
