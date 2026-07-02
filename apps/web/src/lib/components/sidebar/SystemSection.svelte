<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { pagesStore } from "$lib/stores/pages.svelte";
	import { listPages, type ViewEntity } from "$lib/api/client";
	import { chatSessions } from "$lib/stores/chatSessions.svelte";
	import type { SystemSection } from "$lib/sidebar/sections";
	import SidebarNavItem from "./SidebarNavItem.svelte";

	interface Props {
		section: SystemSection;
		collapsed?: boolean;
		accentColor?: string | null;
	}

	let {
		section,
		collapsed = false,
		accentColor = null,
	}: Props = $props();

	// Local expand state (folder/view expansion no longer lives in the window shell store)
	let isExpanded = $state(false);

	// Smart section data
	let smartItems = $state<ViewEntity[]>([]);
	let smartLoading = $state(false);
	let lastCacheVersion = $state(-1);

	// The chat list is served from the shared, reactive chatSessions store (single
	// source of truth) instead of a private cached fetch — so titles/renames/deletes
	// propagate here the instant any surface mutates the store. Pages keep the
	// existing fetch+cache path.
	const isChatSection = $derived(section.type === 'smart' && section.namespace === 'chat');

	const chatEntities = $derived<ViewEntity[]>(
		chatSessions.sessions.slice(0, section.limit ?? 8).map((c) => ({
			id: `/chat/${c.conversation_id}`,
			name: c.title ?? 'Untitled',
			namespace: 'chat',
			icon: c.icon || 'ri:chat-1-line',
			updated_at: c.last_updated ?? undefined,
		})),
	);

	const displayItems = $derived(isChatSection ? chatEntities : smartItems);
	const displayLoading = $derived(
		isChatSection ? chatSessions.isLoading && chatSessions.sessions.length === 0 : smartLoading,
	);

	// Seed from cache on mount
	{
		const cached = windowShellStore.smartSectionCache.get(section.id);
		if (cached) {
			smartItems = cached;
			lastCacheVersion = windowShellStore.viewCacheVersion;
		}
	}

	// Fetch smart section items when expanded or cache invalidated (pages only —
	// chat is store-backed and handled by the effect below).
	$effect.pre(() => {
		if (section.type !== 'smart' || isChatSection) return;
		const currentVersion = windowShellStore.viewCacheVersion;
		if (isExpanded && lastCacheVersion !== currentVersion) {
			const forceRefresh = lastCacheVersion !== -1;
			fetchSmartItems(currentVersion, forceRefresh);
		}
	});

	// Chat section: populate the shared store once, the first time it's expanded.
	// Guarded so an empty result (a user with no chats) can't re-trigger the load.
	let chatsRequested = $state(false);
	$effect(() => {
		if (
			isChatSection &&
			isExpanded &&
			!chatsRequested &&
			chatSessions.sessions.length === 0 &&
			!chatSessions.isLoading
		) {
			chatsRequested = true;
			chatSessions.load();
		}
	});

	async function fetchSmartItems(cacheVersion: number, forceRefresh = false) {
		if (smartLoading) return;

		if (!forceRefresh) {
			const cached = windowShellStore.smartSectionCache.get(section.id);
			if (cached) {
				smartItems = cached;
				lastCacheVersion = cacheVersion;
				return;
			}
		}

		smartLoading = true;
		lastCacheVersion = cacheVersion;

		try {
			let entities: ViewEntity[] = [];

			if (section.namespace === 'chat') {
				const res = await fetch('/api/chats');
				const data = await res.json();
				entities = (data.conversations || [])
					.slice(0, section.limit ?? 8)
					.map((c: { conversation_id: string; title: string; icon?: string; last_updated?: string }) => ({
						id: `/chat/${c.conversation_id}`,
						name: c.title,
						namespace: 'chat',
						icon: c.icon || 'ri:chat-1-line',
						updated_at: c.last_updated,
					}));
			} else if (section.namespace === 'page') {
				const data = await listPages(section.limit ?? 8);
				entities = (data.pages || []).map((p: { id: string; title: string; icon?: string | null; updated_at?: string }) => ({
					id: `/page/${p.id}`,
					name: p.title,
					namespace: 'page',
					icon: p.icon || 'ri:file-text-line',
					updated_at: p.updated_at,
				}));
			}

			smartItems = entities;
			windowShellStore.updateSmartSectionCache(section.id, entities);
		} catch (e) {
			console.error(`[SystemSection] Failed to fetch ${section.namespace} items:`, e);
		} finally {
			smartLoading = false;
		}
	}

	function toggleExpanded() {
		isExpanded = !isExpanded;
	}

	function handleClick(e: MouseEvent) {
		e.preventDefault();
		e.stopPropagation();
		toggleExpanded();
	}

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === "Enter" || e.key === " ") {
			e.preventDefault();
			toggleExpanded();
		}
	}

	function handleQuickAdd(e: MouseEvent) {
		e.preventDefault();
		e.stopPropagation();
		if (section.quickAdd === 'chat') handleNewChat();
		else if (section.quickAdd === 'page') handleNewPage();
	}

	function handleNewChat() {
		windowShellStore.openTabFromRoute("/", {
			label: "New Chat",
			forceNew: true,
			preferEmptyPane: true,
		});
	}

	async function handleNewPage() {
		try {
			const page = await pagesStore.createNewPage();
			windowShellStore.openTabFromRoute(`/page/${page.id}`, {
				label: page.title,
				forceNew: true,
				preferEmptyPane: true,
			});
		} catch (e) {
			console.error("[SystemSection] Failed to create page:", e);
		}
	}

	function handleMoreClick(e: MouseEvent) {
		e.preventDefault();
		e.stopPropagation();
		if (section.moreRoute) {
			windowShellStore.openTabFromRoute(section.moreRoute);
		}
	}
