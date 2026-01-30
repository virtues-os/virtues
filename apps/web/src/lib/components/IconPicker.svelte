<script lang="ts">
	/**
	 * IconPicker - Icon and Emoji selection modal
	 *
	 * A centered modal for selecting icons or emojis.
	 * Supports search, tabs (Icons, Emoji, Recent), and recent selections.
	 */
	import Icon from './Icon.svelte';
	import { addCollection } from '@iconify/svelte';
	import { onMount } from 'svelte';

	interface Props {
		/** Current value (emoji or icon name) */
		value?: string | null;
		/** Called when an icon/emoji is selected */
		onSelect: (value: string | null) => void;
		/** Called when picker is closed */
		onClose: () => void;
		/** Whether to show the "Remove icon" option (default: true) */
		showRemove?: boolean;
	}

	let { value = null, onSelect, onClose, showRemove = true }: Props = $props();

	let search = $state('');
	let activeTab = $state<'icons' | 'emoji' | 'recent'>('icons');
	let pickerEl: HTMLDivElement;
	let searchInputEl: HTMLInputElement;

	// Full Remix Icons collection (lazy-loaded)
	let fullCollectionLoaded = $state(false);
	let fullCollectionLoading = $state(false);
	let allRiIconNames = $state<string[]>([]);
	let allIconsPage = $state(0);
	const ALL_ICONS_PAGE_SIZE = 200;
	const paginatedAllIcons = $derived(
		allRiIconNames.slice(0, (allIconsPage + 1) * ALL_ICONS_PAGE_SIZE)
	);
	const hasMoreIcons = $derived(
		paginatedAllIcons.length < allRiIconNames.length
	);

	// Portal action - moves element to body for proper z-index stacking
	function portal(node: HTMLElement) {
		document.body.appendChild(node);
		return {
			destroy() {
				node.remove();
			}
		};
	}

	// Recent icons from localStorage
	let recentIcons = $state<string[]>([]);
	const RECENT_KEY = 'virtues:recent-icons';
	const MAX_RECENT = 24;

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

	// Filtered items based on search
	const filteredEmojis = $derived(
		search
			? allEmojis.filter(e => e.includes(search.toLowerCase()))
			: null
	);

	const searchableIcons = $derived(
		fullCollectionLoaded ? allRiIconNames : allIcons
	);

	const filteredIcons = $derived(
		search
			? searchableIcons.filter(i => i.toLowerCase().includes(search.toLowerCase()))
			: null
	);

	const filteredRecent = $derived(
		search
			? recentIcons.filter(i => i.toLowerCase().includes(search.toLowerCase()) || i.includes(search))
			: recentIcons
	);

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
		try {
			const stored = localStorage.getItem(RECENT_KEY);
			if (stored) {
				recentIcons = JSON.parse(stored);
			}
		} catch (e) {
			console.error('Failed to load recent icons:', e);
		}

		// Focus search input
		setTimeout(() => searchInputEl?.focus(), 50);

		// Start loading the full icon collection
		loadFullCollection();
	});

	function saveRecent(icon: string) {
		// Add to front, remove duplicates, limit size
		recentIcons = [icon, ...recentIcons.filter(i => i !== icon)].slice(0, MAX_RECENT);
		try {
			localStorage.setItem(RECENT_KEY, JSON.stringify(recentIcons));
		} catch (e) {
			console.error('Failed to save recent icons:', e);
		}
	}

	function handleSelect(icon: string) {
		saveRecent(icon);
		onSelect(icon);
		onClose();
	}

	function handleRemove() {
		onSelect(null);
		onClose();
	}

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			e.preventDefault();
			onClose();
		}
	}

	// Check if value is an emoji (starts with emoji character) vs icon (contains :)
	function isEmoji(val: string): boolean {
		return !val.includes(':');
	}
</script>

<svelte:window onkeydown={handleKeydown} />

