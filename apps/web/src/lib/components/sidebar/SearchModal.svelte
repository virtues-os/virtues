<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import { fade, fly } from "svelte/transition";
	import { cubicOut } from "svelte/easing";
	import { spaceStore } from "$lib/stores/space.svelte";
	import { chatSessions } from "$lib/stores/chatSessions.svelte";
	import { pagesStore } from "$lib/stores/pages.svelte";
	import {
		getAvailableThemes,
		getThemeDisplayName,
		getTheme,
		applyTheme,
		setTheme,
		themeMetadata,
		type Theme,
	} from "$lib/utils/theme";

	interface Props {
		open?: boolean;
		onClose: () => void;
	}

	let { open = false, onClose }: Props = $props();

	// Modal mode: 'search' or 'theme'
	type ModalMode = "search" | "theme";
	let mode = $state<ModalMode>("search");

	let searchQuery = $state("");
	let selectedIndex = $state(0);
	let inputEl: HTMLInputElement | null = $state(null);
	let modalEl: HTMLDivElement | null = $state(null);

	// Theme selection state
	let originalTheme = $state<Theme | null>(null);
	let themeSelectedIndex = $state(0);
	const themes = getAvailableThemes();

	// Focus modal when entering theme mode
	$effect(() => {
		if (mode === "theme" && modalEl) {
			modalEl.focus();
		}
	});

	// Create new page action
	async function createNewPage() {
		const page = await pagesStore.createNewPage();
		spaceStore.openTabFromRoute(`/page/${page.id}`, {
			label: page.title,
			preferEmptyPane: true,
		});
	}

	// Enter theme selection mode
	function enterThemeMode() {
		originalTheme = getTheme();
		themeSelectedIndex = themes.indexOf(originalTheme);
		if (themeSelectedIndex === -1) themeSelectedIndex = 0;
		mode = "theme";
	}

	// Exit theme mode without saving
	function exitThemeMode() {
		if (originalTheme) {
			applyTheme(originalTheme);
		}
		mode = "search";
		originalTheme = null;
	}

	// Save selected theme and exit
	function saveTheme() {
		const selectedTheme = themes[themeSelectedIndex];
		setTheme(selectedTheme);
		mode = "search";
		originalTheme = null;
		onClose();
	}

	// Preview theme on selection change
	function previewTheme(index: number) {
		themeSelectedIndex = index;
		applyTheme(themes[index]);
	}

	// Quick actions
	const quickActions = [
		{
			id: "new-chat",
			label: "New Chat",
			icon: "ri:add-line",
			shortcut: "⌘N",
			action: () => spaceStore.openTabFromRoute("/"),
		},
		{
			id: "new-page",
			label: "New Page",
			icon: "ri:file-text-line",
			shortcut: "⌘⇧N",
			action: createNewPage,
		},
		{
			id: "wiki",
			label: "Go to Wiki",
			icon: "ri:book-2-line",
			shortcut: "⌘W",
			action: () => spaceStore.openTabFromRoute("/wiki"),
		},
		{
			id: "sources",
			label: "Go to Sources",
			icon: "ri:device-line",
			action: () => spaceStore.openTabFromRoute("/source"),
		},
		{
			id: "change-theme",
			label: "Change Theme",
			icon: "ri:palette-line",
			action: enterThemeMode,
			keepOpen: true,
		},
		{
			id: "settings",
			label: "Open Settings",
			icon: "ri:settings-4-line",
			action: () => spaceStore.openTabFromRoute("/virtues/account"),
		},
	];

	// Filter results based on search
	const filteredResults = $derived.by(() => {
		const query = searchQuery.toLowerCase().trim();

		if (!query) {
			// Show quick actions, recent chats, and recent pages when empty
			return {
				actions: quickActions,
				chats: chatSessions.sessions.slice(0, 5),
				pages: pagesStore.pages.slice(0, 5),
			};
		}

		// Filter quick actions
		const matchedActions = quickActions.filter((a) =>
			a.label.toLowerCase().includes(query),
		);

		// Filter chats
		const matchedChats = chatSessions.sessions
			.filter((c) =>
				(c.title || "Untitled").toLowerCase().includes(query),
			)
			.slice(0, 5);

		// Filter pages
		const matchedPages = pagesStore.pages
			.filter((p) =>
				(p.title || "Untitled").toLowerCase().includes(query),
			)
			.slice(0, 5);

		return {
			actions: matchedActions,
			chats: matchedChats,
			pages: matchedPages,
		};
	});

	// Total results count for keyboard navigation
	const totalResults = $derived(
		filteredResults.actions.length + filteredResults.chats.length + filteredResults.pages.length,
	);

	function handleKeydown(e: KeyboardEvent) {
		if (mode === "theme") {
			handleThemeKeydown(e);
			return;
		}

		if (e.key === "Escape") {
			e.preventDefault();
			onClose();
		} else if (e.key === "ArrowDown") {
			e.preventDefault();
			selectedIndex = Math.min(selectedIndex + 1, totalResults - 1);
		} else if (e.key === "ArrowUp") {
			e.preventDefault();
			selectedIndex = Math.max(selectedIndex - 1, 0);
		} else if (e.key === "Enter") {
			e.preventDefault();
			selectCurrentItem();
		}
	}

	function handleThemeKeydown(e: KeyboardEvent) {
		if (e.key === "Escape") {
			e.preventDefault();
			exitThemeMode();
		} else if (e.key === "ArrowDown") {
			e.preventDefault();
			const newIndex = Math.min(themeSelectedIndex + 1, themes.length - 1);
			previewTheme(newIndex);
		} else if (e.key === "ArrowUp") {
			e.preventDefault();
			const newIndex = Math.max(themeSelectedIndex - 1, 0);
			previewTheme(newIndex);
		} else if (e.key === "Enter") {
			e.preventDefault();
			saveTheme();
		}
	}

	function selectCurrentItem() {
		const actionsCount = filteredResults.actions.length;
		const chatsCount = filteredResults.chats.length;

		if (selectedIndex < actionsCount) {
			// It's an action
			const action = filteredResults.actions[selectedIndex];
			action.action();
			if (!action.keepOpen) {
				onClose();
			}
		} else if (selectedIndex < actionsCount + chatsCount) {
			// It's a chat
			const chatIndex = selectedIndex - actionsCount;
			const chat = filteredResults.chats[chatIndex];
			if (chat) {
				spaceStore.openTabFromRoute(
					`/chat/${chat.conversation_id}`,
					{
						label: chat.title || "Chat",
					},
				);
				onClose();
			}
		} else {
			// It's a page
			const pageIndex = selectedIndex - actionsCount - chatsCount;
			const page = filteredResults.pages[pageIndex];
			if (page) {
				spaceStore.openTabFromRoute(`/page/${page.id}`, {
					label: page.title || "Untitled",
				});
				onClose();
			}
		}
	}

	function handleBackdropClick(e: MouseEvent) {
		if (e.target === e.currentTarget) {
			onClose();
		}
	}

	// Focus input and load pages when modal opens
	$effect(() => {
		if (open && inputEl) {
			inputEl.focus();
			searchQuery = "";
			selectedIndex = 0;
			mode = "search";
			originalTheme = null;
			// Load pages if not already loaded
			if (pagesStore.pages.length === 0 && !pagesStore.pagesLoading) {
				pagesStore.loadPages();
			}
		}
	});

	// Scroll selected item into view when navigating with keyboard
	$effect(() => {
		if (!open) return;

		if (mode === "search" && selectedIndex >= 0) {
			const selectedEl = document.querySelector(`[data-result-index="${selectedIndex}"]`);
			if (selectedEl) {
				selectedEl.scrollIntoView({ block: "nearest" });
			}
		} else if (mode === "theme") {
			const selectedEl = document.querySelector(`[data-theme-index="${themeSelectedIndex}"]`);
			if (selectedEl) {
				selectedEl.scrollIntoView({ block: "nearest" });
			}
		}
	});
