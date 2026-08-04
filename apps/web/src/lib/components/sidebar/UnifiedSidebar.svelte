<script lang="ts">
	import { onMount } from "svelte";
	import { fly } from "svelte/transition";
	import { cubicIn, cubicOut } from "svelte/easing";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import Icon from "$lib/components/Icon.svelte";
	import { sidebarState } from "$lib/stores/sidebarState.svelte";
	import WorkspaceHeader from "./WorkspaceHeader.svelte";
	import SidebarFooter from "./SidebarFooter.svelte";
	import SystemSection from "./SystemSection.svelte";
	import DeskSection from "./DeskSection.svelte";
	import ZoneHeader from "./ZoneHeader.svelte";
	import { sidebarZones } from "$lib/stores/sidebarZones.svelte";
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

	// The panel swaps as one object, not as a cascade of rows.
	//
	// It used to waterfall: every row animated in on its own 30ms delay, so
	// entering Settings played eleven little arrivals. That reads as a flourish
	// the second time and as latency by the tenth — the panel appeared to take
	// 300ms to assemble when the work was instant. A crossfade with 16px of
	// travel says the same thing (these are different contents) in under
	// 200ms, and says it about the panel rather than about each row.
	//
	// Sequential, not concurrent — this is the part that decides whether it
	// feels good. Svelte runs `in:` and `out:` at the same time by default, so
	// for the whole overlap you are looking at two half-transparent copies of
	// a list sliding through each other. That reads as smeared, not as a
	// swap; no amount of tuning the distance fixes it, because the problem is
	// that both panels are visible at once.
	//
	// So the old panel leaves first (110ms, short travel — it is going away,
	// it doesn't need to be watched), and the new one waits for it to finish
	// before arriving (210ms over 18px). The grid cell holds the height
	// throughout, so nothing collapses in the gap.
	const swapKey = $derived(sidebarMode.activeId ?? "root");

	// Tailwind utility class strings
	const sidebarClass = $derived.by(() =>
		[
			"sidebar-container relative h-full bg-transparent",
			"transition-[width] duration-300 ease-[cubic-bezier(0.34,1.56,0.64,1)]",
			isCollapsed ? "sidebar-collapsed" : "w-[220px] overflow-hidden",
		].join(" "),
	);

	const sidebarInnerClass = $derived.by(() =>
		[
			"flex h-full min-w-[220px] w-[220px] flex-col",
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
			{:else}
				<!-- One swap, not eleven. The two panels are stacked in a single
				     grid cell so the outgoing one can leave while the incoming
				     one arrives, with no reflow between them. -->
				<div class="nav-swap">
					{#key swapKey}
						<div
							class="nav-layer"
							in:fly={{ x: 18, duration: 210, delay: 110, easing: cubicOut, opacity: 0 }}
							out:fly={{ x: -10, duration: 110, easing: cubicIn, opacity: 0 }}
						>
							{#if sidebarMode.active && !isCollapsed}
								<SidebarModePanel mode={sidebarMode.active} />
							{:else}
								<!-- The Desk: what the user has taken off the shelf.
								     Serif spines with bookcloth dots — the type
								     distinction encodes ownership, which is what keeps
								     pins from reading as a tinted nav row (the failure
								     that retired them the first time). -->
								<DeskSection collapsed={isCollapsed} />

								<!-- The Library: every fixed room the app has, in
								     one shelf. Stable forever, so muscle memory
								     can live in it. Still a loop over groups —
								     there is one today, and the Workbench that
								     used to be the second one may not be the last
								     shelf anyone proposes. -->
								{#each SECTION_GROUPS as group, i (group.id)}
									<div class="nav-group">
										{#if group.label && !isCollapsed}
											<ZoneHeader id={group.id} label={group.label} />
										{/if}
										<div
											class="sidebar-expandable"
											class:expanded={!sidebarZones.isCollapsed(group.id)}
										>
											<div class="sidebar-expandable-inner">
												{#each group.items as section (section.id)}
													<SystemSection
														{section}
														collapsed={isCollapsed}
														accentColor={null}
													/>
												{/each}
												<!-- The seam to the next zone, inside the fold
												     that owns it — the Desk's trick, for the
												     same reason: a spacer OUTSIDE the clipping
												     box survives the fold and leaves a hole
												     belonging to nothing. -->
												{#if i < SECTION_GROUPS.length - 1}
													<div class="zone-tail" aria-hidden="true"></div>
												{/if}
											</div>
										</div>
									</div>
								{/each}
							{/if}
						</div>
					{/key}
				</div>
			{/if}
		</nav>

		<SidebarFooter
			collapsed={isCollapsed}
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

	@keyframes spin {
		from {
			transform: rotate(0deg);
		}
		to {
			transform: rotate(360deg);
		}
	}

	/* The gap above the first zone is the same one row of air that sits
	   between zones, so Search → Desk and Desk → Library read as one
	   interval rather than two arbitrary ones. */
	.workspace-nav {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		overflow-x: hidden;
		padding: var(--sidebar-interactive-height) 0 12px 8px;
	}

	.workspace-nav.collapsed {
		display: flex;
		flex-direction: column;
		align-items: center;
	}

	/* Both layers share one grid cell, so the leaving panel doesn't push the
	   arriving one around while they overlap. Cheaper and steadier than
	   absolute positioning: the cell keeps the taller layer's height, so the
	   scroll container never jumps mid-swap. */
	.nav-swap {
		display: grid;
	}

	.nav-layer {
		grid-area: 1 / 1;
		min-width: 0;
	}

	/* Group header — the "contents-page" treatment: serif smallcaps,
	/* No margin here. The row of air between the zones is owned by the Desk's
	   own collapsible region, so it folds away with the pins — otherwise
	   closing the Desk left a 28px hole between two adjacent subtitles, and
	   the space read as belonging to nothing. */

	/* Same token as every other interval in the column, so Desk → Workbench
	   and Workbench → Library read as one repeated beat. */
	.zone-tail {
		height: var(--sidebar-interactive-height);
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