<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
<div class="picker-backdrop" use:portal onclick={(e) => e.target === e.currentTarget && onClose()}>
	<div class="icon-picker" bind:this={pickerEl}>
	<!-- Search -->
	<div class="picker-search">
		<Icon icon="ri:search-line" width="14" />
		<input
			type="text"
			bind:value={search}
			bind:this={searchInputEl}
			placeholder="Search..."
			class="search-input"
		/>
	</div>

	<!-- Tabs: Icons first, then Emoji, then Recent -->
	<div class="picker-tabs">
		<button
			class="tab"
			class:active={activeTab === 'icons'}
			onclick={() => activeTab = 'icons'}
		>
			Icons
		</button>
		<button
			class="tab"
			class:active={activeTab === 'emoji'}
			onclick={() => activeTab = 'emoji'}
		>
			Emoji
		</button>
		<button
			class="tab"
			class:active={activeTab === 'recent'}
			onclick={() => activeTab = 'recent'}
		>
			Recent
		</button>
	</div>

	<!-- Content -->
	<div class="picker-content">
		{#if activeTab === 'icons'}
			{#if search && filteredIcons}
				<div class="icon-grid">
					{#each filteredIcons as icon}
						<button class="icon-btn" onclick={() => handleSelect(icon)} title={icon}>
							<Icon {icon} width="20" />
						</button>
					{/each}
					{#if filteredIcons.length === 0}
						<div class="empty">No icons found</div>
					{/if}
				</div>
			{:else}
				{#each iconCategories as category}
					<div class="category">
						<div class="category-name">{category.name}</div>
						<div class="icon-grid">
							{#each category.icons as icon}
								<button class="icon-btn" onclick={() => handleSelect(icon)} title={icon}>
									<Icon {icon} width="20" />
								</button>
							{/each}
						</div>
					</div>
				{/each}
				{#if fullCollectionLoaded}
					<div class="category">
						<div class="category-name">All Icons ({allRiIconNames.length})</div>
						<div class="icon-grid">
							{#each paginatedAllIcons as icon}
								<button class="icon-btn" onclick={() => handleSelect(icon)} title={icon}>
									<Icon {icon} width="20" />
								</button>
							{/each}
						</div>
						{#if hasMoreIcons}
							<button class="load-more-btn" onclick={() => allIconsPage++}>
								Load more icons...
							</button>
						{/if}
					</div>
				{:else if fullCollectionLoading}
					<div class="category">
						<div class="category-name">Loading all icons...</div>
					</div>
				{/if}
			{/if}

		{:else if activeTab === 'emoji'}
			{#if search && filteredEmojis}
				<div class="emoji-grid">
					{#each filteredEmojis as emoji}
						<button class="emoji-btn" onclick={() => handleSelect(emoji)}>
							{emoji}
						</button>
					{/each}
					{#if filteredEmojis.length === 0}
						<div class="empty">No emojis found</div>
					{/if}
				</div>
			{:else}
				{#each emojiCategories as category}
					<div class="category">
						<div class="category-name">{category.name}</div>
						<div class="emoji-grid">
							{#each category.emojis as emoji}
								<button class="emoji-btn" onclick={() => handleSelect(emoji)}>
									{emoji}
								</button>
							{/each}
						</div>
					</div>
				{/each}
			{/if}

		{:else if activeTab === 'recent'}
			{#if filteredRecent.length > 0}
				<div class="icon-grid mixed">
					{#each filteredRecent as item}
						<button
							class="icon-btn"
							class:emoji={isEmoji(item)}
							onclick={() => handleSelect(item)}
							title={isEmoji(item) ? item : item}
						>
							{#if isEmoji(item)}
								<span class="emoji-char">{item}</span>
							{:else}
								<Icon icon={item} width="20" />
							{/if}
						</button>
					{/each}
				</div>
			{:else}
				<div class="empty">No recent icons</div>
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
</div>

<style>
	.picker-backdrop {
		position: fixed;
		inset: 0;
		z-index: 10000;
		background: rgba(0, 0, 0, 0.4);
		display: flex;
		align-items: flex-start;
		justify-content: center;
		padding-top: 12vh;
		animation: backdrop-in 150ms ease-out;
	}

	@keyframes backdrop-in {
		from { opacity: 0; }
		to { opacity: 1; }
	}

	.icon-picker {
		width: 360px;
		max-height: 480px;
		background: var(--color-surface-elevated);
		border: 1px solid var(--color-border);
		border-radius: 12px;
		box-shadow: 0 16px 48px rgba(0, 0, 0, 0.24);
		display: flex;
		flex-direction: column;
		overflow: hidden;
		animation: picker-in 150ms ease-out;
	}

	@keyframes picker-in {
		from {
			opacity: 0;
			transform: translateY(-8px) scale(0.96);
		}
		to {
			opacity: 1;
			transform: translateY(0) scale(1);
		}
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

	.tab {
		flex: 1;
		padding: 10px 12px;
		font-size: 13px;
		font-weight: 500;
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
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.5px;
		color: var(--color-foreground-subtle);
		padding: 4px 4px 8px;
	}

	.emoji-grid {
		display: grid;
		grid-template-columns: repeat(8, 1fr);
		gap: 2px;
	}

	.icon-grid {
		display: grid;
		grid-template-columns: repeat(6, 1fr);
		gap: 4px;
	}

	.icon-grid.mixed {
		grid-template-columns: repeat(6, 1fr);
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
		width: 40px;
		height: 40px;
		background: none;
		border: none;
		border-radius: 8px;
		color: var(--color-foreground-muted);
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
