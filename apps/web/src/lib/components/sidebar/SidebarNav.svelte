<script lang="ts">
	/**
	 * The sidebar — one column, ChatGPT-shaped.
	 *
	 * This replaced the rail + swapping-panel. That two-column machine was a lens
	 * over ten storage-kind rooms; the research (and the app's own chat-primary
	 * reality) kept saying the honest shape is a single list where actions,
	 * destinations, a container and recents each own one zone and don't compete
	 * in the same hierarchy. So:
	 *
	 *   actions       New chat, New page — "I do this"
	 *   destinations  Home, Wiki, Applets — "I go here"
	 *   collection    Notebooks — the container; a notebook holds chats and pages
	 *                 and is also their retrieval scope (both, per app_notebook_items)
	 *   objects       Recents — what you touched, chats and pages together, typed
	 *   utility        Files, Sources, Settings — the plumbing, pinned to the foot
	 *
	 * A destination that used to get its section nav from the old sidebar mode
	 * (Wiki, Settings, Sources) now carries that nav IN THE PAGE (a SubNav), so
	 * one flat sidebar is enough and nothing has to swap.
	 */
	import { onMount } from 'svelte';
	import { slide } from 'svelte/transition';
	import { cubicInOut } from 'svelte/easing';
	import AtlasIcon from './AtlasIcon.svelte';
	import Icon from '$lib/components/Icon.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { chatSessions, type ChatSession } from '$lib/stores/chatSessions.svelte';
	import { pagesStore } from '$lib/stores/pages.svelte';
	import { notebookStore } from '$lib/stores/notebook.svelte';
	import { sidebarState } from '$lib/stores/sidebarState.svelte';
	import { search } from '$lib/stores/search.svelte';
	import { contextMenu, type ContextMenuItem } from '$lib/stores/contextMenu.svelte';
	import { isEmoji } from '$lib/utils/iconHelpers';
	import { isAppleKeyboard } from '$lib/utils/platform';
	import { deleteChat } from '$lib/api/client';
	import GettingStartedCard from './GettingStartedCard.svelte';
	import { gettingStarted } from '$lib/stores/gettingStarted.svelte';

	// Pages aren't loaded globally the way chats are; the sidebar is the first
	// surface that needs them (for Recents), so it loads them once on mount.
	onMount(() => {
		void pagesStore.loadPages();
	});

	const searchHint = isAppleKeyboard ? '⌘K' : 'Ctrl K';

	const reduceMotion =
		typeof window !== 'undefined' &&
		window.matchMedia('(prefers-reduced-motion: reduce)').matches;
	const slideOpts = { duration: reduceMotion ? 0 : 200, easing: cubicInOut };

	// The active tab's route — a row lights when the pane is showing it.
	const activeRoute = $derived.by(() => {
		const pane = windowShellStore.activePane;
		const tab = pane?.tabs.find((t) => t.id === pane.activeTabId);
		return tab?.route ?? null;
	});

	function isActive(href: string): boolean {
		if (!activeRoute) return false;
		if (href === '/home') return activeRoute === '/home';
		return activeRoute === href || activeRoute.startsWith(href + '/');
	}

	function open(href: string, label: string) {
		windowShellStore.openTabFromRoute(href, { label, focusExisting: true });
	}

	function newChat() {
		windowShellStore.openTabFromRoute('/', { label: 'New Chat', forceNew: true });
	}

	async function newPage() {
		const page = await pagesStore.createNewPage();
		windowShellStore.openTabFromRoute(`/page/${page.id}`, { label: page.title, forceNew: true });
	}

	function newNotebook() {
		open('/notebooks', 'Notebooks');
	}

	// ── Notebooks (the container) ───────────────────────────────
	const notebooks = $derived(notebookStore.notebooks);
	let notebooksOpen = $state(true);
	let recentsOpen = $state(true);

	// ── Recents (chats + pages, typed) ──────────────────────────
	type Recent = { kind: 'chat' | 'page'; id: string; title: string; url: string; ts: number };

	function ts(s: string | null | undefined): number {
		if (!s) return 0;
		const n = new Date(s).getTime();
		return Number.isNaN(n) ? 0 : n;
	}

	const recentsAll = $derived.by<Recent[]>(() => {
		const chats: Recent[] = chatSessions.sessions.map((c: ChatSession) => ({
			kind: 'chat',
			id: c.conversation_id,
			title: c.title?.trim() || 'New chat',
			url: `/chat/${c.conversation_id}`,
			ts: ts(c.last_updated),
		}));
		const pages: Recent[] = pagesStore.pages.map((p) => ({
			kind: 'page',
			id: p.id,
			title: p.title?.trim() || 'Untitled',
			url: `/page/${p.id}`,
			ts: ts(p.updated_at),
		}));
		return [...chats, ...pages].sort((a, b) => b.ts - a.ts);
	});

	type Filter = 'all' | 'chat' | 'page';
	let filter = $state<Filter>('all');
	let filterOpen = $state(false);
	const FILTERS: { id: Filter; label: string }[] = [
		{ id: 'all', label: 'All' },
		{ id: 'chat', label: 'Chats' },
		{ id: 'page', label: 'Pages' },
	];

	const LIMIT = 20;
	let showAll = $state(false);
	const recents = $derived.by(() => {
		const list = filter === 'all' ? recentsAll : recentsAll.filter((r) => r.kind === filter);
		return showAll ? list : list.slice(0, LIMIT);
	});
	const hasMore = $derived(
		(filter === 'all' ? recentsAll.length : recentsAll.filter((r) => r.kind === filter).length) >
			LIMIT,
	);

	function rowMenu(e: MouseEvent, r: Recent) {
		e.preventDefault();
		e.stopPropagation();
		const items: ContextMenuItem[] = [
			{
				id: 'open-beside',
				label: 'Open beside',
				icon: 'ri:layout-column-line',
				action: () => {
					windowShellStore.openRouteBeside(r.url, r.title);
				},
			},
			{
				id: 'delete',
				label: r.kind === 'chat' ? 'Delete chat' : 'Delete page',
				icon: 'ri:delete-bin-line',
				dividerBefore: true,
				action: () => void removeRecent(r),
			},
		];
		contextMenu.show({ x: e.clientX, y: e.clientY }, items);
	}

	async function removeRecent(r: Recent) {
		windowShellStore.closeTabsByRoute(r.url);
		if (r.kind === 'chat') {
			try {
				await deleteChat(r.id);
			} catch (err) {
				console.error('[SidebarNav] delete chat failed:', err);
			}
			chatSessions.remove(r.id);
		} else {
			try {
				await pagesStore.removePage(r.id);
			} catch (err) {
				console.error('[SidebarNav] delete page failed:', err);
			}
		}
	}

	function toggleCollapse() {
		sidebarState.toggle();
	}
