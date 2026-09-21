<script lang="ts">
	/**
	 * The rail — the shell's fixed spine.
	 *
	 * It never opens a tab and never touches the panes. Clicking a room swaps
	 * the panel beside it; clicking the room already selected collapses the
	 * panel and leaves the rail standing. That is VS Code's activity-bar
	 * contract, the reflex most people already have, and the reason the rail is
	 * a LENS rather than a router: with split panes "where am I?" and "what's
	 * open?" are different questions, and a router would answer both with one
	 * highlight and therefore lie.
	 *
	 * Three states, no new furniture and no banned edge-strip:
	 *
	 *   resting   — muted glyph and label.
	 *   occupied  — a pane holds this room. Full ink, NO fill.
	 *   selected  — the panel is showing this room. Full ink on a filled tile
	 *               from the interaction ramp. The fill is what tells selected
	 *               from occupied.
	 *
	 * Deliberately not a coloured leading-edge strip (design.md's first
	 * anti-slop rule) and not a dot, which would be one more object on a desk
	 * whose job is to whisper.
	 *
	 * No separators inside it: separation goes whitespace → lightness →
	 * elevation and stops at the first that reads, and here whitespace reads.
	 * One even rhythm for every room; only the utility pair is set apart, by the
	 * spacer that pushes it to the foot.
	 *
	 * Two objects on the rail are not rooms: the ∴ mark at the head, and Setup
	 * at the foot of the spacer while getting started is unfinished. Each has
	 * a job of its own (toggle the sidebar; open the getting-started chat),
	 * neither takes the occupied or selected state, and Setup leaves on
	 * graduation without moving anything — it sits at the bottom of the
	 * spacer, so its going only lengthens the gap.
	 */
	import AtlasIcon from './AtlasIcon.svelte';
	import SetupTile from './SetupTile.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { sidebarRoom } from '$lib/stores/sidebarRoom.svelte';
	import { sidebarState } from '$lib/stores/sidebarState.svelte';
	import { gettingStarted } from '$lib/stores/gettingStarted.svelte';
	import { roomForRoute, roomsInGroup, type Room } from '$lib/sidebar/rooms';

	const primary = roomsInGroup('primary');
	const library = roomsInGroup('library');
	const utility = roomsInGroup('utility');

	/**
	 * Which rooms the panes are holding. Every pane's ACTIVE tab counts, not
	 * every open tab — a room parked in a background tab is not a room you are
	 * in, and marking it would make the rail glow with history.
	 */
	const occupied = $derived.by(() => {
		const ids = new Set<string>();
		for (const pane of windowShellStore.panes) {
			const tab = pane.tabs.find((t) => t.id === pane.activeTabId);
			const room = roomForRoute(tab?.route);
			if (room) ids.add(room.id);
		}
		return ids;
	});

	const selectedId = $derived(sidebarRoom.selectedId);
	const panelOpen = $derived(!sidebarState.collapsed);

	function isSelected(room: Room): boolean {
		return room.id === selectedId && panelOpen;
	}

	function toggleSidebar() {
		// The mark is the sidebar's own control, in both directions. It was
		// identity-only, with the hide button living over on the panel header —
		// but that put the control on the thing being hidden, so it vanished with
		// it and the way back had to be a second, different affordance. One
		// object, one job, always in the same place.
		sidebarState.toggle();
	}

	/**
	 * THE MARK AS A HINGE. ∴ is equilateral, so a third of a turn about its
	 * centroid lands exactly on itself: the dots trade places, the eye reads a
	 * turn, and the resting frame is the logo unchanged. The mark turns
	 * whenever the sidebar moves — click, ⌘S, or a room's second press — one
	 * way to open and the other to close, because the hinge is the thing that
	 * moved, not the thing that was pressed.
	 *
	 * It is driven off the sidebar's state rather than the click so every
	 * door tells the same story, and it skips the first run so mounting does
	 * not turn the mark. This is the ONLY motion the rail mark makes on its
	 * own: ∴ in motion already means "a turn is working" (see ThinkingMark),
	 * so an idle or looping rail mark would be the app lying about itself.
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

	function activate(room: Room) {
		if (room.id === selectedId) {
			// The second press on the same room is the collapse. A rail item that
			// re-navigated here would be a control going where you already are,
			// which design.md prohibits.
			sidebarState.toggle();
			return;
		}
		sidebarRoom.select(room.id);
		sidebarState.collapsed = false;
	}
</script>

<nav class="rail" aria-label="Rooms">
	<!-- Identity AND the sidebar's toggle. The mark no longer carries the
	     "way home" job the old path-mast root did (Chats is the ground now,
	     and it is the first tile); what it carries instead is open/closed, in
	     both directions, from a place that never moves. -->
	<button
		type="button"
		class="rail-mark rail-mark-btn"
		aria-label={panelOpen ? 'Hide the sidebar' : 'Show the sidebar'}
		aria-expanded={panelOpen}
		title={panelOpen ? 'Hide the sidebar (⌘S)' : 'Show the sidebar (⌘S)'}
		onclick={toggleSidebar}
	>
		<!-- Drawn, not typed. The JJannon ∴ glyph is text-weight — a 21px glyph
		     put a 12px figure with 2px dots over a column of 20px line icons. The
		     geometry is the app icon's and ThinkingMark's, exactly: equilateral,
		     side 15, r 3, on the 24-unit box `virtues:logo` uses in icons.ts, so
		     the rail, the chat, and the Dock all show one mark. -->
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

	{#snippet railItem(room: Room)}
		<button
			type="button"
			class="rail-item"
			class:selected={isSelected(room)}
			class:occupied={occupied.has(room.id) && !isSelected(room)}
			aria-label={room.label}
			aria-pressed={isSelected(room)}
			title={`${room.label} · ${room.chord}`}
			onclick={() => activate(room)}
		>
			<span class="rail-tile">
				<AtlasIcon name={room.icon} size={20} stroke={1.0} bare />
			</span>
			<span class="rail-label">{room.label}</span>
		</button>
	{/snippet}

	{#each primary as room (room.id)}{@render railItem(room)}{/each}
	{#each library as room (room.id)}{@render railItem(room)}{/each}

	<div class="rail-spacer" aria-hidden="true"></div>

	{#if gettingStarted.loaded && !gettingStarted.unsupported && !gettingStarted.graduated}
		<SetupTile />
	{/if}

	{#each utility as room (room.id)}{@render railItem(room)}{/each}
</nav>

<style>
	.rail {
		/* Wide enough for "Settings" / "Applets" at 11px under a 40px tile. */
		width: 72px;
		flex: none;
		display: flex;
		flex-direction: column;
		align-items: center;
		/* Top inset places the mark on the same centreline as the panel title
		   and the pane toolbar, which float 12px down.
		
		   Horizontally the padding is 12 LEFT and 0 RIGHT, which looks lopsided
		   in the rule and is symmetric on screen. What sits to the rail's right
		   is never the rail's own edge — it is the panel card (ml-3) when open,
		   or the pane card (m-3) when collapsed, and both supply 12px of desk.
		   So 12/0 puts exactly 12px on either side of the tiles in BOTH states;
		   the old symmetric 6/6 read as 6px to the window edge against 18px to
		   the card. Optical symmetry over arithmetic symmetry. */
		padding: 12px 0 12px 12px;
		gap: 2px;
		overflow: hidden;
		/* The rail is the only thing left on the desk ground — the panel and the
		   pane are the white card beside it — so it paints nothing of its own. */
		background: transparent;
	}

	.rail-mark {
		display: grid;
		place-items: center;
		width: 40px;
		height: var(--chrome-row-h);
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
	   swells in turn, then the apex, more and longer, once. It is the mark
	   saying its own name rather than the opacity dim every link has, and it
	   is ThinkingMark's pulse (1.22 / 1.26, 140ms stagger — under ~100ms the
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

	/* The whole item takes the fill — icon and label together, as one object.
	   The fill used to sit on a 40px square behind the glyph alone, which left
	   the label outside the thing that was selected. */
	.rail-item {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 4px;
		width: 100%;
		padding: 8px 4px;
		border: none;
		border-radius: var(--sidebar-interactive-radius);
		background: none;
		cursor: pointer;
		color: var(--color-foreground-muted);
		transition:
			background var(--sidebar-transition-duration) ease,
			color var(--sidebar-transition-duration) ease;
	}

	.rail-tile {
		display: grid;
		place-items: center;
	}

	.rail-label {
		font-size: 11px;
		font-weight: 500;
		line-height: 1.1;
		letter-spacing: 0.01em;
		white-space: nowrap;
	}

	.rail-item :global(svg) {
		opacity: 0.8;
		transition: opacity var(--sidebar-transition-duration) ease;
	}

	.rail-item:hover {
		color: var(--color-foreground);
		background: var(--sidebar-hover-bg);
	}
	.rail-item:hover :global(svg) { opacity: 1; }

	/* Occupied: a pane holds this room. Full ink, no fill. */
	.rail-item.occupied { color: var(--color-foreground); }
	.rail-item.occupied :global(svg) { opacity: 1; }

	/* Selected: the panel is showing this room. */
	.rail-item.selected {
		color: var(--color-foreground);
		background: var(--active-bg);
	}
	.rail-item.selected :global(svg) { opacity: 1; }

	.rail-item:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: -2px;
	}

	.rail-spacer {
		flex: 1;
		min-height: 12px;
	}
</style>
