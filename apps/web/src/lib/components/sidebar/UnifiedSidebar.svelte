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
			title: "Data",
			icon: "ri:brain-line",
			defaultExpanded: false,
			items: [
				// Coming soon - Timeline feature in development
				// {
				// 	id: "timeline",
				// 	type: "link",
				// 	label: "Timeline",
				// 	href: "/timeline",
				// 	icon: "ri:time-line",
				// 	pagespace: "timeline",
				// },
				{
					id: "sources",
					type: "link",
					label: "Sources",
					href: "/data/sources",
					icon: "ri:device-line",
					pagespace: "data/sources",
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
				? [
						"w-8 border-r border-black/5",
						"duration-400 delay-0",
						"data-[theme=dark]:border-white/10 data-[theme=night]:border-white/10",
					].join(" ")
				: ["w-60", "duration-300 delay-100"].join(" "),
		].join(" "),
	);

	const sidebarInnerClass = $derived.by(() =>
		[
			"flex h-full min-w-60 w-60 flex-col",
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
				{/if}
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
		/>
	</div>
</aside>

<SearchModal open={isSearchOpen} onClose={closeSearch} />
