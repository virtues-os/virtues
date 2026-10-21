<script lang="ts">
	/**
	 * The panel — one frame, whatever room the sidebar is showing (`sidebarRoom`).
	 *
	 * Every room gets the same head so the swap reads as one object changing its
	 * contents rather than seven different sidebars: a title row at
	 * `--chrome-row-h` (level with the top row of the pane across the seam), the room's
	 * name in the serif — the shell's one editorial voice — led by a back arrow
	 * to Home in every room but Home, then the body.
	 *
	 * The title is TEXT, not a control. An earlier version made it a button that
	 * opened the room's full page, but a heading that silently navigates is a
	 * surprise, and you stepped into the room to get here.
	 *
	 * The collapse control is NOT here — it is a button in the window header. Putting
	 * it on the panel meant the control vanished with the thing it hid, so the
	 * way back had to be a different affordance somewhere else. The mark is one
	 * object doing one job from a place that never moves.
	 */
	import SidebarModePanel from './SidebarModePanel.svelte';
	import HomePanel from './panels/HomePanel.svelte';
	import SetupPanel from '$lib/components/setup/SetupPanel.svelte';
	import AtlasIcon from './AtlasIcon.svelte';
	import { sidebarRoom } from '$lib/stores/sidebarRoom.svelte';
	import { DEFAULT_ROOM_ID } from '$lib/sidebar/rooms';
	import { SIDEBAR_MODES } from '$lib/sidebar/modes';
	import type { Room } from '$lib/sidebar/rooms';

	interface Props {
		room: Room;
	}

	let { room }: Props = $props();

	const mode = $derived(room.panel.kind === 'rows' ? SIDEBAR_MODES[room.panel.modeId] : null);
	const inRoom = $derived(room.id !== DEFAULT_ROOM_ID);

</script>

<div class="panel">
	<div class="panel-head" class:in-room={inRoom}>
		{#if inRoom}
			<!-- A room's list is stepped into from Home; this steps back out.
			     The arrow sits on the rows' icon line and the title on their
			     label line. -->
			<button
				type="button"
				class="panel-back"
				aria-label="Back to Home"
				title="Back to Home"
				onclick={() => sidebarRoom.leave()}
			>
				<AtlasIcon name="back" size={16} bare />
			</button>
		{/if}
		<span class="panel-title">{room.title ?? room.label}</span>
		<!-- The head used to carry a room's quick-add (+). The verbs live as
		     doors in the Home panel now, where they are read as words. -->
	</div>

	<div class="panel-body">
		{#if room.panel.kind === 'home'}
			<HomePanel />
		{:else if room.panel.kind === 'setup'}
			<SetupPanel />
		{:else if mode}
			<SidebarModePanel {mode} />
		{/if}
	</div>
</div>

<style>
	.panel {
		display: flex;
		flex-direction: column;
		height: 100%;
		min-height: 0;
	}

	.panel-head {
		display: flex;
		align-items: center;
		gap: 6px;
		height: var(--chrome-row-h);
		flex: none;
		/* One left edge with the rows below: the body insets 4px and every row
		   12px inside it, so the title starts at 16px and the whole column
		   shares one spine. */
		padding-left: 16px;
		padding-right: 8px;
	}

	/* The back control's 28px box puts its glyph at 16px, like the rows'
	   icons; the title then starts at 40px, like their labels. */
	.panel-head.in-room {
		padding-left: 10px;
		gap: 2px;
	}

	.panel-back {
		display: grid;
		place-items: center;
		width: 28px;
		height: 28px;
		flex: none;
		padding: 0;
		border: none;
		border-radius: var(--sidebar-interactive-radius);
		background: none;
		color: var(--color-foreground-muted);
		cursor: pointer;
		transition:
			background-color var(--motion-hover-out) var(--ease-premium),
			color var(--motion-hover-out) var(--ease-premium);
	}

	.panel-back:hover {
		background: var(--hover-bg);
		color: var(--color-foreground);
		transition-duration: var(--motion-hover-in);
	}

	.panel-back:active {
		background: var(--press-bg);
		transition-duration: var(--motion-press);
	}

	.panel-back:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: -2px;
	}

	@media (prefers-reduced-motion: reduce) {
		.panel-back {
			transition: none;
		}
	}

	/* The shell's one editorial voice, said once. Serif, regular — never bold. */
	.panel-title {
		flex: 1;
		min-width: 0;
		font-family: var(--font-serif);
		font-size: 16px;
		font-weight: 400;
		line-height: 1.2;
		color: var(--color-foreground);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.panel-body {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		overflow-x: hidden;
		padding: 4px 4px 12px;
	}

</style>