</script>

{#if open}
	<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
	<div 
		class="modal-backdrop" 
		onclick={handleBackdropClick}
		transition:fade={{ duration: 150, easing: cubicOut }}
	>
		<div
			bind:this={modalEl}
			class="modal"
			role="dialog"
			aria-modal="true"
			aria-label={mode === "theme" ? "Select Theme" : "Search"}
			tabindex="-1"
			onkeydown={handleKeydown}
			transition:fly={{ y: -8, duration: 150, easing: cubicOut }}
		>
			{#if mode === "theme"}
				<!-- Theme Selection Header -->
				<div class="search-input-container">
					<button class="back-button" onclick={exitThemeMode}>
						<Icon icon="ri:arrow-left-line" width="18" />
					</button>
					<span class="mode-title">Select Theme</span>
					<kbd class="escape-hint">Esc</kbd>
				</div>

				<!-- Theme List -->
				<div class="results">
					<div class="result-group">
						<span class="group-label">Themes</span>
						{#each themes as theme, i}
							<button
								class="result-item"
								class:selected={themeSelectedIndex === i}
								data-theme-index={i}
								onclick={() => {
									themeSelectedIndex = i;
									saveTheme();
								}}
							>
								<Icon
									icon={themeMetadata[theme].icon}
									width="16"
									class="result-icon"
								/>
								<span class="result-label">{getThemeDisplayName(theme)}</span>
								<span class="theme-description">{themeMetadata[theme].description}</span>
							</button>
						{/each}
					</div>
				</div>
			{:else}
				<!-- Search Input -->
				<div class="search-input-container">
					<Icon
						icon="ri:search-line"
						width="18"
						class="search-icon"
					/>
					<input
						bind:this={inputEl}
						bind:value={searchQuery}
						type="text"
						placeholder="Search chats, pages, or actions..."
						class="search-input"
					/>
					<kbd class="escape-hint">Esc</kbd>
				</div>

				<!-- Results -->
				<div class="results">
				{#if filteredResults.actions.length > 0}
					<div class="result-group">
						<span class="group-label">Quick Actions</span>
						{#each filteredResults.actions as action, i}
							<button
								class="result-item"
								class:selected={selectedIndex === i}
								data-result-index={i}
								onclick={() => {
									action.action();
									if (!action.keepOpen) {
										onClose();
									}
								}}
								onmouseenter={() => (selectedIndex = i)}
							>
								<Icon
									icon={action.icon}
									width="16"
									class="result-icon"
								/>
								<span class="result-label">{action.label}</span>
								{#if action.shortcut}
									<kbd class="result-shortcut"
										>{action.shortcut}</kbd
									>
								{/if}
							</button>
						{/each}
					</div>
				{/if}

				{#if filteredResults.chats.length > 0}
					<div class="result-group">
						<span class="group-label">Recent Chats</span>
						{#each filteredResults.chats as chat, i}
							{@const index = filteredResults.actions.length + i}
							<button
								class="result-item"
								class:selected={selectedIndex === index}
								data-result-index={index}
								onclick={() => {
									spaceStore.openTabFromRoute(
										`/chat/${chat.conversation_id}`,
										{
											label: chat.title || "Chat",
										},
									);
									onClose();
								}}
								onmouseenter={() => (selectedIndex = index)}
							>
								<Icon
									icon="ri:message-3-line"
									width="16"
									class="result-icon"
								/>
								<span class="result-label"
									>{chat.title || "Untitled"}</span
								>
							</button>
						{/each}
					</div>
				{/if}

				{#if filteredResults.pages.length > 0}
					<div class="result-group">
						<span class="group-label">Recent Pages</span>
						{#each filteredResults.pages as page, i}
							{@const index = filteredResults.actions.length + filteredResults.chats.length + i}
							<button
								class="result-item"
								class:selected={selectedIndex === index}
								data-result-index={index}
								onclick={() => {
									spaceStore.openTabFromRoute(`/page/${page.id}`, {
										label: page.title || "Untitled",
									});
									onClose();
								}}
								onmouseenter={() => (selectedIndex = index)}
							>
								<Icon
									icon="ri:file-text-line"
									width="16"
									class="result-icon"
								/>
								<span class="result-label"
									>{page.title || "Untitled"}</span
								>
							</button>
						{/each}
					</div>
				{/if}

				{#if totalResults === 0 && searchQuery}
					<div class="no-results">
						<span>No results found for "{searchQuery}"</span>
					</div>
				{/if}
			</div>
			{/if}
		</div>
	</div>
{/if}

<style>
	@reference "../../../app.css";

	.modal-backdrop {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.4);
		display: flex;
		align-items: flex-start;
		justify-content: center;
		padding-top: 15vh;
		z-index: 9999;
	}

	.modal {
		width: 100%;
		max-width: 520px;
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: 12px;
		box-shadow: 0 16px 48px rgba(0, 0, 0, 0.2);
		overflow: hidden;
		outline: none;
	}

	.search-input-container {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 14px 16px;
		border-bottom: 1px solid var(--border);
	}

	.search-icon {
		color: var(--foreground-muted);
		flex-shrink: 0;
	}

	.search-input {
		flex: 1;
		border: none;
		background: transparent;
		font-size: 15px;
		color: var(--foreground);
		outline: none;
	}

	.search-input::placeholder {
		color: var(--foreground-subtle);
	}

	.escape-hint {
		font-family: var(--font-mono);
		font-size: 10px;
		padding: 3px 6px;
		background: var(--surface-elevated);
		border-radius: 4px;
		color: var(--foreground-subtle);
	}

	.results {
		max-height: 400px;
		overflow-y: auto;
		padding: 8px;
	}

	.result-group {
		margin-bottom: 8px;
	}

	.group-label {
		display: block;
		font-size: 11px;
		font-weight: 500;
		text-transform: uppercase;
		letter-spacing: 0.02em;
		color: var(--foreground-subtle);
		padding: 6px 8px;
	}

	.result-item {
		display: flex;
		align-items: center;
		gap: 10px;
		width: 100%;
		padding: 10px 12px;
		border-radius: 8px;
		cursor: pointer;
		background: transparent;
		border: none;
		text-align: left;
		color: var(--foreground);
		transition: background-color 80ms ease-out;
	}

	.result-item:hover,
	.result-item.selected {
		background: var(--surface-overlay);
	}

	.result-item.selected {
		background: var(--primary-subtle);
	}

	.result-icon {
		color: var(--foreground-muted);
		flex-shrink: 0;
	}

	.result-label {
		flex: 1;
		font-size: 14px;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.result-shortcut {
		font-family: var(--font-mono);
		font-size: 10px;
		padding: 2px 6px;
		background: var(--surface-elevated);
		border-radius: 4px;
		color: var(--foreground-subtle);
	}

	.no-results {
		padding: 24px 16px;
		text-align: center;
		color: var(--foreground-subtle);
		font-size: 14px;
	}

	.back-button {
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 4px;
		border: none;
		background: transparent;
		color: var(--foreground-muted);
		cursor: pointer;
		border-radius: 4px;
		transition: background-color 80ms ease-out;
	}

	.back-button:hover {
		background: var(--surface-overlay);
		color: var(--foreground);
	}

	.mode-title {
		flex: 1;
		font-size: 15px;
		font-weight: 500;
		color: var(--foreground);
	}

	.theme-description {
		font-size: 12px;
		color: var(--foreground-subtle);
		white-space: nowrap;
	}
</style>
