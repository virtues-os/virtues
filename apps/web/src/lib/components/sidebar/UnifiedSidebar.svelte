<script lang="ts">
	import { onMount } from "svelte";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import Icon from "$lib/components/Icon.svelte";
	import { sidebarState } from "$lib/stores/sidebarState.svelte";
	import WorkspaceHeader from "./WorkspaceHeader.svelte";
	import SidebarFooter from "./SidebarFooter.svelte";
	import SystemSection from "./SystemSection.svelte";
	import PinnedSection from "./PinnedSection.svelte";
	import { SECTION_GROUPS } from "$lib/sidebar/sections";
	import SearchModal from "./SearchModal.svelte";
	import SidebarModePanel from "./SidebarModePanel.svelte";
	import { sidebarMode } from "$lib/stores/sidebarMode.svelte";
	import { shortcuts } from "$lib/shortcuts/registry.svelte";
	import { onSummon, setSummonShortcut, storedSummonChord } from "$lib/tauri/bridge";

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

		// The OS-global chord. Native has already focused the window by the time
		// this fires; summoning the app and then making you press ⌘K is two
		// steps for one intent, so it opens the palette. Reaching for Virtues
		// from another app is nearly always reaching for something *in* it.
		//
		// `open`, not `toggle`: the chord arrives from outside, where you can't
		// see whether the palette is already up, and a toggle would close it
		// half the time for no reason the user could have predicted.
		let unlistenSummon: (() => void) | null = null;
		let disposed = false;
		void onSummon(() => {
			isSearchOpen = true;
		}).then((un) => {
			// onMount's cleanup may already have run — this resolves a tick late.
			if (disposed) un();
			else unlistenSummon = un;
		});

		// Re-apply the stored rebind. Native binds the default at startup so the
		// chord works before any window exists; this replaces it if the user has
		// chosen another.
		const chord = storedSummonChord();
		void setSummonShortcut(chord);

		// Global shortcuts live in the registry, not in a hand-rolled if-chain.
		// Besides discoverability, the registry matches modifiers exactly — the
		// old chain tested `metaKey && key === 's'` without excluding Shift, so
		// ⌘⇧S collapsed the sidebar as a side effect.
		const unregisterShortcuts = shortcuts.register(
			{
				id: "chat.new-temporary",
				keys: "mod+shift+t",
				label: "New temporary chat",
				group: "Create",
				run: handleNewTemporaryChat,
			},
			{
				id: "page.new",
				keys: "mod+shift+n",
				label: "New page",
				group: "Create",
				run: handleNewPage,
			},
			{
				id: "chat.new",
				keys: "mod+n",
				label: "New chat",
				group: "Create",
				run: handleNewChat,
			},
			{
				id: "sidebar.toggle",
				keys: "mod+s",
				label: "Show or hide the sidebar",
				group: "Window",
				run: toggleCollapse,
			},
			{
				id: "search.toggle",
				keys: "mod+k",
				label: "Ask or search",
				group: "Window",
				run: toggleSearch,
			},
			{
				id: "wiki.open",
				keys: "mod+w",
				label: "Open the wiki",
				group: "Go to",
				run: handleWikiOverview,
			},
		);

		return () => {
			disposed = true;
			unlistenSummon?.();
			unregisterShortcuts();
		};
	});

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

	function handleNewTemporaryChat() {
		// Ghost chat — never saved to history
		windowShellStore.openTabFromRoute("/?temporary=1", {
			label: "Temporary Chat",
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

	// Running offset of each group's first row, so the waterfall reads as one
	// continuous fall down the panel rather than restarting per group. Slot 1
	// is the masthead, so nav rows start at 2 and land under it rather than
	// alongside it. The footer sits at slot 10, after the longest nav list.
	const GROUP_OFFSETS = SECTION_GROUPS.reduce<number[]>(
		(acc, group) => [...acc, acc[acc.length - 1] + group.items.length],
		[2],
	);

	const navDelay = (groupIndex: number, itemIndex: number) =>
		(GROUP_OFFSETS[groupIndex] + itemIndex) * STAGGER_DELAY;

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
			class="sidebar-expand-button group absolute top-0 left-0 w-[14px] z-30 flex h-full cursor-pointer items-center justify-center border-none bg-transparent"
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
			onSearch={handleSearch}
		/>

		<nav
			class="workspace-nav"
			class:collapsed={isCollapsed}
		>
			{#if !storeReady}
				<div class="loading-state">
					<Icon icon="ri:loader-4-line" width="16" class="spinner" />
					<span>Loading...</span>
				</div>
			{:else if sidebarMode.active && !isCollapsed}
				<SidebarModePanel mode={sidebarMode.active} stagger={STAGGER_DELAY} />
			{:else}
				<!-- Pinned sits above the system destinations: it's the user's own
				     list, and burying their choices under ours had it read as an
				     afterthought. Renders nothing when empty, so a new box still
				     opens on Home. -->
				<PinnedSection collapsed={isCollapsed} />

				<!-- System destinations, grouped nouns-vs-verbs (from constants).
				     The sidebar is a stable contents-page, not a mode rail. -->
				{#each SECTION_GROUPS as group, groupIndex (group.id)}
					<div class="nav-group">
						{#if group.label && !isCollapsed}
							<div class="nav-group-header">{group.label}</div>
						{/if}
						{#each group.items as section, itemIndex (section.id)}
							<SystemSection
								{section}
								collapsed={isCollapsed}
								accentColor={null}
								animationDelay={navDelay(groupIndex, itemIndex)}
							/>
						{/each}
					</div>
				{/each}

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

	/* Hover zone. Deliberately narrow: the pane toolbar's own sidebar-toggle
	   sits immediately to the right, so a wide zone gets swiped through on the
	   way to that button and the peek fires when nobody asked for it. 14px is
	   the window edge and nothing else. */
	.sidebar-collapsed::before {
		content: "";
		position: absolute;
		top: 0;
		left: 0;
		width: 14px;
		height: 100%;
		z-index: 20;
		pointer-events: auto;
		cursor: pointer;
	}

	/* The peek reveals the icon; it must NOT change width. The collapsed aside
	   is a flex child, so any width here shoves the whole pane sideways — which
	   is exactly the shift that made the toolbar's toggle button crawl away
	   from the cursor as you reached for it. The expand button is absolutely
	   positioned, so opacity alone is enough to show it. */
	.sidebar-collapsed:hover .sidebar-expand-icon {
		opacity: 1;
		transition-delay: 120ms; /* intent delay — a pass-through shouldn't flash */
	}

	@media (prefers-reduced-motion: reduce) {
		.sidebar-collapsed:hover .sidebar-expand-icon {
			transition-delay: 0ms;
		}
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

	/* Group header — the "contents-page" treatment: serif smallcaps,
	   letterspaced, quiet. Carries the classical register of the panel. */
	/* The reflect↔work seam: a blank gap between groups, no text header. */
	.nav-group + .nav-group {
		margin-top: 14px;
	}

	.nav-group-header {
		font-family: var(--font-serif);
		font-size: 11px;
		font-weight: 500;
		text-transform: uppercase;
		letter-spacing: 0.14em;
		color: var(--color-foreground-subtle);
		padding: 0 8px;
		margin: 16px 0 4px;
		user-select: none;
		animation: fadeSlideIn 200ms cubic-bezier(0.2, 0, 0, 1) backwards;
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

</style>
