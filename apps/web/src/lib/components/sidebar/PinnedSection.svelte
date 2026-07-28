<!--
	PinnedSection.svelte

	The "Pinned" rail at the top of the sidebar — the routes the user chose to
	keep. Each pin is a thing/page/day/person/project/external URL. Click
	navigates; right-click → unpin; drag (or ⌥↑/⌥↓) reorders via
	`PUT /api/pins/reorder`.

	Pins are deliberately plain. An earlier pass gave each one a coloured
	"ribbon" on its leading edge; it was solving the wrong problem and it looked
	it. Pins don't read as confusing because they lack colour — they read as
	confusing because a pinned "Pages" and a nav "Pages" render identically. That
	is a structural collision, and decoration is not an answer to it. Worse, a
	2px sliver on a 28px row three pixels from a 16px icon isn't identity, it's
	lint. The real fix is that pins should be a different SHAPE from nav rows,
	which is a change worth making properly rather than tinting around.

	Two display modes, Arc-style: `list` (icon + label, one per row) and `icons`
	(a wrapping grid of tiles, for people who keep enough pins that labels stop
	earning their vertical space). The choice persists in localStorage — it's a
	per-device reading preference, not account state.

	NOT "Bookmarks". `data_content_bookmark` already owns that word for
	*ingested* saved links (GitHub stars, browser bookmarks) — a separate
	ontology with its own table and records view. Two user-facing "Bookmarks"
	meaning different things would be worse than the plainer word.

	Also distinct from project pins and `app_notebook_items.role` — these are
	user-global ("always visible"), not scoped to any notebook.
-->

<script lang="ts">
	import { onMount, tick } from 'svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { contextMenu } from '$lib/stores/contextMenu.svelte';
	import Icon from '$lib/components/Icon.svelte';
	import type { Pin } from '$lib/api/client';
	import { pinsStore } from '$lib/stores/pins.svelte';

	interface Props {
		collapsed?: boolean;
	}

	let { collapsed = false }: Props = $props();

	type DisplayMode = 'list' | 'icons';
	const DISPLAY_KEY = 'virtues-pins-display';

	const pins = $derived(pinsStore.pins);

	let display = $state<DisplayMode>('list');

	// A collapsed sidebar has no room for labels, so it renders as icons
	// regardless of the stored preference — without overwriting it, so
	// expanding again returns you to the mode you actually chose.
	const effectiveDisplay = $derived<DisplayMode>(collapsed ? 'icons' : display);

	onMount(() => {
		void pinsStore.load();
		try {
			const saved = localStorage.getItem(DISPLAY_KEY);
			if (saved === 'list' || saved === 'icons') display = saved;
		} catch {
			/* private mode — the default stands */
		}
	});

	function setDisplay(mode: DisplayMode) {
		display = mode;
		try {
			localStorage.setItem(DISPLAY_KEY, mode);
		} catch {
			/* private mode — the choice just doesn't survive a reload */
		}
	}

	function toggleDisplay() {
		setDisplay(display === 'list' ? 'icons' : 'list');
	}

	function open(pin: Pin) {
		if (pin.url.startsWith('http://') || pin.url.startsWith('https://')) {
			window.open(pin.url, '_blank', 'noopener,noreferrer');
			return;
		}
		windowShellStore.openTabFromRoute(pin.url);
	}

	function showContextMenu(e: MouseEvent, pin: Pin, index: number) {
		e.preventDefault();
		e.stopPropagation();
		contextMenu.show({ x: e.clientX, y: e.clientY }, [
			{
				id: 'move-up',
				label: 'Move up',
				icon: 'ri:arrow-up-line',
				disabled: index === 0,
				action: () => void move(index, index - 1)
			},
			{
				id: 'move-down',
				label: 'Move down',
				icon: 'ri:arrow-down-line',
				disabled: index === pins.length - 1,
				action: () => void move(index, index + 1)
			},
			{
				id: 'display',
				label: display === 'list' ? 'Show as icons' : 'Show as list',
				icon: display === 'list' ? 'ri:grid-fill' : 'ri:list-unordered',
				action: () => toggleDisplay()
			},
			{
				id: 'unpin',
				label: 'Unpin',
				icon: 'ri:pushpin-fill',
				action: async () => {
					await pinsStore.remove(pin.id);
				}
			}
		]);
	}

	// ── Reorder ──────────────────────────────────────────────────────────────
	// Drag for pointers, ⌥↑/⌥↓ for keyboards. The keyboard path is not a
	// courtesy: drag-and-drop has no keyboard equivalent of its own, so
	// without it reordering is simply unavailable to anyone not using a mouse
	// — and unreachable on touch, where HTML5 drag events don't fire at all.
	// The context menu carries the same two moves for that reason.

	let dragIndex = $state<number | null>(null);
	let overIndex = $state<number | null>(null);
	let listEl = $state<HTMLUListElement | null>(null);

	/**
	 * `keepFocus` restores focus to the row at its new index.
	 *
	 * The `{#each}` is keyed, so Svelte *moves* the existing node rather than
	 * recreating it — and moving a focused element in the DOM blurs it. Without
	 * this, ⌥↓ works exactly once: the row moves, focus lands on <body>, and a
	 * second press does nothing. Which makes moving an item two places
	 * impossible by keyboard, i.e. the keyboard path is only notionally there.
	 */
	async function move(from: number, to: number, keepFocus = false) {
		if (to < 0 || to >= pins.length) return;
		try {
			await pinsStore.reorder(from, to);
			if (keepFocus) {
				await tick();
				const rows = listEl?.querySelectorAll<HTMLElement>('.pin-row');
				rows?.[to]?.focus();
			}
		} catch {
			/* the store has already rolled the order back */
		}
	}

	function onDragStart(e: DragEvent, index: number) {
		dragIndex = index;
		if (!e.dataTransfer) return;
		e.dataTransfer.effectAllowed = 'move';
		// Firefox refuses to start a drag unless some payload is set.
		e.dataTransfer.setData('text/plain', String(index));
	}

	function onDragOver(e: DragEvent, index: number) {
		if (dragIndex === null) return;
		e.preventDefault();
		if (e.dataTransfer) e.dataTransfer.dropEffect = 'move';
		overIndex = index;
	}

	function onDrop(e: DragEvent, index: number) {
		e.preventDefault();
		const from = dragIndex;
		dragIndex = null;
		overIndex = null;
		if (from === null) return;
		void move(from, index);
	}

	function onDragEnd() {
		dragIndex = null;
		overIndex = null;
	}

	function onKeyDown(e: KeyboardEvent, index: number) {
		if (!e.altKey) return;
		if (e.key === 'ArrowUp') {
			e.preventDefault();
			void move(index, index - 1, true);
		} else if (e.key === 'ArrowDown') {
			e.preventDefault();
			void move(index, index + 1, true);
		}
	}
