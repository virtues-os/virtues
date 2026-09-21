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
	 */
	import AtlasIcon from './AtlasIcon.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { sidebarRoom } from '$lib/stores/sidebarRoom.svelte';
	import { sidebarState } from '$lib/stores/sidebarState.svelte';
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
	>∴</button>

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
		font-family: var(--font-serif);
		font-size: 21px;
		font-weight: 400;
		color: var(--color-foreground);
	}

	.rail-mark-btn {
		border: none;
		background: none;
		padding: 0;
		cursor: pointer;
		transition: opacity var(--sidebar-transition-duration) ease;
	}

	.rail-mark-btn:hover { opacity: 0.6; }

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
		background: color-mix(in srgb, var(--color-foreground) 12%, transparent);
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
