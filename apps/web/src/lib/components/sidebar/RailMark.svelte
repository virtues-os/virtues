<script lang="ts">
	/**
	 * The ∴ mark: the app's identity and the sidebar's toggle. It leads the
	 * window header, above the rail, so it never moves when the sidebar opens
	 * or closes. Click toggles the sidebar; right-click toggles the rail's
	 * labels. With the labels gone the rail narrows to the glyph column
	 * (Obsidian's ribbon); the room names stay in each tile's tooltip and
	 * accessible name.
	 */
	import { sidebarState } from '$lib/stores/sidebarState.svelte';

	const panelOpen = $derived(!sidebarState.collapsed);
	const compact = $derived(sidebarState.railCompact);

	function toggleSidebar() {
		// The mark is the sidebar's own control, in both directions. A hide
		// button on the panel header would vanish with the panel, and the way
		// back would have to be a second, different control.
		sidebarState.toggle();
	}

	function toggleLabels(e: MouseEvent) {
		// The browser's own context menu has nothing to offer on the mark.
		e.preventDefault();
		sidebarState.toggleRailCompact();
	}

	/**
	 * THE MARK AS A HINGE. ∴ is equilateral, so a third of a turn about its
	 * centroid lands exactly on itself: the dots trade places, the eye reads a
	 * turn, and the resting frame is the logo unchanged. The mark turns
	 * whenever the sidebar moves — click, ⌘B, or a room's second press — one
	 * way to open and the other to close, because the hinge is the thing that
	 * moved, not the thing that was pressed.
	 *
	 * It is driven off the sidebar's state rather than the click so every
	 * door tells the same story, and it skips the first run so mounting does
	 * not turn the mark. This is the ONLY motion the mark makes on its own:
	 * ∴ in motion already means "a turn is working" (see ThinkingMark), so an
	 * idle or looping mark would be the app lying about itself.
	 */
	let turn = $state<'open' | 'close' | null>(null);
	let lastOpen: boolean | undefined;
	$effect(() => {
		const open = panelOpen;
		if (lastOpen !== undefined && lastOpen !== open) turn = open ? 'open' : 'close';
		lastOpen = open;
	});

	function turnDone(e: AnimationEvent) {
		// animationend bubbles, and the hover beat on the dots ends too — only
		// the figure's own turn clears the turn.
		if (e.target === e.currentTarget) turn = null;
	}
</script>

<button
	type="button"
	class="rail-mark rail-mark-btn"
	aria-label={panelOpen ? 'Hide the sidebar' : 'Show the sidebar'}
	aria-expanded={panelOpen}
	title={`${panelOpen ? 'Hide' : 'Show'} the sidebar (⌘B) · Right-click to ${compact ? 'show' : 'hide'} labels`}
	onclick={toggleSidebar}
	oncontextmenu={toggleLabels}
>
	<!-- Drawn, not typed. A typed ∴ glyph is text-weight — a 21px glyph
	     put a 12px figure with 2px dots over a column of 20px line icons. The
	     geometry is the app icon's and ThinkingMark's, exactly: equilateral,
	     side 15, r 3, on the 24-unit box `virtues:logo` uses in icons.ts, so
	     the header, the chat, and the Dock all show one mark. -->
	<svg viewBox="0 0 24 24" width="17" height="17" aria-hidden="true" focusable="false">
		<g
			class="mark-figure"
			class:turn-open={turn === 'open'}
			class:turn-close={turn === 'close'}
			onanimationend={turnDone}
		>
			<circle class="mark-dot apex" cx="12" cy="5" r="3" />
			<circle class="mark-dot left" cx="4.5" cy="18" r="3" />
			<circle class="mark-dot right" cx="19.5" cy="18" r="3" />
		</g>
	</svg>
</button>

<style>
	.rail-mark {
		display: grid;
		place-items: center;
		width: 32px;
		height: 28px;
		flex: none;
		color: var(--color-foreground);
	}

	.rail-mark svg {
		display: block;
		/* The hover swell and the turn both reach past the 24-box. */
		overflow: visible;
	}

	.rail-mark-btn {
		border: none;
		background: none;
		padding: 0;
		cursor: pointer;
	}

	/* The turn is about the CENTROID (12, 13.667), not the box centre — about
	   the centre a 120° turn walks the mark 3.75 units off and you see the
	   wobble. No fill-mode: the last frame and the resting frame are the same
	   three dots, so the snap back to 0° is invisible. */
	.mark-figure {
		transform-box: view-box;
		transform-origin: 12px 13.667px;
	}

	.mark-figure.turn-open {
		animation: mark-turn-open 320ms cubic-bezier(0.2, 0.7, 0.2, 1) 1;
	}

	.mark-figure.turn-close {
		animation: mark-turn-close 320ms cubic-bezier(0.2, 0.7, 0.2, 1) 1;
	}

	@keyframes mark-turn-open {
		from { transform: rotate(0deg); }
		to { transform: rotate(120deg); }
	}

	@keyframes mark-turn-close {
		from { transform: rotate(0deg); }
		to { transform: rotate(-120deg); }
	}

	.mark-dot {
		fill: currentColor;
		transform-box: fill-box;
		transform-origin: center;
	}

	/* HOVER: one syllogism beat — premise, premise, therefore. The base pair
	   swells in turn, then the apex, more and longer, once. It is
	   ThinkingMark's pulse (1.22 / 1.26, 140ms stagger — under ~100ms the
	   three stop reading as an order) played once instead of looped. Plays on
	   hover-in only; the pointer leaving snaps to rest, which is where every
	   beat ends anyway. */
	.rail-mark-btn:hover .mark-dot.left {
		animation: mark-premise 520ms ease-out 1;
	}

	.rail-mark-btn:hover .mark-dot.right {
		animation: mark-premise 520ms ease-out 140ms 1;
	}

	.rail-mark-btn:hover .mark-dot.apex {
		animation: mark-conclude 760ms ease-out 280ms 1;
	}

	@keyframes mark-premise {
		0% { transform: scale(1); }
		35% { transform: scale(1.22); }
		100% { transform: scale(1); }
	}

	@keyframes mark-conclude {
		0% { transform: scale(1); }
		30% { transform: scale(1.26); }
		60% { transform: scale(1.06); }
		100% { transform: scale(1); }
	}

	@media (prefers-reduced-motion: reduce) {
		.mark-figure,
		.rail-mark-btn:hover .mark-dot {
			animation: none;
		}
	}

	.rail-mark-btn:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: 2px;
		border-radius: var(--sidebar-interactive-radius);
	}
</style>