</script>

{#if !collapsed}
	{#if section.type === 'link' && section.href}
		<div class="system-section">
			<SidebarNavItem
				item={{
					id: section.id,
					type: 'link',
					label: section.name,
					icon: section.icon,
					href: section.href,
				}}
				{collapsed}
				{accentColor}
				isSystemItem={true}
				onQuickAdd={section.quickAdd ? handleQuickAdd : undefined}
				quickAddTitle={section.quickAdd === 'chat' ? 'New Chat' : section.quickAdd === 'page' ? 'New Page' : undefined}
			/>
		</div>
	{:else}
	<div class="system-section">
		<div
			class="sidebar-interactive system"
			role="button"
			tabindex="0"
			onclick={handleClick}
			onkeydown={handleKeydown}
		>
			<span class="folder-toggle" class:expanded={isExpanded}>
				<span class="folder-toggle-icon">
					<Icon icon={section.icon} width="16" class="sidebar-icon" />
				</span>
				<svg
					class="folder-toggle-chevron"
					width="12"
					height="12"
					viewBox="0 0 16 16"
					fill="none"
				>
					<path
						d="M6 4L10 8L6 12"
						stroke="currentColor"
						stroke-width="1.5"
						stroke-linecap="round"
						stroke-linejoin="round"
					/>
				</svg>
			</span>

			<span class="sidebar-label">{section.name}</span>

			<span class="sidebar-item-actions">
				{#if section.moreRoute}
					<button class="sidebar-item-action" title="View All" onclick={handleMoreClick}>
						<svg
							width="14"
							height="14"
							viewBox="0 0 16 16"
							fill="currentColor"
						>
							<circle cx="4" cy="8" r="1.25" />
							<circle cx="8" cy="8" r="1.25" />
							<circle cx="12" cy="8" r="1.25" />
						</svg>
					</button>
				{/if}
				{#if section.quickAdd}
					<button
						class="sidebar-item-action"
						title="New {section.quickAdd === 'chat' ? 'Chat' : 'Page'}"
						onclick={handleQuickAdd}
					>
						<svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
							<path d="M8 3.5v9M3.5 8h9" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" fill="none" />
						</svg>
					</button>
				{/if}
			</span>
		</div>

		<!-- Section contents — CSS grid expand/collapse -->
		<div class="sidebar-expandable-content" class:expanded={isExpanded}>
			<div class="sidebar-expandable-overflow">
				<div class="sidebar-expandable-inner">
					{#if section.type === 'static' && section.items}
						{#each section.items as item (item.id)}
							<SidebarNavItem
								item={{
									id: item.id,
									type: 'link',
									label: item.label,
									icon: item.icon,
									href: item.href,
								}}
								{collapsed}
								indent={1}
								{accentColor}
								isSystemItem={true}
							/>
						{/each}
					{:else if section.type === 'smart'}
						{#if displayLoading && displayItems.length === 0}
							<div class="sidebar-loading">Loading...</div>
						{:else if displayItems.length === 0}
							<div class="sidebar-empty">No matches</div>
						{:else}
							{#each displayItems as item (item.id)}
								<SidebarNavItem
									item={{
										id: item.id,
										type: 'link',
										label: item.name,
										icon: item.icon || 'ri:file-line',
										href: item.id,
									}}
									{collapsed}
									indent={1}
									{accentColor}
									isSystemItem={true}
								/>
							{/each}
						{/if}
					{/if}
				</div>
			</div>
		</div>
	</div>
	{/if}
{/if}

<style>
	@reference "../../../app.css";
	@reference "$lib/styles/sidebar.css";

	.system-section {
		display: flex;
		flex-direction: column;
	}

	/* ------- Icon ↔ Chevron slide toggle (matches UnifiedFolder) ------- */
	.folder-toggle {
		position: relative;
		width: 16px;
		height: 16px;
		flex-shrink: 0;
		overflow: hidden;
		cursor: pointer;
	}

	.folder-toggle-icon,
	.folder-toggle-chevron {
		position: absolute;
		inset: 0;
		display: flex;
		align-items: center;
		justify-content: center;
		transition:
			opacity 120ms ease,
			transform 160ms ease;
	}

	.folder-toggle-icon {
		opacity: 1;
		transform: translateY(0);
	}

	.folder-toggle-chevron {
		opacity: 0;
		transform: translateY(6px);
		color: var(--color-foreground-subtle);
		margin: auto;
	}

	.sidebar-interactive:hover .folder-toggle-icon {
		opacity: 0;
		transform: translateY(-6px);
	}

	.sidebar-interactive:hover .folder-toggle-chevron {
		opacity: 1;
		transform: translateY(0);
	}

	.folder-toggle.expanded .folder-toggle-icon {
		opacity: 0;
		transform: translateY(-6px);
	}

	.folder-toggle.expanded .folder-toggle-chevron {
		opacity: 1;
		transform: translateY(0) rotate(90deg);
	}

	.sidebar-interactive:hover .folder-toggle.expanded .folder-toggle-chevron {
		transform: translateY(0) rotate(90deg);
	}

	/* CSS grid expand/collapse (matches UnifiedFolder) */
	.sidebar-expandable-content {
		display: grid;
		grid-template-rows: 0fr;
		transition: grid-template-rows 150ms ease;
	}

	.sidebar-expandable-content.expanded {
		grid-template-rows: 1fr;
	}

	.sidebar-expandable-overflow {
		overflow: hidden;
		padding-top: 4px;
	}
</style>
