<script lang="ts">
	/**
	 * IconPicker - Icon and Emoji selection popover content
	 *
	 * Content component for selecting icons or emojis.
	 * Supports search, tabs (Icons, Emoji, Recent), and recent selections.
	 * Use inside a Popover primitive for proper positioning and dismiss behavior.
	 */
	import Icon from './Icon.svelte';
	import { addCollection } from '@iconify/svelte';
	import { onMount } from 'svelte';
	import { PIN_COLORS, accentCss } from '$lib/sidebar/pin-colors';

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

	// Emoji are already colored; tinting them does nothing, so the row hides
	// rather than sitting there inert on that tab.
	const colorRowVisible = $derived(!!onColorSelect && activeTab !== 'emoji');
	// A custom hex is anything that isn't one of the nine keys.
	const isCustom = $derived(!!activeColor && activeColor.startsWith('#'));
	const gridTint = $derived(activeColor ? `color: ${accentCss(activeColor)}` : '');

	function pickColor(key: string | null) {
		activeColor = key;
		onColorSelect?.(key);
	}

	let pickerEl: HTMLDivElement;
	let searchInputEl: HTMLInputElement;

	// Full Remix Icons collection (lazy-loaded)
	let fullCollectionLoaded = $state(false);
	let fullCollectionLoading = $state(false);
	let allRiIconNames = $state<string[]>([]);
	let allIconsPage = $state(0);
	const ALL_ICONS_PAGE_SIZE = 200;

	// Emoji categories with common emojis
	const emojiCategories = [
		{
			name: 'Smileys',
			emojis: ['😀', '😃', '😄', '😁', '😅', '😂', '🙂', '😊', '😇', '🥰', '😍', '🤩', '😘', '😋', '😛', '🤪', '😎', '🤓', '🧐', '🤔', '😏', '😌', '😴', '🥳']
		},
		{
			name: 'Gestures',
			emojis: ['👋', '🤚', '✋', '🖐️', '👌', '🤌', '✌️', '🤞', '🫰', '🤟', '🤘', '🤙', '👈', '👉', '👆', '👇', '☝️', '👍', '👎', '👊', '✊', '🤛', '🤜', '👏', '🙌', '🫶', '👐', '🤝', '🙏']
		},
		{
			name: 'Objects',
			emojis: ['📝', '📄', '📁', '📂', '🗂️', '📅', '📆', '📌', '📍', '🔖', '🏷️', '💼', '📦', '🎁', '🔑', '🗝️', '🔒', '🔓', '💡', '🔦', '🧭', '⏰', '⌚', '📱', '💻', '🖥️', '🖨️', '⌨️', '🖱️', '💾']
		},
		{
			name: 'Symbols',
			emojis: ['❤️', '🧡', '💛', '💚', '💙', '💜', '🖤', '🤍', '🤎', '💔', '❣️', '💕', '💞', '💓', '💗', '💖', '💘', '💝', '⭐', '🌟', '✨', '💫', '🔥', '💯', '✅', '❌', '⚠️', '💬', '💭', '🔔']
		},
		{
			name: 'Nature',
			emojis: ['🌸', '🌺', '🌻', '🌼', '🌷', '🌹', '🥀', '🌱', '🌲', '🌳', '🌴', '🌵', '🍀', '🍁', '🍂', '🍃', '🌈', '☀️', '🌤️', '⛅', '🌦️', '🌧️', '⛈️', '🌩️', '❄️', '🌊']
		},
		{
			name: 'Food',
			emojis: ['🍎', '🍊', '🍋', '🍌', '🍉', '🍇', '🍓', '🫐', '🍒', '🍑', '🥭', '🍍', '🥥', '🥝', '🍅', '🥑', '🥦', '🥬', '🥒', '🌶️', '🫑', '🌽', '🥕', '🧄', '🧅', '🥔']
		},
		{
			name: 'Activities',
			emojis: ['⚽', '🏀', '🏈', '⚾', '🥎', '🎾', '🏐', '🏉', '🥏', '🎱', '🏓', '🏸', '🏒', '🥅', '⛳', '🏹', '🎣', '🥊', '🥋', '🎽', '🛹', '🛼', '🎿', '⛷️', '🏂', '🎮', '🎲', '🧩', '🎯', '🎳']
		},
		{
			name: 'Travel',
			emojis: ['🚗', '🚕', '🚌', '🚎', '🏎️', '🚓', '🚑', '🚒', '🚐', '🛻', '🚚', '🚛', '🚜', '🏍️', '🛵', '🚲', '🛴', '✈️', '🚀', '🛸', '🚁', '🛶', '⛵', '🚤', '🛥️', '🚢', '🏠', '🏡', '🏢', '🏣']
		}
	];

	// Flatten emojis for search
	const allEmojis = emojiCategories.flatMap(cat => cat.emojis);

	// All registered icons from icons.ts - organized by category
	const iconCategories = [
		{
			name: 'Files & Folders',
			icons: [
				'ri:file-text-line', 'ri:file-line', 'ri:file-fill', 'ri:file-text-fill',
				'ri:file-list-3-line', 'ri:file-info-line', 'ri:file-code-fill',
				'ri:file-pdf-fill', 'ri:file-excel-fill', 'ri:file-word-fill',
				'ri:file-ppt-fill', 'ri:file-zip-fill', 'ri:file-unknow-line',
				'ri:folder-line', 'ri:folder-fill', 'ri:folder-open-line',
				'ri:folder-add-line', 'ri:folder-chart-fill'
			]
		},
		{
			name: 'Books & Writing',
			icons: [
				'ri:book-line', 'ri:book-2-line', 'ri:book-open-line',
				'ri:quill-pen-line', 'ri:edit-line', 'ri:double-quotes-l', 'ri:translate-2'
			]
		},
		{
			name: 'Communication',
			icons: [
				'ri:chat-1-line', 'ri:chat-3-line', 'ri:chat-smile-2-line',
				'ri:message-3-line', 'ri:mail-line', 'ri:send-plane-line',
				'ri:send-plane-fill', 'ri:feedback-line'
			]
		},
		{
			name: 'Interface',
			icons: [
				'ri:settings-3-line', 'ri:settings-4-line', 'ri:search-line',
				'ri:search-eye-line', 'ri:add-line', 'ri:add-circle-line',
				'ri:delete-bin-line', 'ri:delete-bin-7-line', 'ri:refresh-line',
				'ri:filter-line', 'ri:filter-3-line', 'ri:apps-line',
				'ri:list-check', 'ri:list-unordered', 'ri:swap-line',
				'ri:upload-2-line', 'ri:external-link-line', 'ri:information-line',
				'ri:question-line', 'ri:alert-line', 'ri:error-warning-line',
				'ri:layout-column-line', 'ri:layout-right-line'
			]
		},
		{
			name: 'Navigation & Maps',
			icons: [
				'ri:compass-line', 'ri:compass-3-line', 'ri:map-pin-line',
				'ri:map-pin-2-line', 'ri:map-pin-add-line', 'ri:map-2-line',
				'ri:global-line', 'ri:footprint-line', 'ri:run-line'
			]
		},
		{
			name: 'Data & Charts',
			icons: [
				'ri:database-2-line', 'ri:database-2-fill', 'ri:bar-chart-line',
				'ri:line-chart-line', 'ri:dashboard-line', 'ri:table-line',
				'ri:node-tree', 'ri:speed-line'
			]
		},
		{
			name: 'Users & People',
			icons: [
				'ri:user-line', 'ri:user-3-line', 'ri:user-add-line',
				'ri:user-settings-line', 'ri:user-star-line'
			]
		},
		{
			name: 'Time & Calendar',
			icons: [
				'ri:calendar-line', 'ri:calendar-2-line', 'ri:calendar-event-line',
				'ri:calendar-check-line', 'ri:time-line', 'ri:history-line'
			]
		},
		{
			name: 'Media',
			icons: [
				'ri:image-fill', 'ri:movie-fill', 'ri:music-fill',
				'ri:play-fill', 'ri:play-line', 'ri:pause-line', 'ri:mic-line'
			]
		},
		{
			name: 'Objects & Symbols',
			icons: [
				'ri:lightbulb-line', 'ri:lightbulb-flash-line', 'ri:magic-line',
				'ri:bookmark-line', 'ri:bookmark-fill', 'ri:heart-line',
				'ri:heart-pulse-line', 'ri:lock-line', 'ri:link', 'ri:links-line',
				'ri:pushpin-line', 'ri:unpin-line', 'ri:price-tag-3-line',
				'ri:box-3-line', 'ri:palette-line', 'ri:mickey-line',
				'ri:seedling-line', 'ri:cloud-line', 'ri:moon-line', 'ri:moon-fill'
			]
		},
		{
			name: 'Development & Tech',
			icons: [
				'ri:terminal-box-line', 'ri:bug-line', 'ri:tools-line',
				'ri:cpu-line', 'ri:robot-line', 'ri:robot-fill',
				'ri:plug-line', 'ri:hard-drive-2-line', 'ri:computer-line',
				'ri:device-line', 'ri:wifi-line'
			]
		},
		{
			name: 'Business',
			icons: [
				'ri:building-line', 'ri:building-2-line', 'ri:bank-line',
				'ri:wallet-line', 'ri:money-dollar-circle-line', 'ri:bank-card-line'
			]
		},
		{
			name: 'Brands',
			icons: ['ri:apple-fill', 'ri:google-line', 'ri:twitter-x-fill']
		}
	];

	// Flatten icons for search
	const allIcons = iconCategories.flatMap(cat => cat.icons);

	// One flat pool per tab. The curated list leads (it is the hand-picked,
	// most-wanted set) and the rest of the collection follows once it has
	// loaded, deduped so a curated icon doesn't appear twice on one sheet.
	const searchableIcons = $derived(
		fullCollectionLoaded
			? [...allIcons, ...allRiIconNames.filter((i) => !allIcons.includes(i))]
			: allIcons
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

	const visibleEmojis = $derived.by(() => {
		const q = search.trim().toLowerCase();
		return q ? allEmojis.filter((e) => e.includes(q)) : allEmojis;
	});

	// Lazy-load the full Remix Icons collection
	async function loadFullCollection() {
		if (fullCollectionLoaded || fullCollectionLoading) return;
		fullCollectionLoading = true;
		try {
			const { icons: riData } = await import('@iconify-json/ri');
			const parsed = typeof riData === 'string' ? JSON.parse(riData) : riData;
			addCollection(parsed);
			allRiIconNames = Object.keys(parsed.icons).map(n => `ri:${n}`).sort();
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

		// Start loading the full icon collection
		loadFullCollection();
	});

	function handleSelect(icon: string) {
		onSelect(icon);
		close();
	}

	function handleRemove() {
		onSelect(null);
		close();
	}

	// Check if value is an emoji (starts with emoji character) vs icon (contains :)
	function isEmoji(val: string): boolean {
		return !val.includes(':');
	}
</script>

<div class="icon-picker" bind:this={pickerEl}>
	<!-- Two tabs. "Recent" is gone: it was a third place the same glyphs lived,
	     and search over the whole collection finds a repeat faster than
	     remembering which tab it was filed under. -->
	<div class="picker-tabs">
		<button
			class="tab"
			class:active={activeTab === 'icons'}
			onclick={() => (activeTab = 'icons')}
		>
			Icons
		</button>
		<button
			class="tab"
			class:active={activeTab === 'emoji'}
			onclick={() => (activeTab = 'emoji')}
		>
			Emojis
		</button>
	</div>

	{#if colorRowVisible}
		<!-- The color lives in the same panel as the icon because they are one
		     decision. Selecting here doesn't close — the grid below restains, so
		     you can see the pairing before committing to a glyph.

		     The tick goes INSIDE the chosen swatch rather than ringing it: at
		     this size a ring reads as a slightly bigger circle, which is not a
		     state anyone can spot in a row of ten. -->
		<div class="color-row" role="radiogroup" aria-label="Icon color">
			<button
				class="swatch swatch-auto"
				class:selected={!activeColor}
				onclick={() => pickColor(null)}
				role="radio"
				aria-checked={!activeColor}
				title="No color — follows the text around it"
				aria-label="No color"
			>
				{#if !activeColor}
					<Icon icon="ri:check-line" width="16" />
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
						<Icon icon="ri:check-line" width="16" />
					{/if}
				</button>
			{/each}

			<span class="color-divider" aria-hidden="true"></span>

			<!-- Custom. The hex is stored verbatim, but `accentCss` pulls its
			     LIGHTNESS into the current theme's legible band before drawing
			     it — so a navy picked on paper stays navy and stays visible on
			     the black theme, instead of being the one color in the app that
			     ignores where it is. Hue and chroma, the part actually chosen,
			     are untouched. -->
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
				{#if isCustom}
					<Icon icon="ri:check-line" width="16" />
				{/if}
			</label>
		</div>
	{/if}

	<div class="picker-search">
		<input
			type="text"
			bind:value={search}
			bind:this={searchInputEl}
			placeholder={activeTab === 'emoji' ? 'Search emojis...' : 'Search icons...'}
			class="search-input"
		/>
	</div>

	<!-- Content. One flat grid, no category headings: the headings turned a
	     dense sheet of glyphs into a scroll of small sections, and nobody
	     browses icons by the taxonomy someone else chose — they scan, or they
	     type. -->
	<div class="picker-content">
		{#if activeTab === 'icons'}
			<div class="icon-grid" style={gridTint}>
				{#each visibleIcons as icon (icon)}
					<button class="icon-btn" onclick={() => handleSelect(icon)} title={icon}>
						<Icon {icon} width="17" />
					</button>
				{/each}
			</div>
			{#if visibleIcons.length === 0}
				<div class="empty">No icons found</div>
			{:else if hasMoreIcons && !search}
				<button class="load-more-btn" onclick={() => allIconsPage++}>
					Load more
				</button>
			{/if}
		{:else}
			<div class="emoji-grid">
				{#each visibleEmojis as emoji (emoji)}
					<button class="emoji-btn" onclick={() => handleSelect(emoji)}>
						{emoji}
					</button>
				{/each}
			</div>
			{#if visibleEmojis.length === 0}
				<div class="empty">No emojis found</div>
			{/if}
		{/if}
	</div>

	<!-- Footer with remove option (conditional) -->
	{#if showRemove && value}
		<div class="picker-footer">
			<button class="remove-btn" onclick={handleRemove}>
				<Icon icon="ri:delete-bin-line" width="14" />
				Remove icon
			</button>
		</div>
	{/if}
</div>

<style>
	.icon-picker {
		width: 360px;
		/* Never taller than the window it floats in. A fixed 480px panel in a
		   short window gets flipped by the positioner and then hangs off the
		   top edge, which is how the whole picker ends up showing you nothing
		   but its footer. */
		max-height: min(480px, calc(100vh - 24px));
		display: flex;
		flex-direction: column;
		overflow: hidden;
	}

	.picker-search {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 12px;
		border-bottom: 1px solid var(--color-border);
		color: var(--color-foreground-muted);
	}

	.search-input {
		flex: 1;
		background: none;
		border: none;
		outline: none;
		font-size: 14px;
		color: var(--color-foreground);
	}

	.search-input::placeholder {
		color: var(--color-foreground-subtle);
	}

	.picker-tabs {
		display: flex;
		border-bottom: 1px solid var(--color-border);
		padding: 0 8px;
	}

	/* One row, no wrap, no scroll: ten swatches is the whole palette and the
	   panel is sized for it. A wrapping second row would push the grid down by
	   a line for one extra color nobody asked for. */
	/* Ten circles inside 360px: sized to fit and spread by the row rather than
	   by a fixed gap, so the last swatch can't fall off the panel edge. */
	.color-row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 4px;
		padding: 12px 14px;
		border-bottom: 1px solid var(--color-border);
	}

	.swatch {
		width: 24px;
		height: 24px;
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

	/* The tick sits inside the swatch — at 26px an outer ring just reads as a
	   marginally larger circle, which is not a state anyone spots in a row. */
	.swatch.selected {
		transform: none;
	}

	.color-divider {
		width: 1px;
		height: 18px;
		flex: none;
		background: var(--color-border);
	}

	/* The custom well wears a colour wheel until something is chosen, then the
	   chosen colour — so it reads as "pick your own", not as a tenth hue. */
	.swatch-custom {
		position: relative;
		overflow: hidden;
		background:
			var(--swatch, none),
			conic-gradient(
				from 0deg,
				#e5484d, #e2a336, #e5c518, #46a758,
				#12a594, #0091ff, #6e56cf, #d6409f, #e5484d
			);
		background-size: cover;
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

	/* "No color" — a hairline ring, since there is no color to show. */
	.swatch-auto {
		background: transparent;
		color: var(--color-foreground-muted);
		box-shadow: inset 0 0 0 1.5px var(--color-border);
	}

	.swatch-auto.selected {
		box-shadow: inset 0 0 0 1.5px var(--color-foreground-muted);
	}

	.tab {
		flex: 1;
		padding: 10px 12px;
		font-size: 13px;
		font-weight: 400;
		color: var(--color-foreground-muted);
		background: none;
		border: none;
		border-bottom: 2px solid transparent;
		cursor: pointer;
		transition: all 150ms;
	}

	.tab:hover {
		color: var(--color-foreground);
	}

	.tab.active {
		color: var(--color-primary);
		border-bottom-color: var(--color-primary);
	}

	.picker-content {
		flex: 1;
		overflow-y: auto;
		padding: 12px;
		min-height: 240px;
		max-height: 340px;
	}

	.category {
		margin-bottom: 12px;
	}

	.category-name {
		font-size: 11px;
		font-weight: 400;
		color: var(--color-foreground-subtle);
		padding: 4px 4px 8px;
	}

	.emoji-grid {
		display: grid;
		grid-template-columns: repeat(8, 1fr);
		gap: 2px;
	}

	/* Dense sheet, not a list of buttons. Ten to a row at 360px, so the eye
	   scans a block of glyphs the way it scans a contact sheet — which is the
	   only way anyone actually finds an icon they can't name. */
	.icon-grid {
		display: grid;
		grid-template-columns: repeat(10, 1fr);
		gap: 2px;
		color: var(--color-foreground-muted);
	}

	.emoji-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 32px;
		height: 32px;
		font-size: 20px;
		background: none;
		border: none;
		border-radius: 6px;
		cursor: pointer;
		transition: background 100ms;
	}

	.emoji-btn:hover {
		background: var(--color-surface-overlay);
	}

	.icon-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 100%;
		aspect-ratio: 1;
		background: none;
		border: none;
		border-radius: 8px;
		/* `inherit`, NOT a color of its own. The chosen tint is set on the grid
		   and reaches the glyph by inheritance; a color here silently won every
		   time and the swatches appeared to do nothing. */
		color: inherit;
		cursor: pointer;
		transition: all 100ms;
	}

	.icon-btn:hover {
		background: var(--color-surface-overlay);
		color: var(--color-foreground);
	}

	.icon-btn.emoji {
		font-size: 20px;
	}

	.emoji-char {
		font-size: 20px;
		line-height: 1;
	}

	.empty {
		grid-column: 1 / -1;
		padding: 24px;
		text-align: center;
		color: var(--color-foreground-subtle);
		font-size: 13px;
	}

	.picker-footer {
		padding: 8px 12px;
		border-top: 1px solid var(--color-border);
	}

	.remove-btn {
		display: flex;
		align-items: center;
		gap: 6px;
		width: 100%;
		padding: 8px 12px;
		font-size: 13px;
		color: var(--color-error);
		background: none;
		border: none;
		border-radius: 6px;
		cursor: pointer;
		transition: background 100ms;
	}

	.remove-btn:hover {
		background: color-mix(in srgb, var(--color-error) 10%, transparent);
	}

	.load-more-btn {
		width: 100%;
		padding: 8px;
		font-size: 12px;
		color: var(--color-primary);
		background: none;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		cursor: pointer;
		margin-top: 8px;
		transition: background 100ms;
	}

	.load-more-btn:hover {
		background: var(--color-surface-overlay);
	}
</style>
