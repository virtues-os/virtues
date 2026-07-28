<!--
	PinnedSection.svelte

	The "Pinned" rail at the top of the sidebar — the routes the user chose to
	keep. Each pin is a thing/page/day/person/project/external URL. Click
	navigates; right-click → unpin; drag (or ⌥↑/⌥↓) reorders via
	`PUT /api/pins/reorder`.

	THE RIBBON. Each pin can carry a colour, drawn as a 2px bar on the row's
	leading edge — the sewn-in ribbon of a book you've marked. It does four
	jobs at once, which is why it is the one flourish in here:

	  · it's the only colour in the sidebar, so the section stops reading as an
	    extension of the nav below it — your marks against the system's rooms;
	  · it identifies a pin at a glance AND at icon size, which is what makes
	    the icons display mode viable at all (two unlabelled 14px glyphs are
	    indistinguishable; two coloured tiles are not);
	  · it replaces the "PINNED" header, which was an 11px uppercase label
	    heavier than the two-to-six items beneath it;
	  · it comes from the subject — a book you keep your place in — rather than
	    from what other apps' sidebars look like.

	Colours are `--cat-*` token keys, never hex: that palette is already
	documented as non-semantic and already carries a light/dark pair, so a
	ribbon adapts across all sixteen themes for free. This is the one place the
	app's no-non-semantic-colour rule is deliberately broken, and the exception
	is principled — the rule stops the SYSTEM asserting meaning through hue; a
	pin's colour is the owner's own index, where colour means "mine".

	Two display modes, Arc-style: `list` (ribbon + icon + label, one per row)
	and `icons` (a wrapping grid of ribboned tiles, for people who keep enough
	pins that labels stop earning their vertical space). The choice persists in
	localStorage — it's a per-device reading preference, not account state.

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

	/**
	 * The ribbon palette — keys into `--cat-*` in themes.css.
	 *
	 * Deliberately a subset of the twelve: the light/dark siblings
	 * (`*-light`) are there so a chart can shade a series, and offering both
	 * halves of a pair here would mean two swatches most people can't tell
	 * apart. Eight distinguishable hues is more than anyone needs for a list
	 * that tops out around six items.
	 */
	const RIBBON_COLORS = [
		'rose',
		'orange',
		'yellow',
		'emerald',
		'cyan',
		'indigo',
		'violet',
		'pink',
	] as const;

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
				id: 'color',
				label: 'Ribbon',
				icon: 'ri:bookmark-line',
				submenu: [
					{
						id: 'color-none',
						label: 'None',
						icon: pin.color ? 'ri:close-line' : 'ri:check-line',
						action: () => void setColor(pin, null)
					},
					...RIBBON_COLORS.map((c) => ({
						id: `color-${c}`,
						// Capitalised name rather than a swatch-only row: a context
						// menu is a list of words, and a colour with no name can't be
						// read out, searched, or described to anyone.
						label: c.charAt(0).toUpperCase() + c.slice(1),
						icon: pin.color === c ? 'ri:check-line' : 'ri:circle-fill',
						action: () => void setColor(pin, c)
					}))
				]
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
	async function setColor(pin: Pin, color: string | null) {
		try {
			await pinsStore.setColor(pin.id, color);
		} catch {
			/* the store has already put the previous colour back */
		}
	}

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
	     The ribbons say whose these are; a hairline says where they end. -->
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
						class:ribboned={!!pin.color}
						style={pin.color ? `--ribbon: var(--cat-${pin.color})` : undefined}
						draggable="true"
						title={pin.label ?? pin.url}
						ondragstart={(e) => onDragStart(e, index)}
						ondragend={onDragEnd}
						onclick={() => open(pin)}
						onkeydown={(e) => onKeyDown(e, index)}
						oncontextmenu={(e) => showContextMenu(e, pin, index)}
					>
						<Icon icon={pin.icon ?? 'ri:pushpin-line'} width="14" />
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
	/* A hairline instead of a header. The group needs separating from the nav
	   below it, not naming — the ribbons already say whose these are. */
	.pinned-section {
		display: flex;
		flex-direction: column;
		gap: 0.125rem;
		padding: 0.25rem 0.375rem 0.5rem;
		margin-bottom: 0.375rem;
		border-bottom: 1px solid var(--color-border-subtle, var(--color-border));
	}
	.pinned-section.collapsed {
		padding: 0.25rem 0.25rem 0.5rem;
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
		gap: 0.5rem;
		width: 100%;
		padding: 0.3125rem 0.5rem;
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
		padding: 0.375rem;
	}

	/* The ribbon: a bar sewn into the leading edge, not a dot beside the icon.
	   An edge mark scans as a column down the group — you read the set of
	   ribbons in one glance — where dots would just be more icons.
	   Inset rather than a border so the row's own box doesn't shift when a
	   colour is set or cleared. */
	.pin-row.ribboned {
		box-shadow: inset 2px 0 0 0 var(--ribbon);
	}
	.pin-row.ribboned:not(.icon-only) {
		padding-left: calc(0.5rem + 3px);
	}
	/* At icon size the ribbon runs along the bottom instead: a 2px bar on the
	   left of a 26px square reads as a rendering artefact, while an underline
	   reads as a tab marker — and it leaves the glyph centred. */
	.pin-row.ribboned.icon-only {
		box-shadow: inset 0 -2px 0 0 var(--ribbon);
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
