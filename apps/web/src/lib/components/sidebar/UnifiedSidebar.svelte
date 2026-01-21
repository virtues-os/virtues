<script lang="ts">
	import { page } from "$app/state";
	import { onMount } from "svelte";
	import { chatSessions } from "$lib/stores/chatSessions.svelte";
	import { windowTabs } from "$lib/stores/windowTabs.svelte";
	import { bookmarks } from "$lib/stores/bookmarks.svelte";
	import SidebarHeader from "./SidebarHeader.svelte";
	import SidebarAccordion from "./SidebarAccordion.svelte";
	import SidebarNavItem from "./SidebarNavItem.svelte";
	import SidebarFooter from "./SidebarFooter.svelte";
	import SearchModal from "./SearchModal.svelte";
	import BookmarksModal from "$lib/components/BookmarksModal.svelte";
	import { WikiSidebarSection } from "$lib/components/wiki";
	import type { SidebarSectionData } from "./types";

	const STORAGE_KEY = "virtues-sidebar-collapsed";
	const EXPANDED_SECTIONS_KEY = "virtues-sidebar-expanded";

	// Collapsed state (icon-only mode)
	let isCollapsed = $state(false);

	// Expanded sections state
	let expandedSections = $state<Set<string>>(new Set(["history"]));

	// Search modal state
	let isSearchOpen = $state(false);

	// Bookmarks modal state
	let isBookmarksOpen = $state(false);

	// Load state from localStorage
	onMount(() => {
		const storedCollapsed = localStorage.getItem(STORAGE_KEY);
		if (storedCollapsed !== null) {
			isCollapsed = storedCollapsed === "true";
		}

		const storedExpanded = localStorage.getItem(EXPANDED_SECTIONS_KEY);
		if (storedExpanded) {
			try {
				expandedSections = new Set(JSON.parse(storedExpanded));
			} catch {
				// Use defaults
			}
		}

		// Keyboard shortcuts
		window.addEventListener("keydown", handleKeydown);
		return () => window.removeEventListener("keydown", handleKeydown);
	});

	// Persist collapsed state
	$effect(() => {
		localStorage.setItem(STORAGE_KEY, String(isCollapsed));
	});

	// Persist expanded sections
	$effect(() => {
		localStorage.setItem(
			EXPANDED_SECTIONS_KEY,
			JSON.stringify([...expandedSections]),
		);
	});

	function handleKeydown(e: KeyboardEvent) {
		// Cmd+N or Ctrl+N - New chat
		if ((e.metaKey || e.ctrlKey) && e.key === "n") {
			e.preventDefault();
			handleNewChat();
		}
		// Cmd+[ or Ctrl+[ - Toggle sidebar collapse
		if ((e.metaKey || e.ctrlKey) && e.key === "[") {
			e.preventDefault();
			toggleCollapse();
		}
		// Cmd+K or Ctrl+K - Search
		if ((e.metaKey || e.ctrlKey) && e.key === "k") {
			e.preventDefault();
			handleSearch();
		}
		// Cmd+B or Ctrl+B - Open bookmarks
		if ((e.metaKey || e.ctrlKey) && e.key === "b") {
			e.preventDefault();
			handleOpenBookmarks();
		}
		// Cmd+D or Ctrl+D - Bookmark current tab
		if ((e.metaKey || e.ctrlKey) && e.key === "d") {
			e.preventDefault();
			handleBookmarkCurrentTab();
		}
	}

	function handleSearch() {
		isSearchOpen = true;
	}

	function closeSearch() {
		isSearchOpen = false;
	}

	function handleOpenBookmarks() {
		isBookmarksOpen = true;
	}

	function closeBookmarks() {
		isBookmarksOpen = false;
	}

	async function handleBookmarkCurrentTab() {
		const tab = windowTabs.activeTab;
		if (!tab) return;

		await bookmarks.toggleRouteBookmark({
			route: tab.route,
			tab_type: tab.type,
			label: tab.label,
			icon: tab.icon
		});
	}

	function handleNewChat() {
		// Find existing new chat tab (no conversationId) or create one
		const existingNewChat = windowTabs.getAllTabs().find(
			(t) => t.type === 'chat' && !t.conversationId
		);

		if (existingNewChat) {
			windowTabs.setActiveTab(existingNewChat.id);
		} else {
			windowTabs.openTabFromRoute('/', { label: 'New Chat', preferEmptyPane: true });
		}
	}

	function toggleCollapse() {
		isCollapsed = !isCollapsed;
	}

	function toggleSection(sectionId: string) {
		const newSet = new Set(expandedSections);
		if (newSet.has(sectionId)) {
			newSet.delete(sectionId);
		} else {
			newSet.add(sectionId);
		}
		expandedSections = newSet;
	}

	// Recent chats for display (last 5)
	const recentChats = $derived(chatSessions.sessions.slice(0, 5));

	// Check if there are more chats beyond what's shown
	const hasMoreChats = $derived(chatSessions.sessions.length > 5);

	// Build accordion sections (Memory only - onboarding is handled separately)
	const sections = $derived.by(() => {
		const result: SidebarSectionData[] = [];

		// Memory section (merged Data + Views)
		result.push({
			id: "memory",
			title: "Data",
			icon: "ri:brain-line",
			defaultExpanded: false,
			items: [
				{
					id: "sources",
					type: "link",
					label: "Sources",
					href: "/data/sources",
					icon: "ri:device-line",
					pagespace: "data/sources",
				},
				{
					id: "activity",
					type: "link",
					label: "Activity",
					href: "/data/jobs",
					icon: "ri:history-line",
					pagespace: "data/jobs",
				},
				{
					id: "usage",
					type: "link",
					label: "Usage",
					href: "/usage",
					icon: "ri:dashboard-2-line",
					pagespace: "usage",
				},
				{
					id: "storage",
					type: "link",
					label: "Object Storage",
					href: "/storage",
					icon: "ri:database-2-line",
					pagespace: "storage",
				},
				{
					id: "drive",
					type: "link",
					label: "Drive",
					href: "/data/drive",
					icon: "ri:hard-drive-2-line",
					pagespace: "data/drive",
				},
			],
		});

		return result;
	});

	// Stagger delay per item (ms)
	const STAGGER_DELAY = 30;

	// Compute global animation indices for all sidebar items (top to bottom)
	// Order: logo, actions, chats header, chat items, section headers, section items, footer
	const animationIndices = $derived.by(() => {
		let globalIndex = 0;

		// Header elements
		const logoIndex = globalIndex++;
		const actionsIndex = globalIndex++;

		// Chats accordion header
		const historyLabelIndex = globalIndex++;

		// Then each chat item
		const chatStartIndex = globalIndex;
		globalIndex += recentChats.length;

		// Compute starting index for each section (header + items)
		const sectionHeaderIndices: Map<string, number> = new Map();
		const sectionItemStartIndices: Map<string, number> = new Map();
		for (const section of sections) {
			// Section header gets an index
			sectionHeaderIndices.set(section.id, globalIndex++);
			// Then items start after header
			sectionItemStartIndices.set(section.id, globalIndex);
			globalIndex += section.items.length;
		}

		// Footer comes last
		const footerIndex = globalIndex++;

		return {
			logoIndex,
			actionsIndex,
			historyLabelIndex,
			chatStartIndex,
			sectionHeaderIndices,
			sectionItemStartIndices,
			footerIndex,
		};
	});

	// Initialize expanded state from section defaults
	$effect(() => {
		for (const section of sections) {
			if (section.defaultExpanded && !expandedSections.has(section.id)) {
				expandedSections = new Set([...expandedSections, section.id]);
			}
		}
	});

	// Tailwind utility class strings
	const sidebarClass = $derived.by(() =>
		[
			"relative h-full overflow-hidden bg-[var(--surface-elevated)]",
			"transition-[width] ease-[cubic-bezier(0.34,1.56,0.64,1)]",
			isCollapsed
				? ["w-8", "duration-400 delay-0"].join(" ")
				: ["w-52", "duration-300 delay-100"].join(" "),
		].join(" "),
	);

	const sidebarInnerClass = $derived.by(() =>
		[
			"flex h-full min-w-52 w-52 flex-col",
			isCollapsed ? "pointer-events-none" : "",
		].join(" "),
	);

	const sectionsClass = $derived.by(() =>
		[
			"flex-1 overflow-y-auto overflow-x-hidden px-2 py-3",
			isCollapsed ? "flex flex-col items-center" : "",
		].join(" "),
	);
