<script lang="ts">
	/**
	 * The panel — one frame, whatever room the rail has selected.
	 *
	 * Every room gets the same head so the swap reads as one object changing its
	 * contents rather than seven different sidebars: a title row at
	 * `--chrome-row-h` (level with the pane toolbar across the seam), the room's
	 * name in the serif — the shell's one editorial voice — then the room's
	 * actions and the collapse control, then the body.
	 *
	 * The title is TEXT, not a control. An earlier version made it a button that
	 * opened the room's full page, but a heading that silently navigates is a
	 * surprise, and the rail already selects the room.
	 *
	 * The collapse control is NOT here — it is the ∴ mark on the rail. Putting
	 * it on the panel meant the control vanished with the thing it hid, so the
	 * way back had to be a different affordance somewhere else. The mark is one
	 * object doing one job from a place that never moves.
	 */
	import AtlasIcon from './AtlasIcon.svelte';
	import SidebarModePanel from './SidebarModePanel.svelte';
	import ChatsPanel from './panels/ChatsPanel.svelte';
	import DeskSection from './DeskSection.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { SIDEBAR_MODES } from '$lib/sidebar/modes';
	import type { Room } from '$lib/sidebar/rooms';

	interface Props {
		room: Room;
	}

	let { room }: Props = $props();

	const mode = $derived(room.panel.kind === 'rows' ? SIDEBAR_MODES[room.panel.modeId] : null);

	function openRoom() {
		windowShellStore.openTabFromRoute(room.href, {
			label: room.label,
			focusExisting: true,
		});
	}

	async function quickAdd() {
		if (room.quickAdd === 'chat') {
			windowShellStore.openTabFromRoute('/', { label: 'New Chat', forceNew: true });
			return;
		}
		if (room.quickAdd === 'page') {
			const { pagesStore } = await import('$lib/stores/pages.svelte');
			const page = await pagesStore.createNewPage();
			windowShellStore.openTabFromRoute(`/page/${page.id}`, {
				label: page.title,
				forceNew: true,
			});
		}
	}
</script>

<div class="panel">
	<div class="panel-head">
		<span class="panel-title">{room.label}</span>
		<div class="panel-actions">
			{#if room.quickAdd}
				<button
					type="button"
					class="panel-icon-btn"
					aria-label={room.quickAdd === 'chat' ? 'New chat' : 'New page'}
					title={room.quickAdd === 'chat' ? 'New chat' : 'New page'}
					onclick={quickAdd}
				>
					<AtlasIcon name={room.quickAdd === 'chat' ? 'new-chat' : 'pages'} size={16} bare />
				</button>
			{/if}
		</div>
	</div>

	<div class="panel-body">
		{#if room.panel.kind === 'desk'}
			<DeskSection headless />
		{:else if room.panel.kind === 'chats'}
			<ChatsPanel />
		{:else if mode}
			<SidebarModePanel {mode} />
		{:else}
			<!-- Not built yet, and saying so is better than an empty column that
			     reads as a room with nothing in it. -->
			<button type="button" class="panel-stub" onclick={openRoom}>
				Open {room.label}
			</button>
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

	.panel-actions {
		display: flex;
		align-items: center;
		gap: 2px;
		flex: none;
	}

	.panel-icon-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 28px;
		height: 28px;
		border: none;
		border-radius: var(--sidebar-interactive-radius);
		background: none;
		cursor: pointer;
		color: var(--color-foreground-muted);
	}

	.panel-icon-btn :global(svg) {
		opacity: var(--sidebar-icon-opacity);
		transition: opacity var(--sidebar-transition-duration) ease;
	}

	.panel-icon-btn:hover {
		background: var(--sidebar-hover-bg);
		color: var(--color-foreground);
	}

	.panel-icon-btn:hover :global(svg) { opacity: 1; }

	.panel-icon-btn:focus-visible,
	.panel-stub:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: -2px;
	}

	.panel-body {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		overflow-x: hidden;
		padding: 4px 4px 12px;
	}

	.panel-stub {
		display: flex;
		align-items: center;
		width: 100%;
		height: var(--sidebar-interactive-height);
		padding: 0 12px;
		border: none;
		border-radius: var(--sidebar-interactive-radius);
		background: none;
		cursor: pointer;
		text-align: left;
		font-size: var(--sidebar-interactive-font-size);
		color: var(--color-foreground-muted);
	}

	.panel-stub:hover {
		background: var(--sidebar-hover-bg);
		color: var(--color-foreground);
	}
</style>
