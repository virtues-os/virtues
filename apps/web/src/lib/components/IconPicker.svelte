<script lang="ts">
	/**
	 * IconPicker - the mark a page, chat, pin or project wears
	 *
	 * Shapes and colors lead: a solid shape in a color is the plainest mark
	 * and the one that shows its color best. Line icons and emoji sit below
	 * behind one search. Use inside a Popover primitive for positioning and
	 * dismiss behavior.
	 */
	import Icon from './Icon.svelte';
	import Button from './Button.svelte';
	import MenuItem from './MenuItem.svelte';
	import { addCollection } from '@iconify/svelte';
	import { onMount } from 'svelte';
	import { PIN_COLORS, accentCss } from '$lib/sidebar/pin-colors';
	import { SHAPES } from '$lib/icons';
	import { GLYPHS as ATLAS_GLYPHS } from '$lib/components/sidebar/AtlasIcon.svelte';

	interface Props {
		/** Current value (emoji or icon name) */
		value?: string | null;
		/** Called when an icon/emoji is selected */
		onSelect: (value: string | null) => void;
		/** Close the popover */
		close: () => void;
		/** Whether to show the "Remove icon" option (default: true) */
		showRemove?: boolean;
		/**
		 * The icon's color: a `--cat-*` token key, or null for "inherit the text
		 * around it". Only meaningful with `onColorSelect`.
		 */
		color?: string | null;
		/**
		 * Set the icon's color. Omit it and the swatch row doesn't render — the
		 * picker is opened from places whose entity has nowhere to store one
		 * (and offering a control that silently forgets is worse than not
		 * offering it).
		 *
		 * Fires immediately and does NOT close: picking a color is a preview you
		 * want to see against the grid before choosing the icon, which is the
		 * whole reason the two live in one panel.
		 */
		onColorSelect?: (color: string | null) => void;
	}

	let {
		value = null,
		onSelect,
		close,
		showRemove = true,
		color = null,
		onColorSelect,
	}: Props = $props();

	let search = $state('');
	let activeTab = $state<'icons' | 'emoji'>('icons');

	// Local echo of the chosen color so the grid tints on click without waiting
	// for the parent to persist and flow a new prop back down.
	let activeColor = $state<string | null>(color);
	$effect(() => {
		activeColor = color;
	});

	// The color row stays put on both tabs. It colors the shapes beside it,
	// and hiding it on the emoji tab made the whole panel jump on switch.
	const colorRowVisible = $derived(!!onColorSelect);
	// A custom hex is anything that isn't one of the nine keys.
	const isCustom = $derived(!!activeColor && activeColor.startsWith('#'));
	const gridTint = $derived(activeColor ? `color: ${accentCss(activeColor)}` : '');

	function pickColor(key: string | null) {
		activeColor = key;
		onColorSelect?.(key);
	}

	let searchInputEl: HTMLInputElement;

	// Full Remix Icons collection (lazy-loaded)
	let fullCollectionLoaded = $state(false);
	let fullCollectionLoading = $state(false);
	let allRiIconNames = $state<string[]>([]);
	let allIconsPage = $state(0);
	const ALL_ICONS_PAGE_SIZE = 200;

	// The full Unicode set, lazy-loaded with its names and keywords so search
	// can find "dog" rather than only the pasted glyph. Emoji newer than 15.1
	// are left out: older systems, and the kiosk's emoji font, draw them as
	// empty boxes. Skin-tone variants are left out too; the base glyph is the
	// icon.
	const EMOJI_MAX_VERSION = 15.1;
	const EMOJI_COMPONENT_GROUP = 2;
	// emojibase groups, objects and symbols first: they label a page or a
	// project, and the faces that lead Unicode's own order mostly don't.
	const EMOJI_GROUP_ORDER = [7, 8, 6, 5, 3, 4, 0, 1, 9];
	type EmojiEntry = { emoji: string; label: string; terms: string };
	let allEmojis = $state<EmojiEntry[]>([]);
	let emojisLoading = $state(false);

	// The curated line set: the icons people reach for to label a page or a
	// project. Line only. A filled glyph beside the thin Atlas rows reads as a
	// different app, so `-fill` icons and the brand marks are left out here and
	// in the full collection below.
	const curatedIcons = [
		'ri:file-text-line', 'ri:file-line', 'ri:file-list-3-line', 'ri:folder-line',
		'ri:folder-open-line', 'ri:book-line', 'ri:book-open-line', 'ri:quill-pen-line',
		'ri:edit-line', 'ri:double-quotes-l', 'ri:translate-2', 'ri:chat-1-line',
		'ri:message-3-line', 'ri:mail-line', 'ri:send-plane-line', 'ri:lightbulb-line',
		'ri:magic-line', 'ri:bookmark-line', 'ri:heart-line', 'ri:heart-pulse-line',
		'ri:lock-line', 'ri:link', 'ri:price-tag-3-line', 'ri:box-3-line', 'ri:palette-line',
		'ri:seedling-line', 'ri:cloud-line', 'ri:moon-line', 'ri:compass-3-line',
		'ri:map-pin-line', 'ri:map-2-line', 'ri:global-line', 'ri:footprint-line',
		'ri:run-line', 'ri:bar-chart-line', 'ri:line-chart-line', 'ri:table-line',
		'ri:node-tree', 'ri:user-line', 'ri:user-star-line', 'ri:calendar-line',
		'ri:calendar-event-line', 'ri:time-line', 'ri:history-line', 'ri:image-line',
		'ri:movie-line', 'ri:music-line', 'ri:mic-line', 'ri:terminal-box-line',
		'ri:bug-line', 'ri:tools-line', 'ri:cpu-line', 'ri:robot-line', 'ri:plug-line',
		'ri:building-line', 'ri:bank-line', 'ri:wallet-line', 'ri:flag-line',
		'ri:star-line', 'ri:flashlight-line', 'ri:plant-line', 'ri:sun-line',
		'ri:home-line', 'ri:graduation-cap-line', 'ri:flask-line', 'ri:restaurant-line',
		'ri:shopping-bag-line', 'ri:plane-line', 'ri:gift-line', 'ri:trophy-line'
	];

	// Atlas leads: the shell's own glyphs, so a page can wear the same mark as
	// the sidebar row that opens its kind.
	const allIcons = [...Object.keys(ATLAS_GLYPHS).map((n) => `atlas:${n}`), ...curatedIcons];

	// One flat pool. Atlas and the curated set lead and the rest of the line
	// collection follows, deduped. The Remix names wait for the collection to
	// load: drawn before it registers, iconify fetches each one from its API.
	const searchableIcons = $derived(
		fullCollectionLoaded
			? [...allIcons, ...allRiIconNames.filter((i) => !allIcons.includes(i))]
			: allIcons.filter((i) => i.startsWith('atlas:'))
	);

	const visibleIcons = $derived.by(() => {
		const q = search.trim().toLowerCase();
		if (q) return searchableIcons.filter((i) => i.toLowerCase().includes(q));
		// Unsearched, the sheet grows a page at a time — 8k icons in one DOM
		// pass is a locked panel on open.
		return searchableIcons.slice(0, (allIconsPage + 1) * ALL_ICONS_PAGE_SIZE);
	});

	const hasMoreIcons = $derived(
		!search && visibleIcons.length < searchableIcons.length
	);

	// Every word must match a name or keyword, so "red heart" narrows rather
	// than widens. Pasting the emoji itself also finds it.
	const visibleEmojis = $derived.by(() => {
		const q = search.trim().toLowerCase();
		if (!q) return allEmojis;
		const words = q.split(/\s+/);
		return allEmojis.filter(
			(e) => e.emoji.includes(q) || words.every((w) => e.terms.includes(w))
		);
	});

	async function loadEmojis() {
		if (allEmojis.length || emojisLoading) return;
		emojisLoading = true;
		try {
			const { default: data } = await import('emojibase-data/en/data.json');
			allEmojis = data
				.filter(
					(e) =>
						e.group !== undefined &&
						e.group !== EMOJI_COMPONENT_GROUP &&
						e.version <= EMOJI_MAX_VERSION
				)
				.sort(
					(a, b) =>
						EMOJI_GROUP_ORDER.indexOf(a.group!) - EMOJI_GROUP_ORDER.indexOf(b.group!) ||
						(a.order ?? 0) - (b.order ?? 0)
				)
				.map((e) => ({
					emoji: e.emoji,
					label: e.label,
					terms: [e.label, ...(e.tags ?? [])].join(' ').toLowerCase()
				}));
		} catch (e) {
			console.error('Failed to load emoji set:', e);
		} finally {
			emojisLoading = false;
		}
	}

	// Lazy-load the full Remix Icons collection
	async function loadFullCollection() {
		if (fullCollectionLoaded || fullCollectionLoading) return;
		fullCollectionLoading = true;
		try {
			const { icons: riData } = await import('@iconify-json/ri');
			const parsed = typeof riData === 'string' ? JSON.parse(riData) : riData;
			addCollection(parsed);
			allRiIconNames = Object.keys(parsed.icons)
				.filter((n) => !n.endsWith('-fill'))
				.map((n) => `ri:${n}`)
				.sort();
			fullCollectionLoaded = true;
		} catch (e) {
			console.error('Failed to load full icon collection:', e);
		} finally {
			fullCollectionLoading = false;
		}
	}

	// Load recent icons on mount
	onMount(() => {
		// Focus search input
		setTimeout(() => searchInputEl?.focus(), 50);

		// Start loading the full icon collection and the emoji set
		loadFullCollection();
		loadEmojis();
	});

	function handleSelect(icon: string) {
		onSelect(icon);
		close();
	}

	function handleRemove() {
		onSelect(null);
		close();
	}