</script>

<aside class={sidebarClass}>
	<!-- Book Spine: When collapsed, the entire sidebar IS the clickable spine -->
	{#if isCollapsed}
		<button
			class="group absolute inset-0 z-10 flex h-full w-full cursor-pointer items-center justify-center border-none bg-transparent"
			onclick={toggleCollapse}
			aria-label="Expand sidebar"
		>
			<svg
				class="h-4 w-4 -translate-x-1 opacity-0 transition-all duration-200 ease-[cubic-bezier(0.2,0,0,1)] group-hover:translate-x-0 group-hover:opacity-100 group-active:scale-95"
				style="color: var(--color-foreground-subtle)"
				viewBox="0 0 16 16"
				fill="currentColor"
			>
				<path
					d="M2 2.5A2.5 2.5 0 0 1 4.5 0h7A2.5 2.5 0 0 1 14 2.5v11a2.5 2.5 0 0 1-2.5 2.5h-7A2.5 2.5 0 0 1 2 13.5v-11zM4.5 1A1.5 1.5 0 0 0 3 2.5v11A1.5 1.5 0 0 0 4.5 15h7a1.5 1.5 0 0 0 1.5-1.5v-11A1.5 1.5 0 0 0 11.5 1h-7z"
				/>
				<path d="M5 4h6v1H5V4zm0 2h6v1H5V6zm0 2h3v1H5V8z" />
			</svg>
		</button>
	{/if}

	<div class={sidebarInnerClass}>
		<SidebarHeader
			collapsed={isCollapsed}
			onNewChat={handleNewChat}
			onToggleCollapse={toggleCollapse}
			onSearch={handleSearch}
			logoAnimationDelay={animationIndices.logoIndex * STAGGER_DELAY}
			actionsAnimationDelay={animationIndices.actionsIndex *
				STAGGER_DELAY}
		/>

		<nav class={sectionsClass}>
			<!-- Chats accordion -->
			<SidebarAccordion
				title="Chats"
				icon="ri:chat-1-line"
				expanded={expandedSections.has("history")}
				collapsed={isCollapsed}
				onToggle={() => toggleSection("history")}
				animationDelay={animationIndices.historyLabelIndex *
					STAGGER_DELAY}
			>
				{#if recentChats.length === 0}
					<div class="px-3 py-2 text-xs text-foreground-muted">
						No chat history
					</div>
				{:else}
					{#each recentChats as session, i}
						<SidebarNavItem
							item={{
								id: session.conversation_id,
								type: "link",
								label: session.title || "Untitled",
								href: `/?conversationId=${session.conversation_id}`,
								icon: "ri:chat-1-line",
								pagespace: session.conversation_id,
							}}
							collapsed={isCollapsed}
							animationDelay={(animationIndices.chatStartIndex +
								i) *
								STAGGER_DELAY}
						/>
					{/each}
					{#if hasMoreChats}
						<SidebarNavItem
							item={{
								id: "history",
								type: "link",
								label: "View all",
								href: "/history",
								icon: "ri:history-line",
								pagespace: "history",
							}}
							collapsed={isCollapsed}
							animationDelay={(animationIndices.chatStartIndex +
								recentChats.length) *
								STAGGER_DELAY}
						/>
					{/if}
				{/if}
			</SidebarAccordion>

			<!-- Wiki section -->
			<SidebarAccordion
				title="Wiki"
				icon="ri:book-2-line"
				expanded={expandedSections.has("wiki")}
				collapsed={isCollapsed}
				onToggle={() => toggleSection("wiki")}
				animationDelay={(animationIndices.chatStartIndex +
					recentChats.length +
					2) *
					STAGGER_DELAY}
			>
				<WikiSidebarSection
					collapsed={isCollapsed}
					baseAnimationDelay={(animationIndices.chatStartIndex +
						recentChats.length +
						3) *
						STAGGER_DELAY}
				/>
			</SidebarAccordion>

			<!-- Accordion sections (Onboarding, Memory) -->
			{#each sections as section}
				<SidebarAccordion
					title={section.title}
					icon={section.icon}
					badge={section.badge}
					expanded={expandedSections.has(section.id)}
					collapsed={isCollapsed}
					onToggle={() => toggleSection(section.id)}
					animationDelay={(animationIndices.sectionHeaderIndices.get(
						section.id,
					) ?? 0) * STAGGER_DELAY}
				>
					{#each section.items as item, i}
						<SidebarNavItem
							{item}
							collapsed={isCollapsed}
							animationDelay={((animationIndices.sectionItemStartIndices.get(
								section.id,
							) ?? 0) +
								i) *
								STAGGER_DELAY}
						/>
					{/each}
				</SidebarAccordion>
			{/each}
		</nav>

		<SidebarFooter
			collapsed={isCollapsed}
			animationDelay={animationIndices.footerIndex * STAGGER_DELAY}
			onOpenBookmarks={handleOpenBookmarks}
		/>
	</div>
</aside>

<SearchModal open={isSearchOpen} onClose={closeSearch} />
<BookmarksModal open={isBookmarksOpen} onClose={closeBookmarks} />

<style>
	@reference "../../../app.css";

	/* Premium easing */
	:root {
		--ease-premium: cubic-bezier(0.2, 0, 0, 1);
	}

	/* Staggered fade-slide animation */
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

</style>
