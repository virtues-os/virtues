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
	 * One even rhythm for every room; only Settings is set apart, by the spacer
	 * that pushes it to the foot.
	 *
	 * The ∴ mark that toggles the sidebar is not on the rail: it leads the
	 * window header above it (RailMark), so it stays put as the panel moves.
	 *
	 * SETUP IS A ROOM, at the very top, above Home, while any step of Setup is
	 * not done (2026-09-23 as Getting started; Setup since 2026-09-24). It was
	 * a tile at the foot of the spacer that opened a chat; now its panel lists
	 * the steps and sends you back into the full-screen flow. It is the one
	 * tile in primary ink while unselected — the standing call to act, and it
	 * expires — and its glyph is a ring with one arc per step, inked as each
	 * is done, so the label stays the one word. It leaves the rail
	 * once every step is done, which only moves Home up into its place.
	 */
	import AtlasIcon from './AtlasIcon.svelte';
	import SetupRing from './SetupRing.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { sidebarRoom } from '$lib/stores/sidebarRoom.svelte';
	import { sidebarState } from '$lib/stores/sidebarState.svelte';
	import { gettingStarted } from '$lib/stores/gettingStarted.svelte';
	import { setup } from '$lib/components/setup/setup.svelte';
	import { roomForRoute, roomsInGroup, type Room } from '$lib/sidebar/rooms';
	import { appletsStore } from '$lib/stores/applets.svelte';

	const setupRooms = roomsInGroup('setup');

	/** Applets that need you, counted against the room that holds them. */
	function attention(room: Room): number {
		if (room.id === 'home') return appletsStore.problems;
		if (room.id === 'settings') return appletsStore.builtInProblems;
		return 0;
	}
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

	// Skipped is not done: a step set aside keeps the tile, because Finish
	// later and Skip both mean "not now", never "not ever". The count is the
	// flow's own seven, read off the same derived state the flow reads.
	const setupOpen = $derived(gettingStarted.loaded && !gettingStarted.unsupported && !setup.complete);
	const setupCount = $derived(`${setup.doneCount}/${setup.steps.length}`);

	// A room that leaves the rail cannot stay selected: once Setup is done
	// the panel falls back to Home rather than showing steps with no tile.
	$effect(() => {
		if (gettingStarted.loaded && !setupOpen && sidebarRoom.selectedId === 'setup') {
			sidebarRoom.select('home');
		}
	});

	const selectedId = $derived(sidebarRoom.selectedId);
	const panelOpen = $derived(!sidebarState.collapsed);
	const compact = $derived(sidebarState.railCompact);

	function isSelected(room: Room): boolean {
		return room.id === selectedId && panelOpen;
	}

	function activate(room: Room) {
		if (room.panel.kind === 'none') {
			// A room with no panel is its page: open it, or bring forward the
			// tab already holding it, full width. The last room's panel closes,
			// or it would stay open beside a page that has none, with nothing on
			// the page to close it. Only the rail tile does this: switching tabs
			// leaves the sidebar as it was.
			windowShellStore.openTabFromRoute(room.href, { label: room.label, focusExisting: true });
			sidebarState.collapsed = true;
			return;
		}
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

<nav class="rail" class:compact style="width: {sidebarState.railWidth}px" aria-label="Rooms">
	{#snippet railItem(room: Room)}
		<button
			type="button"
			class="rail-item"
			class:selected={isSelected(room)}
			class:occupied={occupied.has(room.id) && !isSelected(room)}
			class:setup={room.group === 'setup'}
			aria-label={room.label}
			aria-pressed={isSelected(room)}
			title={room.group === 'setup'
				? `${room.label} · ${setupCount} done · ${room.chord}`
				: attention(room) > 0
					? `${room.label} · ${attention(room)} ${attention(room) === 1 ? 'applet needs' : 'applets need'} you · ${room.chord}`
					: `${room.label} · ${room.chord}`}
			onclick={() => activate(room)}
		>
			<span class="rail-tile">
				{#if room.group === 'setup'}
					<SetupRing size={20} />
				{:else}
					<AtlasIcon name={room.icon} size={20} stroke={1.0} bare />
				{/if}
				<!-- Your applets live in Home, so one that needs you marks Home;
				     a built-in one marks Settings, where it's kept. Only while
				     something is broken. -->
				{#if attention(room) > 0}
					<span class="rail-problem" aria-hidden="true"></span>
				{/if}
			</span>
			<span class="rail-label" aria-hidden={compact}>{room.label}</span>
		</button>
	{/snippet}

	{#if setupOpen}
		{#each setupRooms as room (room.id)}{@render railItem(room)}{/each}
	{/if}
	{#each primary as room (room.id)}{@render railItem(room)}{/each}
	{#each library as room (room.id)}{@render railItem(room)}{/each}

	<div class="rail-spacer" aria-hidden="true"></div>

	{#each utility as room (room.id)}{@render railItem(room)}{/each}
</nav>

<style>
	.rail {
		/* Width comes from sidebarState. RAIL_WIDTH makes each labelled item
		   square (52 × 52: glyph, gap, label, padding), which still clears
		   the widest label, "Timeline" at 11px; compact is a 36px glyph
		   column of 36px squares. */
		flex: none;
		display: flex;
		flex-direction: column;
		align-items: center;
		/* No top inset: the window header sits above the rail, and the first
		   tile starts level with the top of the cards beside it.

		   Horizontally the padding is 12 LEFT and 0 RIGHT, which looks lopsided
		   in the rule and is symmetric on screen. What sits to the rail's right
		   is never the rail's own edge — it is the panel card (ml-3) when open,
		   or the pane card (m-3) when collapsed, and both supply 12px of desk.
		   So 12/0 puts exactly 12px on either side of the tiles in BOTH states;
		   the old symmetric 6/6 read as 6px to the window edge against 18px to
		   the card. Optical symmetry over arithmetic symmetry. */
		padding: 0 0 12px 12px;
		gap: 6px;
		overflow: hidden;
		/* The rail is the only thing left on the desk ground — the panel and the
		   pane are the white card beside it — so it paints nothing of its own. */
		background: transparent;
		/* The aside's own width transition, exactly: the panel beside the rail
		   is fixed-width, so any difference in timing opens the seam between
		   the panel and the pane for the length of the animation. The labels
		   fade out early and in late, so text is never seen being clipped by
		   the edge it is passing through. */
		transition: width 300ms var(--ease-premium);
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
		/* Compact squares the item to the 36px column: the label's gap and
		   row go to equal padding around the 20px glyph. */
		transition:
			background var(--sidebar-transition-duration) ease,
			color var(--sidebar-transition-duration) ease,
			gap 220ms cubic-bezier(0.2, 0.7, 0.2, 1),
			padding 220ms cubic-bezier(0.2, 0.7, 0.2, 1);
		border: none;
		border-radius: var(--sidebar-interactive-radius);
		background: none;
		cursor: pointer;
		color: var(--color-foreground-muted);
	}

	.compact .rail-item {
		gap: 0;
		padding: 8px 0;
	}

	.rail-tile {
		display: grid;
		place-items: center;
		position: relative;
	}

	/* Off the glyph's top-right corner, not on it: at 8px over the glyph
	   it covered the roof of the Home icon. */
	.rail-problem {
		position: absolute;
		top: -2px;
		right: -4px;
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--color-error);
	}

	.rail-label {
		font-size: 11px;
		font-weight: 500;
		line-height: 1.1;
		letter-spacing: 0.01em;
		white-space: nowrap;
		max-height: 13px;
		/* Opening: fade in once the width has made room. */
		transition:
			opacity 160ms ease 200ms,
			max-height 220ms cubic-bezier(0.2, 0.7, 0.2, 1);
	}

	.compact .rail-label {
		opacity: 0;
		max-height: 0;
		/* Closing: fade out first, then the row collapses. */
		transition:
			opacity 100ms ease,
			max-height 220ms cubic-bezier(0.2, 0.7, 0.2, 1) 60ms;
	}

	@media (prefers-reduced-motion: reduce) {
		.rail,
		.rail-item,
		.rail-label {
			transition: none;
		}
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

	/* Getting started: the one filled object on the rail, in the theme's
	   primary, so an unfinished setup is impossible to miss (Adam, 09-23:
	   "patently obvious"). The text takes the page's own background — white
	   on every light theme, and still legible on a dark theme whose primary
	   is light — the same pairing the letter's button uses, since the themes
	   carry no on-primary token. It expires with setup, so the loudness does
	   too. Selected keeps the fill and adds a ring, because the fill cannot
	   also be the selected state's word here. */
	.rail-item.setup,
	.rail-item.setup:hover,
	.rail-item.setup.occupied {
		color: var(--color-background);
		background: var(--color-primary);
	}
	.rail-item.setup:hover {
		background: color-mix(in srgb, var(--color-primary) 88%, var(--color-foreground));
	}
	.rail-item.setup :global(svg) {
		opacity: 1;
	}
	.rail-item.setup.selected {
		background: var(--color-primary);
		/* Inset: the rail clips at its right edge, so an outer ring would be
		   cut off on one side. */
		box-shadow: inset 0 0 0 2px color-mix(in srgb, var(--color-background) 55%, transparent);
	}
	.rail-item.setup:focus-visible {
		outline-offset: 3px;
	}

	.rail-item:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: -2px;
	}

	.rail-spacer {
		flex: 1;
		min-height: 12px;
	}
</style>