</script>

<div class="icon-picker">
	<!-- Shapes first, colors under them. Picking a color repaints the shapes
	     and the icon grid without closing, so you see the pairing before you
	     choose the mark. Picking a shape closes, like any other mark. -->
	<div class="mark-head" style={gridTint}>
		<div class="shape-row" role="group" aria-label="Shapes">
			{#each SHAPES as shape (shape.name)}
				<button
					class="shape-btn"
					class:current={value === `shape:${shape.name}`}
					onclick={() => handleSelect(`shape:${shape.name}`)}
					title={shape.label}
					aria-label={shape.label}
				>
					<Icon icon="shape:{shape.name}" width="20" />
				</button>
			{/each}
		</div>

		{#if colorRowVisible}
			<!-- The tick goes INSIDE the chosen swatch: at this size a ring reads
			     as a slightly bigger circle, which nobody spots in a row of eleven. -->
			<div class="color-row" role="radiogroup" aria-label="Color">
				<button
					class="swatch swatch-auto"
					class:selected={!activeColor}
					onclick={() => pickColor(null)}
					role="radio"
					aria-checked={!activeColor}
					title="No color - follows the text around it"
					aria-label="No color"
				>
					{#if !activeColor}
						<Icon icon="ri:check-line" width="14" />
					{/if}
				</button>
				{#each PIN_COLORS as { key, label } (key)}
					<button
						class="swatch"
						class:selected={activeColor === key}
						style="--swatch: var(--cat-{key})"
						onclick={() => pickColor(key)}
						role="radio"
						aria-checked={activeColor === key}
						title={label}
						aria-label={label}
					>
						{#if activeColor === key}
							<Icon icon="ri:check-line" width="14" />
						{/if}
					</button>
				{/each}

				<!-- Custom. The hex is stored verbatim, but `accentCss` pulls its
				     LIGHTNESS into the current theme's legible band before drawing
				     it, so a navy picked on paper stays visible on a black theme.
				     Hue and chroma, the part actually chosen, are untouched. It
				     shows a plain "+" until a color is chosen. -->
				<label
					class="swatch swatch-custom"
					class:selected={isCustom}
					style={isCustom ? `--swatch: ${accentCss(activeColor)}` : ''}
					title="Custom color"
				>
					<input
						type="color"
						value={isCustom ? (activeColor ?? '#6366f1') : '#6366f1'}
						oninput={(e) => pickColor(e.currentTarget.value)}
					/>
					<Icon icon={isCustom ? 'ri:check-line' : 'ri:add-line'} width="14" />
				</label>
			</div>
		{/if}
	</div>

	<!-- One search, with the icon/emoji switch beside it: it only changes what
	     the grid below shows, so it sits with the grid instead of over the
	     whole panel. -->
	<div class="picker-search">
		<input
			type="text"
			bind:value={search}
			bind:this={searchInputEl}
			placeholder={activeTab === 'emoji' ? 'Search emoji' : 'Search icons'}
			class="search-input"
		/>
		<div class="kind-toggle" role="tablist" aria-label="Kind">
			<button
				role="tab"
				aria-selected={activeTab === 'icons'}
				class:active={activeTab === 'icons'}
				onclick={() => (activeTab = 'icons')}
			>
				Icons
			</button>
			<button
				role="tab"
				aria-selected={activeTab === 'emoji'}
				class:active={activeTab === 'emoji'}
				onclick={() => (activeTab = 'emoji')}
			>
				Emoji
			</button>
		</div>
	</div>

	<!-- Content. One flat grid, no category headings: the headings turned a
	     dense sheet of glyphs into a scroll of small sections, and nobody
	     browses icons by the taxonomy someone else chose — they scan, or they
	     type. -->
	<div class="picker-content">
		{#if activeTab === 'icons'}
			<div class="icon-grid" style={gridTint}>
				{#each visibleIcons as icon (icon)}
					<button
						class="icon-btn"
						class:current={value === icon}
						onclick={() => handleSelect(icon)}
						title={icon.slice(icon.indexOf(':') + 1)}
					>
						<Icon {icon} width="16" />
					</button>
				{/each}
			</div>
			{#if visibleIcons.length === 0}
				<div class="empty">No icons found</div>
			{:else if hasMoreIcons && !search}
				<Button
					variant="secondary"
					size="sm"
					class="w-full mt-2"
					onclick={() => allIconsPage++}
				>
					Load more
				</Button>
			{/if}
		{:else}
			<div class="emoji-grid">
				{#each visibleEmojis as { emoji, label } (emoji)}
					<button
						class="emoji-btn"
						class:current={value === emoji}
						onclick={() => handleSelect(emoji)}
						title={label}
						aria-label={label}
					>
						{emoji}
					</button>
				{/each}
			</div>
			{#if emojisLoading}
				<div class="empty">Loading emojis...</div>
			{:else if visibleEmojis.length === 0}
				<div class="empty">No emojis found</div>
			{/if}
		{/if}
	</div>

	<!-- Footer with remove option (conditional) -->
	{#if showRemove && value}
		<div class="picker-footer">
			<MenuItem
				icon="ri:delete-bin-line"
				label="Remove icon"
				destructive
				onclick={handleRemove}
			/>
		</div>
	{/if}
</div>

<style>
	.icon-picker {
		width: 344px;
		/* Never taller than the window it floats in. A fixed 480px panel in a
		   short window gets flipped by the positioner and then hangs off the
		   top edge, which is how the whole picker ends up showing you nothing
		   but its footer. */
		max-height: min(480px, calc(100vh - 24px));
		display: flex;
		flex-direction: column;
		overflow: hidden;
	}

	/* Shapes and colors share one block with no rule between them: they are
	   one decision, the mark and its color. Unchosen, the shapes are drawn in
	   muted ink; the chosen color reaches them through the inline `color`. */
	.mark-head {
		display: flex;
		flex-direction: column;
		gap: 10px;
		padding: 12px 12px 12px;
		border-bottom: 1px solid var(--color-border);
		color: var(--color-foreground-muted);
	}

	.shape-row {
		display: grid;
		grid-template-columns: repeat(6, 1fr);
		gap: 4px;
	}

	.shape-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		height: 36px;
		padding: 0;
		border: none;
		border-radius: 8px;
		background: var(--color-surface-elevated, transparent);
		box-shadow: inset 0 0 0 1px var(--color-border);
		color: inherit;
		cursor: pointer;
		transition: background 100ms, box-shadow 100ms;
	}

	.shape-btn:hover {
		background: var(--color-surface-overlay);
	}

	.shape-btn.current {
		box-shadow: inset 0 0 0 1.5px var(--color-foreground-muted);
	}

	/* One row, sized to fit and spread by the row rather than by a fixed gap,
	   so the last swatch can't fall off the panel edge. */
	.color-row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 4px;
		padding: 0 2px;
	}

	.swatch {
		width: 22px;
		height: 22px;
		flex: none;
		padding: 0;
		border: none;
		border-radius: 50%;
		background: var(--swatch);
		color: #fff;
		cursor: pointer;
		display: flex;
		align-items: center;
		justify-content: center;
		box-shadow: inset 0 0 0 1px rgb(0 0 0 / 0.08);
		transition: transform 120ms ease;
	}

	.swatch:hover {
		transform: scale(1.1);
	}

	.swatch.selected {
		transform: none;
	}

	/* Custom and "no color" are both hairline rings until chosen, so the row
	   reads as nine colors with a control at each end. */
	.swatch-custom {
		position: relative;
		overflow: hidden;
		background: var(--swatch, transparent);
		color: var(--color-foreground-muted);
		box-shadow: inset 0 0 0 1.5px var(--color-border);
	}

	.swatch-custom.selected {
		color: #fff;
		box-shadow: inset 0 0 0 1px rgb(0 0 0 / 0.08);
	}

	.swatch-custom input[type='color'] {
		position: absolute;
		inset: 0;
		width: 100%;
		height: 100%;
		padding: 0;
		border: none;
		opacity: 0;
		cursor: pointer;
	}

	.swatch-auto {
		background: transparent;
		color: var(--color-foreground-muted);
		box-shadow: inset 0 0 0 1.5px var(--color-border);
	}

	.swatch-auto.selected {
		box-shadow: inset 0 0 0 1.5px var(--color-foreground-muted);
	}

	.picker-search {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 8px 8px 8px 14px;
		border-bottom: 1px solid var(--color-border);
	}

	.search-input {
		flex: 1;
		min-width: 0;
		background: none;
		border: none;
		outline: none;
		font-size: 13px;
		color: var(--color-foreground);
	}

	.search-input::placeholder {
		color: var(--color-foreground-subtle);
	}

	/* A quiet two-way switch in ink, not accent-colored tabs: the accent
	   would compete with the swatches for the eye. */
	.kind-toggle {
		display: flex;
		flex: none;
		padding: 2px;
		border-radius: 7px;
		background: var(--color-surface-overlay);
	}

	.kind-toggle button {
		padding: 3px 9px;
		font-size: 12px;
		line-height: 16px;
		color: var(--color-foreground-muted);
		background: none;
		border: none;
		border-radius: 5px;
		cursor: pointer;
	}

	.kind-toggle button:hover {
		color: var(--color-foreground);
	}

	.kind-toggle button.active {
		color: var(--color-foreground);
		background: var(--color-background);
		box-shadow: 0 0 0 1px var(--color-border);
	}

	.picker-content {
		flex: 1;
		overflow-y: auto;
		padding: 8px 10px 10px;
		min-height: 200px;
		max-height: 300px;
	}

	.emoji-grid {
		display: grid;
		grid-template-columns: repeat(9, 1fr);
		gap: 2px;
	}

	/* Dense sheet, not a list of buttons, so the eye scans a block of glyphs
	   the way it scans a contact sheet. */
	.icon-grid {
		display: grid;
		grid-template-columns: repeat(9, 1fr);
		gap: 2px;
		color: var(--color-foreground-muted);
	}

	.emoji-btn,
	.icon-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 100%;
		aspect-ratio: 1;
		padding: 0;
		background: none;
		border: none;
		border-radius: 7px;
		cursor: pointer;
		transition: background 100ms;
	}

	.emoji-btn {
		font-size: 19px;
		line-height: 1;
	}

	/* `inherit`, NOT a color of its own. The chosen tint is set on the grid
	   and reaches the glyph by inheritance; a color here silently won every
	   time and the swatches appeared to do nothing. */
	.icon-btn {
		color: inherit;
	}

	.emoji-btn:hover,
	.icon-btn:hover {
		background: var(--color-surface-overlay);
	}

	.emoji-btn.current,
	.icon-btn.current {
		background: var(--color-surface-overlay);
		box-shadow: inset 0 0 0 1px var(--color-border);
	}

	.empty {
		grid-column: 1 / -1;
		padding: 24px;
		text-align: center;
		color: var(--color-foreground-subtle);
		font-size: 13px;
	}

	.picker-footer {
		padding: 6px 8px;
		border-top: 1px solid var(--color-border);
	}
</style>
