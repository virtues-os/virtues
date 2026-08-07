<script lang="ts">
	/**
	 * The phone's bottom bar: a floating glass capsule that minimizes on scroll.
	 *
	 *   Home · Chat · Search · Pages · More
	 *
	 * It floats rather than being welded to the bottom edge, which is the whole
	 * point: content passes under it and reads through it, so the screen belongs
	 * to what you are looking at and the chrome is a slab hovering over it.
	 *
	 * Two states, and the difference is *only* inset, height and opacity — never
	 * content. At rest it is nearly full width, a rounded rectangle, fairly
	 * solid; scrolling down it draws in to a narrower, shorter, more transparent
	 * capsule. The same five icons are in both, at the same size, so the bar is
	 * never re-learned and nothing pops in or out under your thumb.
	 *
	 * No labels. They cost three separate problems before they came out: the bar
	 * read as a conventional tab bar wearing glass rather than as a glass object,
	 * the icons had no room to breathe, and the selection highlight had to hug
	 * the icon (leaving the label dangling below it) because it could not
	 * sensibly wrap both. The names live on as `aria-label`, which is what a
	 * screen reader was reading anyway.
	 *
	 * Selection is a capsule that SLIDES between tabs rather than appearing under
	 * the new one. That movement is most of what makes a bar like this feel
	 * alive, and it is the reason the pill is one absolutely-positioned element
	 * measured against the buttons rather than a background on each of them.
	 *
	 * On the material — this is a WKWebView, so it is `backdrop-filter` and a
	 * hairline rim, not Apple's Liquid Glass. It cannot refract what is behind it
	 * or track device motion. There is deliberately no drop shadow: the lift
	 * comes from the rim and the blur, and a cast shadow under a translucent
	 * object only muddies what you are meant to be reading through it.
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

	const activeIndex = $derived(tabs.findIndex(isActive));

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

	// ── The sliding selection pill ───────────────────────────────────────────
	// Measured rather than styled, because it has to travel between buttons.
	// It tracks the active tab's glyph box; the transition on `transform` and
	// `width` is what turns a jump into a slide.
	let glassEl = $state<HTMLElement | null>(null);
	let pill = $state<{ x: number; y: number; w: number; h: number } | null>(null);

	// Queried rather than bound. `bind:this` into an array index inside a keyed
	// `{#each}` left the refs empty here, and the pill silently never rendered —
	// the glyphs are already findable from the container, so this needs no
	// bookkeeping to go wrong.
	/**
	 * Measured off the whole tab. With the labels gone the tab *is* the icon's
	 * slot, so this lands where the reference puts it: a capsule filling most of
	 * the bar's height and about the width of one fifth of it.
	 */
	const INSET_X = 5;
	const INSET_Y = 4; // ~85% of the bar's height, which is where the reference sits

	function measurePill(idx: number) {
		const g = glassEl;
		const el = g?.querySelectorAll<HTMLElement>(".tab")[idx];
		if (!g || !el) {
			pill = null;
			return;
		}
		const gb = g.getBoundingClientRect();
		const eb = el.getBoundingClientRect();
		if (eb.width === 0) return; // not laid out yet — keep the last good value
		pill = {
			x: eb.left - gb.left + INSET_X,
			y: eb.top - gb.top + INSET_Y,
			w: eb.width - INSET_X * 2,
			h: eb.height - INSET_Y * 2,
		};
	}

	/**
	 * What the pill is aiming at. A derived object rather than two loose reads,
	 * because the effect below has to depend on both and a *bare* read is not a
	 * dependency — `void activeIndex;` compiles away, the effect never re-runs,
	 * and the pill sits on whichever tab happened to be active at mount. It did
	 * exactly that until a resize (which calls the measure directly) proved the
	 * measurement was fine and the tracking was not.
	 */
	const pillTarget = $derived({ idx: activeIndex, min: collapsed, glass: glassEl });

	// Re-measure whenever the target moves. Two of these are instantaneous (the
	// active tab changing, the row re-laying out) and one is not: the state
	// transition takes ~340ms, during which the buttons are still moving. A
	// short rAF follow keeps the pill glued to its glyph for the duration
	// instead of arriving before the row it belongs to.
	$effect(() => {
		const { idx, glass } = pillTarget;
		if (!glass) return;

		// Place it now, synchronously. The rAF follow below only *refines* the
		// position while the row is still moving; it must not be what puts the
		// pill on screen in the first place, because a throttled page (hidden
		// tab, backgrounded app) doesn't run animation frames at all — and the
		// pill would then be missing entirely rather than merely un-animated.
		measurePill(idx);

		// …and once more off a timer, because the synchronous attempt above can
		// land before the row has been laid out (it measures zero width and
		// keeps the last good value). rAF would normally catch that on the next
		// frame, but a throttled page runs no frames — so the retry that must
		// not depend on them gets a timeout instead.
		const retry = setTimeout(() => measurePill(idx), 0);

		let raf = 0;
		const until = performance.now() + 460;
		const tick = () => {
			measurePill(idx);
			if (performance.now() < until) raf = requestAnimationFrame(tick);
		};
		raf = requestAnimationFrame(tick);

		const onResize = () => measurePill(idx);
		window.addEventListener("resize", onResize);
		return () => {
			clearTimeout(retry);
			cancelAnimationFrame(raf);
			window.removeEventListener("resize", onResize);
		};
	});
</script>

<!-- While the keyboard is up the bar steps aside: on iOS the composer owns the
     bottom edge, and a tab bar wedged between the text you are writing and the
     keyboard you are writing it with is the thing that reads as broken. It
     slides rather than vanishes, so dismissing the keyboard brings it back the
     way it left. Stowing beats minimizing — a bar that is gone has no state. -->
