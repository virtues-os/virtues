<script lang="ts">
	import { onMount } from "svelte";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import Icon from "$lib/components/Icon.svelte";
	import { sidebarState } from "$lib/stores/sidebarState.svelte";
	import WorkspaceHeader from "./WorkspaceHeader.svelte";
	import SidebarFooter from "./SidebarFooter.svelte";
	import SystemSection from "./SystemSection.svelte";
	import PinnedSection from "./PinnedSection.svelte";
	import SpacesSection from "./SpacesSection.svelte";
	import { SYSTEM_SECTIONS } from "$lib/sidebar/sections";
	import SearchModal from "./SearchModal.svelte";

	// Collapsed state from shared store (also consumed by WindowTabBar)
	const isCollapsed = $derived(sidebarState.collapsed);

	// Search modal state
	let isSearchOpen = $state(false);

	// Track if store is ready
	let storeReady = $state(false);

	// Initialize window shell store and keyboard shortcuts
	onMount(() => {
		windowShellStore
			.init()
			.then(() => {
				storeReady = true;
			})
			.catch((err) => {
				console.error("[UnifiedSidebar] Failed to initialize:", err);
				storeReady = true;
			});

		window.addEventListener("keydown", handleKeydown);

		return () => {
			window.removeEventListener("keydown", handleKeydown);
		};
	});

	function handleKeydown(e: KeyboardEvent) {
		// Cmd+Shift+N - New page
		if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.key === "n") {
			e.preventDefault();
			handleNewPage();
			return;
		}
		// Cmd+N or Ctrl+N - New chat
		if ((e.metaKey || e.ctrlKey) && e.key === "n") {
			e.preventDefault();
			handleNewChat();
		}
		// Cmd+S or Ctrl+S - Toggle sidebar collapse
		if ((e.metaKey || e.ctrlKey) && e.key === "s") {
			e.preventDefault();
			toggleCollapse();
		}
		// Cmd+K or Ctrl+K - Toggle search/command center
		if ((e.metaKey || e.ctrlKey) && e.key === "k") {
			e.preventDefault();
			toggleSearch();
		}
		// Cmd+W or Ctrl+W - Open wiki overview
		if ((e.metaKey || e.ctrlKey) && e.key === "w") {
			e.preventDefault();
			handleWikiOverview();
		}
	}

	function handleSearch() {
		isSearchOpen = true;
	}

	function toggleSearch() {
		isSearchOpen = !isSearchOpen;
	}

	function closeSearch() {
		isSearchOpen = false;
	}

	function handleWikiOverview() {
		windowShellStore.openTabFromRoute("/wiki", {
			label: "Wiki",
			preferEmptyPane: true,
		});
	}

	function handleNewChat() {
		// Always open a new chat tab (forceNew ensures we don't reuse existing)
		windowShellStore.openTabFromRoute("/", {
			label: "New Chat",
			forceNew: true,
		});
	}

	async function handleNewPage() {
		// Create a new page and open it in a new tab
		const { pagesStore } = await import("$lib/stores/pages.svelte");
		const page = await pagesStore.createNewPage();
		windowShellStore.openTabFromRoute(`/page/${page.id}`, {
			label: page.title,
			forceNew: true,
		});
	}

	function toggleCollapse() {
		sidebarState.toggle();
	}

	// Stagger delay per item
	const STAGGER_DELAY = 30;

	// Tailwind utility class strings
	const sidebarClass = $derived.by(() =>
		[
			"sidebar-container relative h-full bg-transparent",
			"transition-[width] duration-300 ease-[cubic-bezier(0.34,1.56,0.64,1)]",
			isCollapsed ? "sidebar-collapsed" : "w-52 overflow-hidden",
		].join(" "),
	);

	const sidebarInnerClass = $derived.by(() =>
		[
			"flex h-full min-w-52 w-52 flex-col",
			isCollapsed ? "pointer-events-none" : "",
		].join(" "),
	);
</script>