</script>

{#if pins.length > 0}
	<!-- No section header. "PINNED" in 11px uppercase with 0.06em tracking was
	     a heavier label than the two-to-six items under it, and it made the
	     group read as another system section rather than as the user's own.
	     Space says where the group ends; that is all it needs. -->
	<div class="pinned-section" class:collapsed aria-label="Pinned">
		<ul bind:this={listEl} class="pin-list" class:icons={effectiveDisplay === 'icons'}>
			{#each pins as pin, index (pin.id)}
				<li
					class:drop-target={overIndex === index && dragIndex !== index}
					class:dragging={dragIndex === index}
					ondragover={(e) => onDragOver(e, index)}
					ondrop={(e) => onDrop(e, index)}
				>
					<button
						type="button"
						class="pin-row"
						class:icon-only={effectiveDisplay === 'icons'}
						draggable="true"
						title={pin.label ?? pin.url}
						ondragstart={(e) => onDragStart(e, index)}
						ondragend={onDragEnd}
						onclick={() => open(pin)}
						onkeydown={(e) => onKeyDown(e, index)}
						oncontextmenu={(e) => showContextMenu(e, pin, index)}
					>
						<Icon icon={pin.icon ?? 'ri:pushpin-line'} width="16" class="sidebar-icon" />
						{#if effectiveDisplay === 'list'}
							<span class="pin-label">{pin.label ?? pin.url}</span>
						{/if}
					</button>
				</li>
			{/each}
		</ul>
	</div>
{/if}

<style>
	/* Space, not a rule. Separation goes whitespace first, and you stop at the
	   first thing that reads — a 1px line is the reflex that produces a boxed,
	   machine-made panel. Nothing else in this sidebar draws a rule; this group
	   doesn't need to be the exception. */
	.pinned-section {
		display: flex;
		flex-direction: column;
		gap: var(--sidebar-item-gap);
		/* No horizontal padding: .workspace-nav already insets its children by
		   8px, and .system-section (the nav rows below) adds none. Adding 8px
		   here double-inset the pins, putting them 8px right of the rows they
		   stack above — the kind of near-miss the eye reads as sloppiness
		   without being able to name it. */
		padding: 0;
		margin-bottom: 16px;
	}
	.pinned-section.collapsed {
		padding: 0;
	}

	.pin-list {
		list-style: none;
		padding: 0;
		margin: 0;
		display: flex;
		flex-direction: column;
		gap: 1px;
	}
	.pin-list.icons {
		flex-direction: row;
		flex-wrap: wrap;
		gap: 2px;
	}

	.pin-list li {
		border-radius: 4px;
	}
	.pin-list li.dragging {
		opacity: 0.4;
	}
	/* The insertion point reads as a rule on the leading edge — a full-row
	   highlight would be ambiguous about whether the drop lands above or on
	   the row under the cursor. */
	.pin-list li.drop-target {
		box-shadow: inset 0 2px 0 0 var(--color-primary, currentColor);
	}
	.pin-list.icons li.drop-target {
		box-shadow: inset 2px 0 0 0 var(--color-primary, currentColor);
	}

	.pin-row {
		display: flex;
		align-items: center;
		gap: var(--sidebar-interactive-gap);
		width: 100%;
		height: var(--sidebar-interactive-height);
		padding: 0 var(--sidebar-padding-left-base);
		background: transparent;
		border: none;
		border-radius: 4px;
		cursor: pointer;
		font: inherit;
		font-size: 0.8125rem;
		color: var(--color-foreground, inherit);
		text-align: left;
	}
	.pin-row.icon-only {
		width: auto;
		justify-content: center;
		padding: 0 8px;
	}


	/* The interaction ramp, not --color-background-hover. That token resolves
	   to --surface-elevated, which sits ~3% off --background in the light
	   themes — on the sidebar's --background parent it read as nothing
	   happening at all. */
	.pin-row:hover {
		background: var(--hover-bg);
	}
	.pin-row:active {
		background: var(--active-bg);
	}
	.pin-row:focus-visible {
		outline: 2px solid var(--color-border-focus, currentColor);
		outline-offset: -2px;
	}

	.pin-label {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

</style>
