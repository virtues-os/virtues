<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import { fade, fly } from "svelte/transition";
	import { cubicOut } from "svelte/easing";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { chatSessions } from "$lib/stores/chatSessions.svelte";
	import { askVirtues } from "$lib/stores/pendingPrompt.svelte";
	import { pagesStore } from "$lib/stores/pages.svelte";
	import { notebookStore } from "$lib/stores/notebook.svelte";
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
	let wasOpen = $state(false); // Track previous open state to detect transitions

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
		windowShellStore.openTabFromRoute(`/page/${page.id}`, {
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
			action: () => windowShellStore.openTabFromRoute("/"),
		},
		{
			id: "new-temp-chat",
			label: "New Temporary Chat",
			icon: "ri:ghost-line",
			shortcut: "⌘⇧T",
			action: () =>
				windowShellStore.openTabFromRoute("/?temporary=1", {
					label: "Temporary Chat",
					forceNew: true,
				}),
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
			action: () => windowShellStore.openTabFromRoute("/wiki"),
		},
		{
			id: "sources",
			label: "Go to Sources",
			icon: "ri:device-line",
			action: () => windowShellStore.openTabFromRoute("/sources"),
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
			action: () => windowShellStore.openTabFromRoute("/virtues/you"),
		},
	];

	// Type scope — a leading `#chats` / `#pages` / `#actions` / `#notebooks`
	// token narrows results to one kind. Singular and plural both work (`#chat` ==
	// `#chats`). The token is stripped from the text that's actually matched.
	type Scope = "chats" | "pages" | "actions" | "notebooks";
	const SCOPE_ALIASES: Record<string, Scope> = {
		chat: "chats", chats: "chats",
		page: "pages", pages: "pages",
		action: "actions", actions: "actions",
		notebook: "notebooks", notebooks: "notebooks",
	};
	const scope = $derived.by<Scope | null>(() => {
		const token = searchQuery.trimStart().match(/^#(\w+)/)?.[1]?.toLowerCase();
		return token ? (SCOPE_ALIASES[token] ?? null) : null;
	});
	// Only strip the leading token when it resolved to a real scope, so a bare `#`
	// or an unknown `#foo` is still searched literally.
	const effectiveQuery = $derived(
		scope ? searchQuery.trim().replace(/^#\w+\s*/, "") : searchQuery.trim(),
	);

	// Filter results based on search (respecting any active type scope). Things and
	// notebooks only surface once there's a query (or their explicit scope), so the
	// bare ⌘K stays lean — actions/chats/pages recents only.
	const filteredResults = $derived.by(() => {
		const query = effectiveQuery.toLowerCase().trim();
		const limit = scope ? 25 : 5;
		const match = (name: string | null) =>
			!query || (name || "Untitled").toLowerCase().includes(query);
		const inScope = (s: Scope) => scope === s || (!scope && !!query);

		return {
			actions:
				!scope || scope === "actions"
					? quickActions.filter((a) => !query || a.label.toLowerCase().includes(query))
					: [],
			chats:
				!scope || scope === "chats"
					? chatSessions.sessions.filter((c) => match(c.title)).slice(0, limit)
					: [],
			pages:
				!scope || scope === "pages"
					? pagesStore.pages.filter((p) => match(p.title)).slice(0, limit)
					: [],
			notebooks: inScope("notebooks")
				? notebookStore.notebooks.filter((s) => match(s.name)).slice(0, limit)
				: [],
		};
	});

	// "Ask Virtues" — when there's free text (and no #scope), the top row opens a
	// real chat with the query. It occupies index 0, shifting nav results down.
	const showAsk = $derived(!scope && effectiveQuery.trim().length > 0);
	const askOffset = $derived(showAsk ? 1 : 0);

	// One flat, ordered list of selectable rows — the single source of truth for
	// keyboard nav and Enter. Order must match the render order below.
	type Row =
		| { kind: "ask" }
		| { kind: "action"; item: (typeof quickActions)[number] }
		| { kind: "chat"; item: (typeof chatSessions.sessions)[number] }
		| { kind: "page"; item: (typeof pagesStore.pages)[number] }
		| { kind: "notebook"; item: (typeof notebookStore.notebooks)[number] };
	const orderedRows = $derived.by<Row[]>(() => [
		...(showAsk ? [{ kind: "ask" } as Row] : []),
		...filteredResults.actions.map((item) => ({ kind: "action", item }) as Row),
		...filteredResults.chats.map((item) => ({ kind: "chat", item }) as Row),
		...filteredResults.pages.map((item) => ({ kind: "page", item }) as Row),
		...filteredResults.notebooks.map((item) => ({ kind: "notebook", item }) as Row),
	]);
	const totalResults = $derived(orderedRows.length);

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
		const row = orderedRows[selectedIndex];
		if (!row) return;

		switch (row.kind) {
			case "ask":
				askVirtues(effectiveQuery.trim());
				onClose();
				break;
			case "action":
				row.item.action();
				if (!row.item.keepOpen) onClose();
				break;
			case "chat":
				windowShellStore.openTabFromRoute(`/chat/${row.item.conversation_id}`, {
					label: row.item.title || "Chat",
				});
				onClose();
				break;
			case "page":
				windowShellStore.openTabFromRoute(`/page/${row.item.id}`, {
					label: row.item.title || "Untitled",
				});
				onClose();
				break;
			case "notebook":
				windowShellStore.openTabFromRoute(`/notebook/${row.item.id}`, {
					label: row.item.name || "Untitled",
				});
				onClose();
				break;
		}
	}

	function handleBackdropClick(e: MouseEvent) {
		if (e.target === e.currentTarget) {
			onClose();
		}
	}

	// Focus input and reset state only when modal first opens (not on re-renders)
	$effect(() => {
		if (open && !wasOpen) {
			// Modal just opened - reset state
			searchQuery = "";
			selectedIndex = 0;
			mode = "search";
			originalTheme = null;
			// Load pages if not already loaded
			if (pagesStore.pages.length === 0 && !pagesStore.pagesLoading) {
				pagesStore.loadPages();
			}
			// Load chat sessions so #chats search has data even on a fresh load
			if (chatSessions.sessions.length === 0 && !chatSessions.isLoading) {
				chatSessions.load();
			}
			// Same for notebooks so its scope has data to match.
			if (notebookStore.notebooks.length === 0 && !notebookStore.loading) {
				notebookStore.load();
			}
		}
		wasOpen = open;
	});

	// Keep the highlighted row valid as the result set changes with the query.
	$effect(() => {
		void searchQuery;
		selectedIndex = 0;
	});

	// Focus input when modal is open and input is available
	$effect(() => {
		if (open && inputEl) {
			inputEl.focus();
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
					{#if scope}
						<span class="scope-chip">#{scope}</span>
					{/if}
					<input
						bind:this={inputEl}
						bind:value={searchQuery}
						type="text"
						placeholder="Search… (try #chats, #pages, #notebooks)"
						class="search-input"
					/>
					<kbd class="escape-hint">Esc</kbd>
				</div>

				<!-- Results -->
				<div class="results">
				{#if showAsk}
					<button
						class="result-item ask-row"
						class:selected={selectedIndex === 0}
						data-result-index={0}
						onclick={() => {
							askVirtues(effectiveQuery.trim());
							onClose();
						}}
						onmouseenter={() => (selectedIndex = 0)}
					>
						<Icon icon="ri:sparkling-2-line" width="16" class="result-icon" />
						<span class="result-label">Ask Virtues — <span class="ask-q">"{effectiveQuery.trim()}"</span></span>
						<kbd class="result-shortcut">⏎</kbd>
					</button>
				{/if}
				{#if filteredResults.actions.length > 0}
					<div class="result-group">
						<span class="group-label">Quick Actions</span>
						{#each filteredResults.actions as action, i}
							{@const index = askOffset + i}
							<button
								class="result-item"
								class:selected={selectedIndex === index}
								data-result-index={index}
								onclick={() => {
									action.action();
									if (!action.keepOpen) {
										onClose();
									}
								}}
								onmouseenter={() => (selectedIndex = index)}
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
							{@const index = askOffset + filteredResults.actions.length + i}
							<button
								class="result-item"
								class:selected={selectedIndex === index}
								data-result-index={index}
								onclick={() => {
									windowShellStore.openTabFromRoute(
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
							{@const index = askOffset + filteredResults.actions.length + filteredResults.chats.length + i}
							<button
								class="result-item"
								class:selected={selectedIndex === index}
								data-result-index={index}
								onclick={() => {
									windowShellStore.openTabFromRoute(`/page/${page.id}`, {
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

				{#if filteredResults.notebooks.length > 0}
					<div class="result-group">
						<span class="group-label">Notebooks</span>
						{#each filteredResults.notebooks as notebook, i}
							{@const index =
								askOffset +
								filteredResults.actions.length +
								filteredResults.chats.length +
								filteredResults.pages.length +
								i}
							<button
								class="result-item"
								class:selected={selectedIndex === index}
								data-result-index={index}
								onclick={() => {
									windowShellStore.openTabFromRoute(`/notebook/${notebook.id}`, {
										label: notebook.name || "Untitled",
									});
									onClose();
								}}
								onmouseenter={() => (selectedIndex = index)}
							>
								<Icon icon={notebook.icon || "ri:layout-grid-line"} width="16" class="result-icon" />
								<span class="result-label">{notebook.name || "Untitled"}</span>
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
		/* Bottom inset keeps tall result lists above the home indicator. */
		padding-top: max(15vh, env(safe-area-inset-top));
		padding-bottom: max(16px, env(safe-area-inset-bottom));
		z-index: var(--z-modal);
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

	:global(.search-icon) {
		color: var(--foreground-muted) !important;
		flex-shrink: 0;
	}

	.scope-chip {
		flex-shrink: 0;
		font-family: var(--font-mono);
		font-size: 11px;
		font-weight: 500;
		padding: 3px 7px;
		border-radius: 6px;
		background: var(--primary-subtle);
		color: var(--primary);
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

	:global(.result-icon) {
		color: var(--foreground-muted) !important;
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

	.ask-row {
		margin-bottom: 4px;
	}

	.ask-row :global(.result-icon) {
		color: var(--primary) !important;
	}

	.ask-q {
		color: var(--foreground-muted);
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
