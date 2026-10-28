<script lang="ts">
	/**
	 * IconPicker - the mark a page, chat, pin or project wears
	 *
	 * One decision, shown as its result: a preview row at the top draws the
	 * thing with its mark and name, and follows whatever the pointer is over.
	 * Color comes first because it is the whole choice for most people: with
	 * no icon, a thing wears a dot in its color. Icons and emoji sit below
	 * behind one search. Use inside a Popover primitive for positioning and
	 * dismiss behavior.
	 */
	import Icon from './Icon.svelte';
	import Button from './Button.svelte';
	import MenuItem from './MenuItem.svelte';
	import { onMount } from 'svelte';
	import { PIN_COLORS, accentCss } from '$lib/sidebar/pin-colors';
	import { GLYPHS as ATLAS_GLYPHS } from '$lib/components/sidebar/AtlasIcon.svelte';
	import { loadLucide } from '$lib/lucide';

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
		 * The icon's color: a `--cat-*` token key, a custom hex, or null for
		 * "inherit the text around it". Only meaningful with `onColorSelect`.
		 */
		color?: string | null;
		/**
		 * Set the icon's color. Omit it and the color row doesn't render: the
		 * picker is opened from places whose entity has nowhere to store one,
		 * and a control that silently forgets is worse than none. Fires
		 * immediately and does NOT close.
		 */
		onColorSelect?: (color: string | null) => void;
		/** The thing's name, drawn in the preview row. */
		label?: string;
		/** The mark the thing wears with no icon chosen. Defaults to the dot. */
		fallback?: string;
		/**
		 * The CSS color the thing wears with no color chosen, such as a pin's
		 * automatic hue. Omit it and "no color" previews in the text color.
		 */
		autoColor?: string | null;
	}

	let {
		value = null,
		onSelect,
		close,
		showRemove = true,
		color = null,
		onColorSelect,
		label = 'Untitled',
		fallback = 'shape:circle',
		autoColor = null,
	}: Props = $props();

	let search = $state('');
	let activeTab = $state<'icons' | 'emoji'>('icons');

	// Local echo of the chosen color so the preview and grid tint on click
	// without waiting for the parent to persist and flow a new prop back down.
	let activeColor = $state<string | null>(null);
	$effect(() => {
		activeColor = color;
	});

	// What the pointer is over. `undefined` means nothing; a null color is a
	// real choice ("no color"), so it can't double as "not hovering".
	let hoverIcon = $state<string | null>(null);
	let hoverColor = $state<string | null | undefined>(undefined);

	const previewIcon = $derived(hoverIcon ?? value ?? fallback);
	const previewColor = $derived(hoverColor !== undefined ? hoverColor : activeColor);
	const previewTint = $derived.by(() => {
		const css = previewColor ? accentCss(previewColor) : autoColor;
		return css ? `color: ${css}` : '';
	});

	// A custom hex is anything that isn't one of the nine keys.
	const isCustom = $derived(!!activeColor && activeColor.startsWith('#'));

	function pickColor(key: string | null) {
		activeColor = key;
		onColorSelect?.(key);
	}

	let searchInputEl: HTMLInputElement;

	// Lucide, loaded once and drawn at Atlas's line (see lucide.ts).
	let lucideNames = $state<string[]>([]);
	let iconsPage = $state(0);
	const ICONS_PAGE_SIZE = 180;

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

	// The icons people reach for to label a page or a project, ahead of the
	// rest of the set in alphabetical order.
	const curatedIcons = [
		'file-text', 'folder', 'book-open', 'notebook-pen', 'pen-line', 'quote',
		'languages', 'message-circle', 'mail', 'send', 'lightbulb', 'sparkles',
		'bookmark', 'heart', 'activity', 'lock', 'link', 'tag', 'box', 'palette',
		'sprout', 'leaf', 'mountain', 'cloud', 'moon', 'sun', 'flame', 'snowflake',
		'compass', 'map-pin', 'map', 'globe', 'footprints', 'dumbbell', 'chart-line',
		'table', 'network', 'user', 'users', 'baby', 'dog', 'star', 'calendar', 'clock',
		'image', 'camera', 'film', 'music', 'headphones', 'mic', 'gamepad-2', 'terminal',
		'code', 'bug', 'wrench', 'cpu', 'bot', 'rocket', 'target', 'puzzle', 'brain',
		'house', 'building', 'landmark', 'briefcase', 'wallet', 'piggy-bank', 'receipt',
		'dollar-sign', 'flag', 'flask-conical', 'graduation-cap', 'stethoscope', 'pill',
		'utensils', 'coffee', 'shopping-bag', 'plane', 'car', 'anchor', 'gift', 'trophy',
		'feather', 'umbrella',
	].map((n) => `lucide:${n}`);

	// Atlas leads: the shell's own glyphs, so a page can wear the same mark as
	// the sidebar row that opens its kind.
	const leadIcons = [...Object.keys(ATLAS_GLYPHS).map((n) => `atlas:${n}`), ...curatedIcons];

	// Lucide names wait for the collection: Atlas is registered at startup.
	const searchableIcons = $derived(
		lucideNames.length
			? [...leadIcons, ...lucideNames.filter((i) => !leadIcons.includes(i))]
			: leadIcons.filter((i) => i.startsWith('atlas:'))
	);

	const visibleIcons = $derived.by(() => {
		const q = search.trim().toLowerCase().replace(/\s+/g, '-');
		if (q) return searchableIcons.filter((i) => i.slice(i.indexOf(':') + 1).includes(q));
		// Unsearched, the sheet grows a page at a time: 2k icons in one DOM
		// pass is a locked panel on open.
		return searchableIcons.slice(0, (iconsPage + 1) * ICONS_PAGE_SIZE);
	});

	const hasMoreIcons = $derived(!search && visibleIcons.length < searchableIcons.length);

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

	onMount(() => {
		setTimeout(() => searchInputEl?.focus(), 50);
		loadLucide()
			.then((names) => (lucideNames = names))
			.catch((e) => console.error('Failed to load icon set:', e));
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
	<!-- The result, drawn before any control: the thing's row as it will
	     look, following the pointer across colors and icons. -->
	<div class="preview" aria-hidden="true">
		<span class="preview-mark" style={previewTint}>
			<Icon icon={previewIcon} width="16" />
		</span>
		<span class="preview-label">{label}</span>
	</div>

	{#if onColorSelect}
		<!-- The tick goes INSIDE the chosen swatch: at this size a ring reads
		     as a slightly bigger circle, which nobody spots in a row of eleven. -->
		<div
			class="color-row"
			role="radiogroup"
			aria-label="Color"
			tabindex="-1"
			onmouseleave={() => (hoverColor = undefined)}
		>
			<button
				class="swatch swatch-auto"
				class:selected={!activeColor}
				onclick={() => pickColor(null)}
				onmouseenter={() => (hoverColor = null)}
				role="radio"
				aria-checked={!activeColor}
				title="No color - follows the text around it"
				aria-label="No color"
			>
				{#if !activeColor}
					<Icon icon="mynaui:check" width="14" />
				{/if}
			</button>
			{#each PIN_COLORS as { key, label: colorLabel } (key)}
				<button
					class="swatch"
					class:selected={activeColor === key}
					style="--swatch: var(--cat-{key})"
					onclick={() => pickColor(key)}
					onmouseenter={() => (hoverColor = key)}
					role="radio"
					aria-checked={activeColor === key}
					title={colorLabel}
					aria-label={colorLabel}
				>
					{#if activeColor === key}
						<Icon icon="mynaui:check" width="14" />
					{/if}
				</button>
			{/each}

			<!-- Custom. The hex is stored verbatim, but `accentCss` pulls its
			     LIGHTNESS into the current theme's legible band before drawing
			     it, so a navy picked on paper stays visible on a black theme.
			     Hue and chroma, the part actually chosen, are untouched. -->
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
				<Icon icon={isCustom ? 'mynaui:check' : 'mynaui:plus'} width="14" />
			</label>
		</div>
	{/if}

	<!-- One search, with the icon/emoji switch beside it: it only changes what
	     the grid below shows. -->
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

	<!-- One flat grid, no category headings: nobody browses icons by the
	     taxonomy someone else chose. They scan, or they type. -->
	<div class="picker-content">
		{#if activeTab === 'icons'}
			<div
				class="icon-grid"
				style={previewTint}
				role="presentation"
				onmouseleave={() => (hoverIcon = null)}
			>
				{#each visibleIcons as icon (icon)}
					<button
						class="icon-btn"
						class:current={value === icon}
						onclick={() => handleSelect(icon)}
						onmouseenter={() => (hoverIcon = icon)}
						title={icon.slice(icon.indexOf(':') + 1)}
					>
						<Icon {icon} width="16" />
					</button>
				{/each}
			</div>
			{#if visibleIcons.length === 0}
				<div class="empty">No icons found</div>
			{:else if hasMoreIcons}
				<Button variant="secondary" size="sm" class="w-full mt-2" onclick={() => iconsPage++}>
					Load more
				</Button>
			{/if}
		{:else}
			<div class="emoji-grid" role="presentation" onmouseleave={() => (hoverIcon = null)}>
				{#each visibleEmojis as { emoji, label: emojiLabel } (emoji)}
					<button
						class="emoji-btn"
						class:current={value === emoji}
						onclick={() => handleSelect(emoji)}
						onmouseenter={() => (hoverIcon = emoji)}
						title={emojiLabel}
						aria-label={emojiLabel}
					>
						{emoji}
					</button>
				{/each}
			</div>
			{#if emojisLoading}
				<div class="empty">Loading emoji...</div>
			{:else if visibleEmojis.length === 0}
				<div class="empty">No emoji found</div>
			{/if}
		{/if}
	</div>

	{#if showRemove && value}
		<div class="picker-footer">
			<MenuItem icon="mynaui:trash" label="Remove icon" destructive onclick={handleRemove} />
		</div>
	{/if}
</div>

<style>
	.icon-picker {
		width: 336px;
		/* Never taller than the window it floats in. A fixed 480px panel in a
		   short window gets flipped by the positioner and then hangs off the
		   top edge, which is how the whole picker ends up showing you nothing
		   but its footer. */
		max-height: min(500px, calc(100vh - 24px));
		display: flex;
		flex-direction: column;
		overflow: hidden;
	}

	/* Drawn like a sidebar row: 16px mark, 8px gap, 14px text, so the
	   preview is the size the thing will be, not an enlarged badge. */
	.preview {
		display: flex;
		align-items: center;
		gap: 8px;
		height: 32px;
		margin: 10px 10px 0;
		padding: 0 10px;
		border-radius: var(--radius-md);
		background: var(--color-surface-overlay);
		color: var(--color-foreground-muted);
	}

	.preview-mark {
		display: flex;
		flex: none;
	}

	.preview-label {
		min-width: 0;
		overflow: hidden;
		white-space: nowrap;
		text-overflow: ellipsis;
		font-size: 14px;
		color: var(--color-foreground);
	}

	/* One row, sized to fit and spread by the row rather than by a fixed gap,
	   so the last swatch can't fall off the panel edge. */
	.color-row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 4px;
		padding: 12px 14px;
		outline: none;
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
		border-top: 1px solid var(--color-border);
		border-bottom: 1px solid var(--color-border);
	}

	.search-input {
		flex: 1;
		min-width: 0;
		background: none;
		border: none;
		outline: none;
		font-size: var(--chrome-label-font-size);
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
		border-radius: var(--radius-md);
		background: var(--color-surface-overlay);
	}

	.kind-toggle button {
		padding: 3px 9px;
		font-size: 12px;
		line-height: 16px;
		color: var(--color-foreground-muted);
		background: none;
		border: none;
		border-radius: var(--radius-md);
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

	/* Dense sheet, not a list of buttons, so the eye scans a block of glyphs
	   the way it scans a contact sheet. */
	.icon-grid,
	.emoji-grid {
		display: grid;
		grid-template-columns: repeat(9, 1fr);
		gap: 2px;
	}

	.icon-grid {
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
		border-radius: var(--radius-md);
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