</script>

<div class="nav">
	<header class="nav-head">
		<button type="button" class="wordmark" onclick={() => open('/home', 'Home')}>
			<span class="tri">∴</span> Virtues
		</button>
		<div class="head-actions">
			<button type="button" class="icon-btn" title={`Ask or search (${searchHint})`} aria-label="Ask or search" onclick={() => search.show()}>
				<AtlasIcon name="search" size={16} bare />
			</button>
			<button type="button" class="icon-btn" title="Hide the sidebar (⌘S)" aria-label="Hide the sidebar" onclick={toggleCollapse}>
				<Icon icon="ri:side-bar-line" width="16" />
			</button>
		</div>
	</header>

	<div class="nav-scroll">
		<!-- actions + destinations -->
		<nav class="group">
			{#snippet row(icon: string, label: string, href: string, onClick?: () => void)}
				<button
					type="button"
					class="row"
					class:active={onClick ? false : isActive(href)}
					onclick={onClick ?? (() => open(href, label))}
				>
					<AtlasIcon name={icon} size={16} bare />
					<span class="row-label">{label}</span>
				</button>
			{/snippet}

			{@render row('home', 'Home', '/home')}
			{@render row('new-chat', 'New chat', '/', newChat)}
			{@render row('pages', 'New page', '/page', newPage)}
			{@render row('wiki', 'Wiki', '/wiki')}
			{@render row('applets', 'Applets', '/applets')}
		</nav>

		<!-- Notebooks — the container -->
		<section class="group">
			<div class="group-head">
				<button type="button" class="group-title" onclick={() => (notebooksOpen = !notebooksOpen)}>
					<Icon class="chev" icon={notebooksOpen ? 'ri:arrow-down-s-line' : 'ri:arrow-right-s-line'} width="15" />
					<span>Notebooks</span>
				</button>
				<div class="group-actions">
					<button type="button" class="icon-btn sm" title="New notebook" aria-label="New notebook" onclick={newNotebook}>
						<Icon icon="ri:add-line" width="15" />
					</button>
				</div>
			</div>
			{#if notebooksOpen}
				<div class="acc" transition:slide={slideOpts}>
				{#if notebooks.length === 0}
					<p class="empty">No notebooks yet</p>
				{:else}
					{#each notebooks as nb (nb.id)}
						<button
							type="button"
							class="row"
							class:active={isActive(`/notebook/${nb.id}`)}
							onclick={() => open(`/notebook/${nb.id}`, nb.name)}
						>
							{#if nb.icon && isEmoji(nb.icon)}
								<span class="emoji">{nb.icon}</span>
							{:else}
								<AtlasIcon name="notebooks" size={16} bare />
							{/if}
							<span class="row-label">{nb.name}</span>
						</button>
					{/each}
				{/if}
				</div>
			{/if}
		</section>

		<!-- Recents — chats + pages -->
		<section class="group">
			<div class="group-head">
				<button type="button" class="group-title" onclick={() => (recentsOpen = !recentsOpen)}>
					<Icon class="chev" icon={recentsOpen ? 'ri:arrow-down-s-line' : 'ri:arrow-right-s-line'} width="15" />
					<span>Recents</span>
				</button>
				<div class="group-actions">
					<div class="filter-wrap">
						<button type="button" class="icon-btn sm" title="Filter" aria-label="Filter recents" onclick={() => (filterOpen = !filterOpen)}>
							<Icon icon="ri:filter-3-line" width="15" />
						</button>
						{#if filterOpen}
							<div class="filter-menu" role="menu">
								{#each FILTERS as f (f.id)}
									<button
										type="button"
										class="filter-item"
										class:on={filter === f.id}
										role="menuitemradio"
										aria-checked={filter === f.id}
										onclick={() => {
											filter = f.id;
											filterOpen = false;
										}}
									>
										<Icon icon="ri:check-line" width="14" />
										{f.label}
									</button>
								{/each}
							</div>
						{/if}
					</div>
				</div>
			</div>

			{#if recentsOpen}
				<div class="acc" transition:slide={slideOpts}>
			{#if recents.length === 0}
				<p class="empty">Nothing yet</p>
			{:else}
				{#each recents as r (r.kind + r.id)}
					<div
						class="row recent"
						class:active={isActive(r.url)}
						role="button"
						tabindex="0"
						onclick={() => open(r.url, r.title)}
						onkeydown={(e) => {
							if (e.key === 'Enter' || e.key === ' ') {
								e.preventDefault();
								open(r.url, r.title);
							}
						}}
						oncontextmenu={(e) => rowMenu(e, r)}
					>
						<span class="row-label">{r.title}</span>
						<button
							type="button"
							class="row-more"
							title="More"
							aria-label="More actions"
							onclick={(e) => rowMenu(e, r)}
						>
							<Icon icon="ri:more-fill" width="15" />
						</button>
					</div>
				{/each}
				{#if hasMore}
					<button type="button" class="show-more" onclick={() => (showAll = !showAll)}>
						{showAll ? 'Show less' : 'Show more'}
					</button>
				{/if}
			{/if}
				</div>
			{/if}
		</section>
	</div>

	{#if gettingStarted.loaded && !gettingStarted.unsupported && !gettingStarted.graduated}
		<GettingStartedCard />
	{/if}

	<footer class="nav-foot">
		<button type="button" class="row" class:active={isActive('/storage')} onclick={() => open('/storage', 'Files')}>
			<AtlasIcon name="drive" size={16} bare /><span class="row-label">Files</span>
		</button>
		<button type="button" class="row" class:active={isActive('/sources')} onclick={() => open('/sources', 'Sources')}>
			<AtlasIcon name="sources" size={16} bare /><span class="row-label">Sources</span>
		</button>
		<button type="button" class="row" class:active={isActive('/virtues')} onclick={() => open('/virtues/you', 'Settings')}>
			<AtlasIcon name="settings" size={16} bare /><span class="row-label">Settings</span>
		</button>
	</footer>
</div>

<style>
	.nav {
		display: flex;
		flex-direction: column;
		height: 100%;
		min-height: 0;
		width: 100%;
	}

	.nav-head {
		display: flex;
		align-items: center;
		gap: 4px;
		height: var(--chrome-row-h);
		flex: none;
		padding: 0 8px 0 12px;
	}

	.wordmark {
		display: flex;
		align-items: center;
		gap: 6px;
		flex: 1;
		min-width: 0;
		border: none;
		background: none;
		padding: 4px 4px;
		border-radius: var(--sidebar-interactive-radius);
		cursor: pointer;
		font-family: var(--font-serif);
		font-size: 15px;
		font-weight: 400;
		color: var(--color-foreground);
		text-align: left;
	}
	.wordmark:hover { background: var(--sidebar-hover-bg); }
	/* The mark sits in a 16px box centred like the row icons below it, so its
	   optical centre lands on the same vertical line as Home/New chat/etc.
	   Larger and full-ink for presence (the serif stays weight 400 — the mark
	   gets its weight from size, not a bold face). */
	.wordmark .tri {
		display: inline-block;
		width: 16px;
		text-align: center;
		font-size: 19px;
		line-height: 1;
		color: var(--color-foreground);
	}

	.head-actions { display: flex; align-items: center; gap: 2px; flex: none; }

	.icon-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 28px;
		height: 28px;
		border: none;
		border-radius: var(--sidebar-interactive-radius);
		background: none;
		cursor: pointer;
		color: var(--color-foreground-muted);
	}
	.icon-btn.sm { width: 22px; height: 22px; }
	.icon-btn :global(svg) { opacity: var(--sidebar-icon-opacity); }
	.icon-btn:hover { background: var(--sidebar-hover-bg); color: var(--color-foreground); }
	.icon-btn:hover :global(svg) { opacity: 1; }

	.nav-scroll {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		overflow-x: hidden;
		padding: 4px 8px 8px;
	}

	.group { display: flex; flex-direction: column; }
	.group + .group { margin-top: 14px; }

	.group-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 4px;
		padding-right: 2px;
		min-height: 28px;
	}

	.group-title {
		display: flex;
		align-items: center;
		gap: 4px;
		border: none;
		background: none;
		padding: 4px 6px 4px 4px;
		border-radius: var(--sidebar-interactive-radius);
		cursor: pointer;
		font-family: var(--font-serif);
		font-size: 15px;
		font-weight: 400;
		color: var(--color-foreground);
	}
	.group-title :global(svg) { opacity: 0.5; color: var(--color-foreground-muted); }

	/* The row's action icons — new, filter — appear only on hover of the group
	   (or while the filter menu is open, via focus-within). */
	.group-actions {
		display: flex;
		align-items: center;
		gap: 2px;
		opacity: 0;
		transition: opacity 120ms ease;
	}
	.group:hover .group-actions,
	.group:focus-within .group-actions { opacity: 1; }

	.row {
		display: flex;
		align-items: center;
		gap: 8px;
		width: 100%;
		height: var(--sidebar-interactive-height);
		padding: 0 8px;
		border: none;
		border-radius: var(--sidebar-interactive-radius);
		background: none;
		cursor: pointer;
		text-align: left;
		font-size: var(--sidebar-interactive-font-size);
		color: var(--color-foreground);
		position: relative;
	}
	.row :global(svg) { opacity: var(--sidebar-icon-opacity); flex: none; }
	.row:hover { background: var(--sidebar-hover-bg); }
	.row:hover :global(svg) { opacity: 1; }
	.row.active { background: color-mix(in srgb, var(--color-foreground) 9%, transparent); }
	.row.active :global(svg) { opacity: 1; }

	.row-label {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.emoji { font-size: 14px; line-height: 1; flex: none; width: 16px; text-align: center; }

	/* recents: kind tag + hover more */
	.recent .row-more {
		display: none;
		align-items: center;
		justify-content: center;
		width: 22px;
		height: 22px;
		margin-right: -4px;
		border: none;
		border-radius: 4px;
		background: none;
		cursor: pointer;
		color: var(--color-foreground-muted);
		flex: none;
	}
	.recent:hover .row-more { display: flex; }
	.recent .row-more:hover { background: color-mix(in srgb, var(--color-foreground) 12%, transparent); color: var(--color-foreground); }

	.filter-wrap { position: relative; }
	.filter-menu {
		position: absolute;
		top: 26px;
		right: 0;
		z-index: 20;
		min-width: 120px;
		padding: 4px;
		border-radius: 8px;
		border: 1px solid var(--color-border);
		background: var(--color-surface);
		box-shadow: 0 1px 2px rgba(20, 20, 28, 0.06), 0 12px 28px -10px rgba(20, 20, 28, 0.28);
	}
	.filter-item {
		display: flex;
		align-items: center;
		gap: 6px;
		width: 100%;
		height: 28px;
		padding: 0 8px;
		border: none;
		border-radius: var(--sidebar-interactive-radius);
		background: none;
		cursor: pointer;
		text-align: left;
		font-size: 13px;
		color: var(--color-foreground);
	}
	.filter-item :global(svg) { opacity: 0; }
	.filter-item.on :global(svg) { opacity: 1; }
	.filter-item:hover { background: var(--sidebar-hover-bg); }

	.empty {
		margin: 0;
		padding: 2px 8px 4px;
		font-size: 13px;
		color: var(--color-foreground-subtle);
	}

	.show-more {
		align-self: flex-start;
		border: none;
		background: none;
		padding: 4px 8px;
		margin-top: 2px;
		cursor: pointer;
		font-size: 12px;
		color: var(--color-foreground-muted);
		border-radius: var(--sidebar-interactive-radius);
	}
	.show-more:hover { color: var(--color-foreground); background: var(--sidebar-hover-bg); }

	.nav-foot {
		flex: none;
		display: flex;
		flex-direction: column;
		gap: 1px;
		padding: 6px 8px 8px;
		border-top: 1px solid var(--color-border-subtle);
	}
	.row:focus-visible,
	.icon-btn:focus-visible,
	.wordmark:focus-visible,
	.group-title:focus-visible,
	.filter-item:focus-visible,
	.show-more:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: -2px;
	}
</style>