<nav class="tabbar" class:min={collapsed} class:stowed={keyboard.open} aria-label="Main">
	<div class="glass" class:min={collapsed} bind:this={glassEl}>
		{#if pill}
			<span
				class="pill"
				style:transform="translate({pill.x}px, {pill.y}px)"
				style:width="{pill.w}px"
				style:height="{pill.h}px"
				aria-hidden="true"
			></span>
		{/if}

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
					<Icon icon={active ? tab.iconActive : tab.icon} width={24} />
				</span>
			</button>
		{/each}
	</div>
</nav>

<style>
	/* The frame: full width, pinned above the home indicator, and static — the
	   capsule's width lives on `.glass` as an interpolatable calc rather than as
	   padding here, because a shorthand logical property is the kind of thing a
	   browser is entitled to decline to animate. */
	.tabbar {
		position: fixed;
		left: 0;
		right: 0;
		bottom: calc(env(safe-area-inset-bottom) + var(--tabbar-gap));
		z-index: var(--z-sticky);
		display: flex;
		justify-content: center;
		padding-inline: 10px;
		transition: transform 0.26s cubic-bezier(0.32, 0.72, 0, 1);
	}

	.tabbar.stowed {
		/* Further than 100%: the bar floats clear of the edge, so its own height
		   is not enough to put it off screen. */
		transform: translateY(180%);
		pointer-events: none;
	}

	/* The material. Theme-derived rather than a fixed smoked grey, because a
	   theme here can be paper-white and a hardcoded dark pane would be a hole
	   punched in it.

	   No drop shadow. The lift is the rim and the blur; a cast shadow under a
	   translucent object darkens the very content you are supposed to be able to
	   read through it. */
	.glass {
		position: relative;
		display: flex;
		/* Stretch, not centre: the tabs have to fill the bar's height for the
		   pill — which is measured off a tab — to fill it too. Centred, each tab
		   shrank to its icon and the highlight came out at 46% of the bar,
		   hugging the glyph exactly like the version this replaced. */
		align-items: stretch;
		justify-content: space-around;
		width: 100%;
		max-width: calc(100% - 0px);
		height: var(--tabbar-h);
		border-radius: 18px;
		/* Thin enough that the blur has something to say. At 76% the fill was
		   doing all the work and the result was a flat panel that happened to
		   sit at the bottom — you could not tell there was anything behind it,
		   which is the only thing separating glass from grey. */
		background-color: color-mix(in srgb, var(--color-surface) 60%, transparent);
		backdrop-filter: blur(32px) saturate(200%);
		-webkit-backdrop-filter: blur(32px) saturate(200%);
		border: 0.5px solid color-mix(in srgb, var(--color-border) 70%, transparent);
		/* The lit top edge that reads as thickness. */
		box-shadow: inset 0 0.5px 0 color-mix(in srgb, var(--color-foreground) 12%, transparent);
		overflow: hidden;
		/* Both ends of max-width are calc(), so there is no percentage-to-keyword
		   interpolation for the browser to give up on. */
		transition:
			max-width 0.34s cubic-bezier(0.32, 0.72, 0, 1),
			height 0.34s cubic-bezier(0.32, 0.72, 0, 1),
			border-radius 0.34s cubic-bezier(0.32, 0.72, 0, 1),
			background-color 0.34s ease;
	}

	/* Minimized: drawn in, shorter, fully capsule, and thin enough that content
	   reads through it — which is the tell that it is floating. 23px is exactly
	   half the minimized height, so the radius lands on a true capsule instead
	   of racing there via a large number. */
	.glass.min {
		/* A harder draw-in than before: with no labels to shed, the inset is
		   most of what says "minimized", so it has to be worth seeing. */
		max-width: calc(100% - 84px);
		height: var(--tabbar-h-min);
		border-radius: 22px; /* exactly half the minimized height — a true capsule */
		background-color: color-mix(in srgb, var(--color-surface) 44%, transparent);
	}

	/* The pill sits behind the row and travels. */
	/* Positioned from the container's own origin — `top: 0; left: 0` plus a
	   two-axis translate — rather than relying on an absolutely-positioned
	   flex child's static position, which is not somewhere to build on. */
	.pill {
		position: absolute;
		top: 0;
		left: 0;
		/* A capsule now that it is wrapping an icon rather than an icon stacked
		   on a word. */
		border-radius: 999px;
		background: color-mix(in srgb, var(--color-foreground) 11%, transparent);
		pointer-events: none;
		transition:
			transform 0.36s cubic-bezier(0.32, 0.72, 0, 1),
			width 0.28s cubic-bezier(0.32, 0.72, 0, 1),
			height 0.28s cubic-bezier(0.32, 0.72, 0, 1);
	}

	.tab {
		position: relative; /* above the pill */
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
		transition: color 0.22s ease;
	}

	.tab.active {
		color: var(--color-foreground);
	}

	/* The pill is measured off this box, so its padding is what gives the
	   selection its shape. */
	.glyph {
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 5px 18px;
		border-radius: 999px;
		transition: transform 0.12s cubic-bezier(0.32, 0.72, 0, 1);
	}

	/* Physical tap feedback. */
	.tab:active .glyph {
		transform: scale(0.88);
	}

	/* Reduce Motion asks for less movement, not for none — and `transition:
	   none` here is why the bar snapped between its two states for anyone who
	   had it switched on. Keep the change legible, take the travel out of it:
	   everything crossfades quickly and the pill stops sliding. */
	@media (prefers-reduced-motion: reduce) {
		.tabbar,
		.glass,
		.tab {
			transition-duration: 0.12s;
			transition-timing-function: ease;
		}

		.pill {
			transition: opacity 0.12s ease;
		}

		.glyph {
			transition: none;
		}
	}
</style>