<aside class={sidebarClass}>
	<!-- Book Spine: When collapsed, show expand button on hover -->
	{#if isCollapsed}
		<button
			class="sidebar-expand-button group absolute top-0 left-0 w-[36px] z-30 flex h-full cursor-pointer items-center justify-center border-none bg-transparent"
			onclick={toggleCollapse}
			aria-label="Expand sidebar"
		>
			<svg
				class="sidebar-expand-icon h-3.5 w-3.5 -translate-x-[3px] opacity-0 transition-all duration-200 ease-premium group-active:scale-95"
				style="color: var(--color-foreground-subtle)"
				viewBox="0 0 24 24"
				fill="none"
				stroke="currentColor"
				stroke-width="2"
				stroke-linecap="round"
				stroke-linejoin="round"
			>
				<!-- Double chevron right >> -->
				<polyline points="6 17 11 12 6 7" />
				<polyline points="13 17 18 12 13 7" />
			</svg>
		</button>
	{/if}

	<div class={sidebarInnerClass}>
		<WorkspaceHeader
			collapsed={isCollapsed}
			animationDelay={STAGGER_DELAY}
		/>

		<!-- Command bar -->
		<button
			class="command-bar"
			class:collapsed={isCollapsed}
			onclick={handleSearch}
			title="Command (⌘K)"
			style="animation-delay: {STAGGER_DELAY * 2}ms; --stagger-delay: {STAGGER_DELAY * 2}ms"
		>
			<span class="command-label">Command</span>
			<kbd class="command-kbd">⌘K</kbd>
		</button>

		<nav
			class="workspace-nav"
			class:collapsed={isCollapsed}
		>
			{#if !storeReady}
				<div class="loading-state">
					<Icon icon="ri:loader-4-line" width="16" class="spinner" />
					<span>Loading...</span>
				</div>
			{:else}
				<!-- Pinned (user-curated; renders nothing when empty) -->
				<PinnedSection collapsed={isCollapsed} />

				<!-- System sections (from constants) -->
				{#each SYSTEM_SECTIONS as section (section.id)}
					<SystemSection
						{section}
						collapsed={isCollapsed}
						accentColor={null}
					/>
				{/each}

				<!-- Spaces — the rooms a chat can live in -->
				<SpacesSection collapsed={isCollapsed} />
			{/if}
		</nav>

		<SidebarFooter
			collapsed={isCollapsed}
			animationDelay={10 * STAGGER_DELAY}
		/>
	</div>
</aside>

<SearchModal open={isSearchOpen} onClose={closeSearch} />

<style>
	@reference "../../../app.css";
	@reference "$lib/styles/sidebar.css";

	/* Collapsed sidebar behavior */
	.sidebar-collapsed {
		width: 0;
		overflow: visible; /* Allow hover zone to extend beyond 0-width */
		/* Transition handled by Tailwind classes on parent */
	}

	/* Hover zone extends through the mini state + page padding area */
	.sidebar-collapsed::before {
		content: "";
		position: absolute;
		top: 0;
		left: 0;
		width: 36px; /* 20px mini state + padding area */
		height: 100%;
		z-index: 20;
		pointer-events: auto;
		cursor: pointer;
	}

	/* On hover, expand to show the open icon */
	.sidebar-collapsed:hover {
		width: 20px;
	}

	/* Show icon when sidebar is hovered */
	.sidebar-collapsed:hover .sidebar-expand-icon {
		opacity: 1;
	}

	@keyframes fadeSlideIn {
		from {
			opacity: 0;
			transform: translateX(-8px);
		}
		to {
			opacity: 1;
			transform: translateX(0);
		}
	}

	@keyframes spin {
		from {
			transform: rotate(0deg);
		}
		to {
			transform: rotate(360deg);
		}
	}

	.workspace-nav {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		overflow-x: hidden;
		padding: 12px 0 12px 8px;
	}

	.workspace-nav.collapsed {
		display: flex;
		flex-direction: column;
		align-items: center;
	}

	.loading-state {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 12px;
		color: var(--color-foreground-subtle);
		font-size: 13px;
	}

	.spinner {
		animation: spin 1s linear infinite;
	}

	/* Command bar — visual separator between header and content */
	.command-bar {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 4px;
		margin: 0 0 4px 8px;
		padding: 5px 8px;
		background: color-mix(in srgb, var(--color-foreground) 5%, transparent);
		border: 1px solid transparent;
		border-radius: 6px;
		font-family: var(--font-sans);
		font-size: 12px;
		color: var(--color-foreground-subtle);
		cursor: pointer;
		transition: all 0.15s ease;
		animation: fadeSlideIn 200ms cubic-bezier(0.2, 0, 0, 1) backwards;
	}

	.command-bar:hover {
		background: color-mix(in srgb, var(--color-foreground) 8%, transparent);
		color: var(--color-foreground-muted);
	}

	.command-bar.collapsed {
		opacity: 0;
		transform: translateX(-8px);
		pointer-events: none;
		transition:
			opacity 150ms cubic-bezier(0.2, 0, 0, 1),
			transform 150ms cubic-bezier(0.2, 0, 0, 1);
	}

	.command-label {
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.command-kbd {
		font-family: inherit;
		font-size: 10px;
		color: var(--color-foreground-subtle);
		opacity: 0.7;
	}
</style>
