<script lang="ts">
	import { goto } from "$app/navigation";
	import { onMount } from "svelte";
	import { chatSessions } from "$lib/stores/chatSessions.svelte";
	import SidebarHeader from "./SidebarHeader.svelte";
	import SidebarAccordion from "./SidebarAccordion.svelte";
	import SidebarNavItem from "./SidebarNavItem.svelte";
	import SidebarFooter from "./SidebarFooter.svelte";
	import SearchModal from "./SearchModal.svelte";
	import type { SidebarSectionData } from "./types";

	const STORAGE_KEY = "virtues-sidebar-collapsed";
	const EXPANDED_SECTIONS_KEY = "virtues-sidebar-expanded";

	// Collapsed state (icon-only mode)
	let isCollapsed = $state(false);

	// Expanded sections state
	let expandedSections = $state<Set<string>>(new Set(["history"]));

	// Search modal state
	let isSearchOpen = $state(false);

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
		// Cmd+K or Ctrl+K - Search (placeholder for future)
		if ((e.metaKey || e.ctrlKey) && e.key === "k") {
			e.preventDefault();
			handleSearch();
		}
	}

	function handleSearch() {
		isSearchOpen = true;
	}

	function closeSearch() {
		isSearchOpen = false;
	}

	function handleNewChat() {
		goto("/");
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

	// Recent chats for display (last 6)
	const recentChats = $derived(chatSessions.sessions.slice(0, 6));

	// Build accordion sections (Memory only - onboarding is handled separately)
	const sections = $derived.by(() => {
		const result: SidebarSectionData[] = [];

		// Memory section (merged Data + Views)
		result.push({
			id: "memory",
			title: "Memory",
			icon: "ri:brain-line",
			defaultExpanded: false,
			items: [
				{
					id: "timeline",
					type: "link",
					label: "Timeline",
					href: "/timeline",
					icon: "ri:time-line",
					pagespace: "timeline",
				},
				{
					id: "entities",
					type: "link",
					label: "Entities",
					href: "/data/entities",
					icon: "ri:map-pin-user-line",
					pagespace: "data/entities",
				},
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
			],
		});

		return result;
	});

	// Compute global animation indices for all sidebar items (top to bottom)
	// Includes: history label, chat items, section headers, section items
	const animationIndices = $derived.by(() => {
		let globalIndex = 0;

		// Chats label gets index 0
		const historyLabelIndex = globalIndex++;

		// Then each chat
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

		return { historyLabelIndex, chatStartIndex, sectionHeaderIndices, sectionItemStartIndices };
	});

	// Initialize expanded state from section defaults
	$effect(() => {
		for (const section of sections) {
			if (section.defaultExpanded && !expandedSections.has(section.id)) {
				expandedSections = new Set([...expandedSections, section.id]);
			}
		}
	});
</script>

<aside class="sidebar" class:collapsed={isCollapsed}>
	<!-- Book Spine: When collapsed, the entire sidebar IS the clickable spine -->
	{#if isCollapsed}
		<button
			class="book-spine"
			onclick={toggleCollapse}
			aria-label="Expand sidebar"
		>
			<svg class="spine-icon" viewBox="0 0 16 16" fill="currentColor">
				<path d="M2 2.5A2.5 2.5 0 0 1 4.5 0h7A2.5 2.5 0 0 1 14 2.5v11a2.5 2.5 0 0 1-2.5 2.5h-7A2.5 2.5 0 0 1 2 13.5v-11zM4.5 1A1.5 1.5 0 0 0 3 2.5v11A1.5 1.5 0 0 0 4.5 15h7a1.5 1.5 0 0 0 1.5-1.5v-11A1.5 1.5 0 0 0 11.5 1h-7z"/>
				<path d="M5 4h6v1H5V4zm0 2h6v1H5V6zm0 2h3v1H5V8z"/>
			</svg>
		</button>
	{/if}

	<div class="sidebar-inner" class:collapsed={isCollapsed}>
		<SidebarHeader
			collapsed={isCollapsed}
			onNewChat={handleNewChat}
			onToggleCollapse={toggleCollapse}
			onSearch={handleSearch}
		/>

		<nav class="sections">
		<!-- Chats accordion -->
		<SidebarAccordion
			title="Chats"
			icon="ri:chat-1-line"
			expanded={expandedSections.has("history")}
			collapsed={isCollapsed}
			onToggle={() => toggleSection("history")}
			animationDelay={animationIndices.historyLabelIndex * 30}
		>
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
					animationDelay={(animationIndices.chatStartIndex + i) * 30}
				/>
			{/each}
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
				animationDelay={(animationIndices.sectionHeaderIndices.get(section.id) ?? 0) * 30}
			>
				{#each section.items as item, i}
					<SidebarNavItem
						{item}
						collapsed={isCollapsed}
						animationDelay={((animationIndices.sectionItemStartIndices.get(section.id) ?? 0) + i) * 30}
					/>
				{/each}
			</SidebarAccordion>
		{/each}
		</nav>

		<SidebarFooter collapsed={isCollapsed} />
	</div>
</aside>

<SearchModal open={isSearchOpen} onClose={closeSearch} />

<style>
	@reference "../../../app.css";

	/* ==============================================
	   EASING CURVES
	   ============================================== */
	:root {
		/* Heavy friction feel */
		--ease-premium: cubic-bezier(0.2, 0.0, 0, 1.0);
		/* Spring with slight bounce - heavy mass */
		--ease-spring: cubic-bezier(0.34, 1.56, 0.64, 1);
	}

	/* ==============================================
	   SIDEBAR CONTAINER - "Book Spine" Metaphor
	   Collapsed state IS the spine - solid background, never 0
	   ============================================== */
	.sidebar {
		@apply relative h-full;
		width: 260px;
		overflow: hidden;
		background: var(--surface-elevated);
		/* CLOSING: Width slides after content fades (100ms delay) */
		transition: width 300ms var(--ease-spring) 100ms;
	}

	.sidebar.collapsed {
		width: 32px;
		/* Keep background solid - the spine of a closed book */
		border-right: 1px solid rgba(0, 0, 0, 0.06);
		/* OPENING: Spring animation, immediate start */
		transition: width 400ms var(--ease-spring) 0ms;
	}

	:global([data-theme="dark"]) .sidebar.collapsed,
	:global([data-theme="night"]) .sidebar.collapsed {
		border-right: 1px solid rgba(255, 255, 255, 0.08);
	}

	/* ==============================================
	   INNER CONTENT WRAPPER - "The Slip" & "The Flow"
	   ============================================== */
	.sidebar-inner {
		@apply flex flex-col h-full;
		min-width: 260px;
		width: 260px;
		/* No opacity transition on wrapper - items handle their own fade with stagger */
	}

	.sidebar-inner.collapsed {
		pointer-events: none; /* Prevent clicking invisible links */
	}

	/* ==============================================
	   WATERFALL STAGGER - Items fade individually
	   Items use --stagger-delay CSS variable from inline style
	   ============================================== */
	/* Items hidden when sidebar collapsed */
	.sidebar-inner.collapsed :global(.nav-item),
	.sidebar-inner.collapsed :global(.accordion-header),
	.sidebar-inner.collapsed :global(.header-container),
	.sidebar-inner.collapsed :global(.footer) {
		opacity: 0;
		transform: translateX(-8px);
		/* CLOSING: All fade out together, no stagger */
		transition:
			opacity 100ms var(--ease-premium) 0ms,
			transform 100ms var(--ease-premium) 0ms;
	}

	/* Items visible with stagger when sidebar expanded */
	.sidebar-inner:not(.collapsed) :global(.nav-item),
	.sidebar-inner:not(.collapsed) :global(.accordion-header) {
		opacity: 1;
		transform: translateX(0);
		/* OPENING: Uses --stagger-delay from inline style for waterfall effect */
	}

	/* ==============================================
	   BOOK SPINE - Full-height clickable area
	   The entire collapsed sidebar IS the button
	   ============================================== */
	.book-spine {
		position: absolute;
		inset: 0;
		width: 100%;
		height: 100%;
		background: transparent;
		border: none;
		cursor: pointer;
		z-index: 10;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	/* Icon hidden by default, fades in on hover */
	.spine-icon {
		width: 16px;
		height: 16px;
		color: rgba(0, 0, 0, 0.25);
		opacity: 0;
		transform: translateX(-4px);
		transition:
			opacity 200ms var(--ease-premium),
			transform 250ms var(--ease-spring);
	}

	:global([data-theme="dark"]) .spine-icon,
	:global([data-theme="night"]) .spine-icon {
		color: rgba(255, 255, 255, 0.3);
	}

	.book-spine:hover .spine-icon {
		opacity: 1;
		transform: translateX(0);
	}

	.book-spine:active .spine-icon {
		transform: scale(0.95);
	}

	/* ==============================================
	   SECTIONS NAV
	   ============================================== */
	.sections {
		@apply flex-1 overflow-y-auto overflow-x-hidden;
		padding: 12px 8px;
	}

	.sidebar.collapsed .sections {
		padding: 12px 8px;
		@apply flex flex-col items-center;
	}

</style>
