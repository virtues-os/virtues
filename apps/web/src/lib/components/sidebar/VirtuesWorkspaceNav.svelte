<script lang="ts">
	import { chatSessions } from "$lib/stores/chatSessions.svelte";
	import { pagesStore } from "$lib/stores/pages.svelte";
	import SidebarNavItem from "./SidebarNavItem.svelte";

	const STORAGE_KEY = "virtues-nav-expanded-sections";

	interface Props {
		collapsed?: boolean;
	}

	let { collapsed = false }: Props = $props();

	// Load pages and chats on mount
	$effect(() => {
		pagesStore.loadPages();
		chatSessions.load();
	});

	// Expansion state for each section - load from localStorage or default to all expanded
	function loadExpandedSections(): Set<string> {
		if (typeof window === 'undefined') {
			return new Set(['sessions', 'pages', 'wiki', 'data', 'developer']);
		}
		const stored = localStorage.getItem(STORAGE_KEY);
		if (stored) {
			try {
				const parsed = JSON.parse(stored);
				if (Array.isArray(parsed)) {
					return new Set(parsed);
				}
			} catch {
				// Ignore parse errors
			}
		}
		return new Set(['sessions', 'pages', 'wiki', 'data', 'developer']);
	}

	let expandedSections = $state<Set<string>>(loadExpandedSections());

	// Persist expanded sections when they change
	$effect(() => {
		// Read the value to track it
		const sections = [...expandedSections];
		localStorage.setItem(STORAGE_KEY, JSON.stringify(sections));
	});

	function toggleSection(sectionId: string) {
		const newSet = new Set(expandedSections);
		if (newSet.has(sectionId)) {
			newSet.delete(sectionId);
		} else {
			newSet.add(sectionId);
		}
		expandedSections = newSet;
	}

	// Get 5 most recent chats for the sidebar
	const recentChatsItems = $derived.by(() => {
		// Sort chats by last_message_at descending and take top 5
		const sorted = [...chatSessions.sessions]
			.sort((a, b) => new Date(b.last_message_at).getTime() - new Date(a.last_message_at).getTime())
			.slice(0, 5);

		return sorted.map(chat => ({
			route: `/chat/${chat.conversation_id}`,
			label: chat.title || 'Untitled Chat',
			icon: 'ri:chat-3-line'
		}));
	});

	// Get 5 most recent pages for the sidebar (from DB via pagesStore)
	const recentPagesItems = $derived.by(() => {
		// Sort pages by updated_at descending and take top 5
		const sorted = [...pagesStore.pages]
			.sort((a, b) => new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime())
			.slice(0, 5);

		return sorted.map(page => ({
			route: `/page/${page.id}`,
			label: page.title || 'Untitled',
			icon: page.icon || 'ri:file-text-line'
		}));
	});

	// Navigation structure - code-driven, opinionated
	// Uses flat namespace-based routes
	// Recent items shown first, "All X" at bottom of each section
	const sections = $derived.by(() => [
		{
			id: 'chats',
			label: 'Chats',
			icon: 'ri:chat-3-line',
			items: [
				...recentChatsItems,
				{ route: '/chat-history', label: 'All Chats', icon: 'ri:chat-history-line' },
			]
		},
		{
			id: 'pages',
			label: 'Pages',
			icon: 'ri:file-list-3-line',
			items: [
				...recentPagesItems,
				{ route: '/page', label: 'All Pages', icon: 'ri:file-list-3-line' },
			]
		},
		{
			id: 'wiki',
			label: 'Wiki',
			icon: 'ri:book-2-line',
			items: [
				{ route: '/wiki', label: 'Overview', icon: 'ri:compass-line' },
				{ route: '/day', label: 'Today', icon: 'ri:calendar-todo-line' },
				{ route: '/person', label: 'People', icon: 'ri:user-line' },
				{ route: '/place', label: 'Places', icon: 'ri:map-pin-line' },
				{ route: '/org', label: 'Organizations', icon: 'ri:building-line' },
			]
		},
		{
			id: 'data',
			label: 'Data',
			icon: 'ri:database-2-line',
			items: [
				{ route: '/drive', label: 'Drive', icon: 'ri:folder-line' },
				{ route: '/sources', label: 'Sources', icon: 'ri:plug-line' },
			]
		},
		{
			id: 'developer',
			label: 'Developer',
			icon: 'ri:code-s-slash-line',
			items: [
				{ route: '/virtues/sql', label: 'SQL Viewer', icon: 'ri:database-line' },
				{ route: '/virtues/sitemap', label: 'Sitemap', icon: 'ri:road-map-line' },
				{ route: '/virtues/terminal', label: 'Terminal', icon: 'ri:terminal-box-line' },
				{ route: '/virtues/lake', label: 'Lake', icon: 'ri:database-2-line' },
				{ route: '/virtues/jobs', label: 'Jobs', icon: 'ri:refresh-line' },
			]
		},
	]);

</script>

<nav class="virtues-nav" class:collapsed>
	{#each sections as section (section.id)}
		{@const isExpanded = expandedSections.has(section.id)}
		<div class="sidebar-dnd-item">
		<div class="section">
			<button
				class="sidebar-interactive"
				onclick={() => toggleSection(section.id)}
			>
				{#if !collapsed}
					<span class="sidebar-label">{section.label}</span>
					<svg
						class="sidebar-chevron"
						class:expanded={isExpanded}
						width="10"
						height="10"
						viewBox="0 0 12 12"
						fill="none"
					>
						<path
							d="M4.5 3L7.5 6L4.5 9"
							stroke="currentColor"
							stroke-width="1.5"
							stroke-linecap="round"
							stroke-linejoin="round"
						/>
					</svg>
				{/if}
			</button>

			{#if !collapsed}
				<div class="sidebar-expandable" class:expanded={isExpanded}>
					<div class="sidebar-expandable-inner">
						{#each section.items as item (item.route)}
							<div class="sidebar-dnd-item">
								<SidebarNavItem
									item={{
										id: item.route,
										type: "link",
										label: item.label,
										icon: item.icon,
										href: item.route,
									}}
									{collapsed}
									indent={1}
									isSystemItem={true}
								/>
							</div>
						{/each}
					</div>
				</div>
			{/if}
		</div>
		</div>
	{/each}
</nav>

<style>
	@reference "../../../app.css";
	@reference "$lib/styles/sidebar.css";

	.virtues-nav {
		display: flex;
		flex-direction: column;
	}

	.virtues-nav.collapsed {
		align-items: center;
	}

	.section {
		display: flex;
		flex-direction: column;
	}

</style>
