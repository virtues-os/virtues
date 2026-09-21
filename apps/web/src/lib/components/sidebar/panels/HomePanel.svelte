<script lang="ts">
	/**
	 * The Home panel — the sidebar's ground, which is what it shows most of
	 * the time, because talking to the box is what most sessions are. It was
	 * the Chats panel until it held pages, projects and applets too; a user
	 * reads the panel's title as the name of the place, and "Chats" over a
	 * list of projects was the room named after one of its contents.
	 *
	 * Doors first, then groups, and the order is the order of reach:
	 *
	 *   doors     New chat, Pages, Applets, Search. The verb, the places, and
	 *             last the door that leaves the panel for the palette.
	 *             Pages and Applets each lived on the rail as a room of their
	 *             own and were rooms nobody walked to — a page is written the
	 *             way a chat is started, and an applet is run from a
	 *             conversation, so their doors stand beside the
	 *             conversation's. Pages carries a `+` on hover, the same shape
	 *             as the Projects label: the word is the list, the plus is
	 *             the new one.
	 *   Pinned    what the user chose to keep, in their own order. This was
	 *             the Desk, behind the Home tile; the shelf is more useful
	 *             where a pin is reached for than in a room you walk to first.
	 *   Projects  the rooms a chat can live in. Five, then "Show more"; the
	 *             label opens the full list. A row's hover card carries what
	 *             the row cannot: counts, the memo, the verbs.
	 *   Today / Yesterday / Earlier
	 *             the conversations, grouped by when, newest first, capped, with
	 *             the archive behind "All chats". Grouping is not reordering —
	 *             the groups are fixed and a chat only ever moves down through
	 *             them. The archive page keeps the grid, the multi-select and
	 *             the columns a 208px column can never carry.
	 *
	 * Every group folds, and its fold is remembered (`sidebarZones`): a folded
	 * group is a statement about how you want the column to look, not a place
	 * you went. A group with nothing in it is not drawn at all — an empty
	 * "Projects" with "No projects yet." under it is a feature announcing
	 * itself, and the doors above already say how to make one.
	 *
	 * Rows reveal their controls on hover — pin, more — rather than carrying
	 * them always: a column of twenty rows with forty buttons is a toolbar,
	 * not a list. Projects get the card as well, because a project has more
	 * to say than a chat does.
	 *
	 * Pinnable: a chat, a project, an applet, a page. A project cannot hold a
	 * project — the server refuses it and the menus never offer it.
	 */
	import { chatSessions, type ChatSession } from '$lib/stores/chatSessions.svelte';
	import { projectStore } from '$lib/stores/project.svelte';
	import { pinsStore } from '$lib/stores/pins.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { pendingPrompt } from '$lib/stores/pendingPrompt.svelte';
	import { sidebarZones } from '$lib/stores/sidebarZones.svelte';
	import { search } from '$lib/stores/search.svelte';
	import { contextMenu, type ContextMenuItem } from '$lib/stores/contextMenu.svelte';
	import { confirmAction, promptText } from '$lib/stores/dialog.svelte';
	import { deleteChat, updateChat, type Pin, type ProjectSummary } from '$lib/api/client';
	import { pinMenuItem, pinIconMenuItem, isPinned, togglePin } from '$lib/pins/pinAction';
	import { getProjectMenuItems } from '$lib/utils/contextMenuItems';
	import { accentCss, clothFor } from '$lib/sidebar/pin-colors';
	import { isEmoji } from '$lib/utils/iconHelpers';
	import Icon from '$lib/components/Icon.svelte';
	import AtlasIcon from '../AtlasIcon.svelte';
	import HoverCard from '../HoverCard.svelte';

	const UNTITLED = 'New chat';
	const PROJECTS_SHOWN = 5;
	const RECENTS_CAP = 20;

	// ── what is where ──────────────────────────────────────────────────────

	const pins = $derived(pinsStore.pins);
	const projects = $derived(projectStore.projects);
	let projectsExpanded = $state(false);
	const visibleProjects = $derived(
		projectsExpanded ? projects : projects.slice(0, PROJECTS_SHOWN),
	);
	const recents = $derived(chatSessions.sessions.slice(0, RECENTS_CAP));

	/** Midnight-relative, so "yesterday" means the calendar day, not 24 hours. */
	function bucketOf(s: ChatSession): 'today' | 'yesterday' | 'earlier' {
		if (!s.last_updated) return 'earlier';
		const then = new Date(s.last_updated);
		if (Number.isNaN(then.getTime())) return 'earlier';
		const startOfToday = new Date();
		startOfToday.setHours(0, 0, 0, 0);
		if (then >= startOfToday) return 'today';
		const startOfYesterday = new Date(startOfToday);
		startOfYesterday.setDate(startOfYesterday.getDate() - 1);
		if (then >= startOfYesterday) return 'yesterday';
		return 'earlier';
	}

	const chatGroups = $derived.by(() => {
		const buckets: Record<string, ChatSession[]> = { today: [], yesterday: [], earlier: [] };
		for (const s of recents) buckets[bucketOf(s)].push(s);
		return [
			{ id: 'today', label: 'Today', items: buckets.today },
			{ id: 'yesterday', label: 'Yesterday', items: buckets.yesterday },
			{ id: 'earlier', label: 'Earlier', items: buckets.earlier },
		].filter((g) => g.items.length > 0);
	});

	/** Fold state, keyed so a future group costs one string. */
	const zoneId = (id: string) => `chats.${id}`;
	const folded = (id: string) => sidebarZones.isCollapsed(zoneId(id));

	const activeRoute = $derived.by(() => {
		const pane = windowShellStore.activePane;
		const tab = pane?.tabs.find((t) => t.id === pane.activeTabId);
		return tab?.route ?? null;
	});

	function titleOf(s: ChatSession): string {
		return s.title?.trim() || UNTITLED;
	}

	function chatRoute(s: ChatSession): string {
		return `/chat/${s.conversation_id}`;
	}

	function projectRoute(p: ProjectSummary): string {
		return `/project/${p.id}`;
	}

	function isExternal(url: string): boolean {
		return /^https?:\/\//i.test(url);
	}

	/** Falls back to the url when a pin has no label, per PinTarget's contract. */
	function pinLabel(pin: Pin): string {
		return pin.label?.trim() || pin.url;
	}

	// ── doors ──────────────────────────────────────────────────────────────

	function newChat() {
		windowShellStore.openTabFromRoute('/', { label: 'New chat' });
	}

	/** Search is the ⌘K palette: one field over chats, pages and projects. */
	function openSearch() {
		search.show();
	}

	function openPages() {
		windowShellStore.openTabFromRoute('/page', { label: 'Pages', focusExisting: true });
	}

	async function newPage(e: MouseEvent) {
		e.preventDefault();
		e.stopPropagation();
		const { pagesStore } = await import('$lib/stores/pages.svelte');
		const page = await pagesStore.createNewPage();
		windowShellStore.openTabFromRoute(`/page/${page.id}`, { label: page.title });
	}

	function openApplets() {
		windowShellStore.openTabFromRoute('/applets', { label: 'Applets', focusExisting: true });
	}

	function openAllChats() {
		windowShellStore.openTabFromRoute('/chat-history', {
			label: 'All chats',
			focusExisting: true,
		});
	}

	function openProjects() {
		windowShellStore.openTabFromRoute('/projects', { label: 'Projects', focusExisting: true });
	}

	// ── opening rows ───────────────────────────────────────────────────────

	function openChat(s: ChatSession) {
		windowShellStore.openTabFromRoute(chatRoute(s), { label: titleOf(s), focusExisting: true });
	}

	function openProject(p: ProjectSummary) {
		closeCard();
		windowShellStore.openTabFromRoute(projectRoute(p), { label: p.name, focusExisting: true });
	}

	function openPin(pin: Pin) {
		if (isExternal(pin.url)) {
			window.open(pin.url, '_blank', 'noopener,noreferrer');
			return;
		}
		windowShellStore.openTabFromRoute(pin.url, { label: pinLabel(pin), focusExisting: true });
	}

	/**
	 * A new chat that already lives in the project. The binding is staged for
	 * the next chat to claim, the same hand-off "Ask this project" uses, so
	 * the create path files it and grounds retrieval there before the first
	 * message.
	 */
	function newChatIn(p: ProjectSummary) {
		closeCard();
		pendingPrompt.setProject(p.id);
		windowShellStore.openTabFromRoute('/', { label: 'New chat' });
	}

	async function newProject() {
		const name = await promptText({
			title: 'New project',
			placeholder: 'Name your project',
			confirmLabel: 'Create',
		});
		if (!name?.trim()) return;
		try {
			const project = await projectStore.create(name.trim());
			windowShellStore.openTabFromRoute(`/project/${project.id}`, {
				label: project.name,
			});
		} catch (e) {
			console.error('[HomePanel] Failed to create project:', e);
		}
	}

	// ── the verbs, per kind ────────────────────────────────────────────────

	/** Every open tab on the route takes the new name, in both panes. */
	function relabelTabs(route: string, label: string) {
		for (const pane of windowShellStore.panes) {
			for (const tab of pane.tabs) {
				if (tab.route === route) windowShellStore.updateTab(tab.id, { label });
			}
		}
	}

	async function renameChat(s: ChatSession) {
		const next = await promptText({
			title: 'Rename chat',
			initialValue: titleOf(s),
			confirmLabel: 'Rename',
		});
		const title = next?.trim();
		if (!title || title === titleOf(s)) return;
		chatSessions.applyTitle(s.conversation_id, title);
		relabelTabs(chatRoute(s), title);
		try {
			await updateChat(s.conversation_id, { title });
		} catch (e) {
			console.error('[HomePanel] rename failed:', e);
			await chatSessions.refresh();
		}
	}

	async function removeChat(s: ChatSession) {
		const ok = await confirmAction({
			title: 'Delete this chat?',
			body: `"${titleOf(s)}" will be deleted. This cannot be undone.`,
			confirmLabel: 'Delete',
			danger: true,
		});
		if (!ok) return;
		try {
			windowShellStore.closeTabsByRoute(chatRoute(s));
			await deleteChat(s.conversation_id);
			chatSessions.remove(s.conversation_id);
			windowShellStore.invalidateViewCache('chat');
		} catch (e) {
			console.error('[HomePanel] Failed to delete chat:', e);
		}
	}

	async function renameProject(p: ProjectSummary) {
		const next = await promptText({
			title: 'Rename project',
			initialValue: p.name,
			confirmLabel: 'Rename',
		});
		const name = next?.trim();
		if (!name || name === p.name) return;
		try {
			await projectStore.update(p.id, { name });
			relabelTabs(projectRoute(p), name);
		} catch (e) {
			console.error('[HomePanel] rename failed:', e);
		}
	}

	async function removeProject(p: ProjectSummary) {
		const ok = await confirmAction({
			title: 'Delete this project?',
			body: `"${p.name}" will be deleted. Its chats and pages stay; they just leave the project.`,
			confirmLabel: 'Delete',
			danger: true,
		});
		if (!ok) return;
		try {
			windowShellStore.closeTabsByRoute(projectRoute(p));
			await projectStore.remove(p.id);
		} catch (e) {
			console.error('[HomePanel] Failed to delete project:', e);
		}
	}

	function chatMenu(s: ChatSession): ContextMenuItem[] {
		const url = chatRoute(s);
		const label = titleOf(s);
		return [
			{
				id: 'open-beside',
				label: 'Open beside',
				icon: 'ri:layout-column-line',
				action: () => {
					windowShellStore.openRouteBeside(url, label);
				},
			},
			{ id: 'rename', label: 'Rename', icon: 'ri:edit-line', action: () => renameChat(s) },
			...getProjectMenuItems(url, label),
			pinMenuItem({ url, label, icon: s.icon }),
			{
				id: 'delete',
				label: 'Delete',
				icon: 'ri:delete-bin-line',
				variant: 'destructive',
				dividerBefore: true,
				action: () => removeChat(s),
			},
		];
	}

	function projectMenu(p: ProjectSummary): ContextMenuItem[] {
		const url = projectRoute(p);
		return [
			{
				id: 'open-beside',
				label: 'Open beside',
				icon: 'ri:layout-column-line',
				action: () => {
					windowShellStore.openRouteBeside(url, p.name);
				},
			},
			{ id: 'new-chat', label: 'New chat here', icon: 'ri:chat-new-line', action: () => newChatIn(p) },
			{ id: 'rename', label: 'Rename', icon: 'ri:edit-line', action: () => renameProject(p) },
			// No "Add to project" here: you can't put a project in a project.
			pinMenuItem({ url, label: p.name, icon: p.icon }),
			{
				id: 'delete',
				label: 'Delete',
				icon: 'ri:delete-bin-line',
				variant: 'destructive',
				dividerBefore: true,
				action: () => removeProject(p),
			},
		];
	}

	function pinMenu(pin: Pin, at: { x: number; y: number }): ContextMenuItem[] {
		const items: ContextMenuItem[] = [];
		if (!isExternal(pin.url)) {
			items.push({
				id: 'open-beside',
				label: 'Open beside',
				icon: 'ri:layout-column-line',
				action: () => {
					windowShellStore.openRouteBeside(pin.url, pinLabel(pin));
				},
			});
		}
		// The picker holds the icon AND the color, so one entry, not two.
		const iconItem = pinIconMenuItem(pin.url, { ...at, width: 0, height: 0 });
		if (iconItem) items.push(iconItem);
		items.push({
			id: 'unpin',
			label: 'Unpin',
			icon: 'ri:unpin-line',
			dividerBefore: items.length > 0,
			action: () => {
				void pinsStore.remove(pin.id);
			},
		});
		return items;
	}

	/** The ⋯ button and the right-click share one menu per row. */
	function showMenu(e: MouseEvent, items: ContextMenuItem[]) {
		e.preventDefault();
		e.stopPropagation();
		closeCard();
		contextMenu.show({ x: e.clientX, y: e.clientY }, items);
	}

	function showMenuFromButton(e: MouseEvent, items: ContextMenuItem[]) {
		e.preventDefault();
		e.stopPropagation();
		closeCard();
		const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
		contextMenu.show(
			{ x: r.left, y: r.bottom },
			items,
			{ anchor: { x: r.left, y: r.top, width: r.width, height: r.height }, placement: 'bottom-end' },
		);
	}

	async function quickPin(e: MouseEvent, target: { url: string; label: string; icon?: string | null }) {
		e.preventDefault();
		e.stopPropagation();
		try {
			await togglePin(target);
		} catch (err) {
			console.error('[HomePanel] pin failed:', err);
		}
	}

	// ── the hover card ─────────────────────────────────────────────────────
	//
	// One card at a time, for the project row the pointer has rested on. The
	// row arms a timer on enter and disarms it on leave; the card, once up,
	// survives a short excursion off the row so the pointer can cross to it.
	// A menu or a click closes it outright — two floating things at once is
	// one too many.

	const OPEN_AFTER_MS = 320;
	const CLOSE_AFTER_MS = 180;

	let card = $state<{ project: ProjectSummary; anchor: HTMLElement } | null>(null);
	let openTimer: ReturnType<typeof setTimeout> | null = null;
	let closeTimer: ReturnType<typeof setTimeout> | null = null;

	function clearTimers() {
		if (openTimer) clearTimeout(openTimer);
		if (closeTimer) clearTimeout(closeTimer);
		openTimer = null;
		closeTimer = null;
	}

	function armCard(project: ProjectSummary, anchor: HTMLElement) {
		clearTimers();
		if (card?.project.id === project.id) return;
		openTimer = setTimeout(() => {
			card = { project, anchor };
		}, OPEN_AFTER_MS);
	}

	function disarmCard() {
		if (openTimer) clearTimeout(openTimer);
		openTimer = null;
		if (!card) return;
		if (closeTimer) clearTimeout(closeTimer);
		closeTimer = setTimeout(closeCard, CLOSE_AFTER_MS);
	}

	function holdCard() {
		if (closeTimer) clearTimeout(closeTimer);
		closeTimer = null;
	}

	function closeCard() {
		clearTimers();
		card = null;
	}

	function onKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape' && card) closeCard();
	}

	/** The live copy of the card's project, so counts move while it is up. */
	const cardProject = $derived(
		card ? (projects.find((p) => p.id === card!.project.id) ?? card.project) : null,
	);

	function cardMeta(p: ProjectSummary): string {
		const parts: string[] = [];
		parts.push(p.chat_count === 1 ? '1 chat' : `${p.chat_count} chats`);
		parts.push(p.item_count === 1 ? '1 item' : `${p.item_count} items`);
		return parts.join(' · ');
	}
</script>

<svelte:window onkeydown={onKeydown} />

<!-- The doors sit in the flow above the groups, told apart from them by air,
     not a rule. They wear Atlas like every nav door in the shell. "Search"
     is not a field here — the field is the ⌘K palette, which has the room
     for results across chats, pages and projects. -->
<div class="panel-doors">
	<button type="button" class="panel-row panel-door" onclick={newChat}>
		<AtlasIcon name="new-chat" size={16} bare />
		<span class="panel-row-text">New chat</span>
	</button>
	<!-- The word opens the list; the + that appears beside it makes a new
	     one. A div, not a button, so the + can be a real button inside it. -->
	<div
		class="panel-row panel-door panel-row-has-actions"
		role="link"
		tabindex="0"
		onclick={openPages}
		onkeydown={(e) => {
			if (e.key === 'Enter' || e.key === ' ') {
				e.preventDefault();
				openPages();
			}
		}}
	>
		<AtlasIcon name="pages" size={16} bare />
		<span class="panel-row-text">Pages</span>
		<span class="row-actions">
			<button type="button" class="row-action" aria-label="New page" title="New page" onclick={newPage}>
				<svg width="14" height="14" viewBox="0 0 16 16" fill="none" aria-hidden="true">
					<path d="M8 3.5v9M3.5 8h9" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
				</svg>
			</button>
		</span>
	</div>
	<button type="button" class="panel-row panel-door" onclick={openApplets}>
		<AtlasIcon name="applets" size={16} bare />
		<span class="panel-row-text">Applets</span>
	</button>
	<button type="button" class="panel-row panel-door" onclick={openSearch}>
		<AtlasIcon name="search" size={16} bare />
		<span class="panel-row-text">Search</span>
	</button>
</div>

<!-- A group's head: the label folds the group; the chevron says so on
     approach and stays while the group is shut, because a folded group has
     to be able to say so — absence reads as missing data, not a decision.
     `door` makes the label ALSO a route (Projects opens the full list) and
     moves the fold onto the chevron alone. -->
{#snippet groupHead(id: string, label: string, door?: () => void, action?: import('svelte').Snippet)}
	<div class="group-head" class:folded={folded(id)}>
		{#if door}
			<button type="button" class="group-label group-door" onclick={door}>{label}</button>
			<button
				type="button"
				class="group-fold"
				aria-expanded={!folded(id)}
				aria-label={folded(id) ? `Show ${label}` : `Hide ${label}`}
				title={folded(id) ? `Show ${label}` : `Hide ${label}`}
				onclick={() => sidebarZones.toggle(zoneId(id))}
			>
				<svg class="chev" width="9" height="6" viewBox="0 0 10 6" fill="none" aria-hidden="true">
					<path d="M1 1l4 4 4-4" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" />
				</svg>
			</button>
		{:else}
			<button
				type="button"
				class="group-label group-toggle"
				aria-expanded={!folded(id)}
				title={folded(id) ? `Show ${label}` : `Hide ${label}`}
				onclick={() => sidebarZones.toggle(zoneId(id))}
			>
				<span>{label}</span>
				<svg class="chev" width="9" height="6" viewBox="0 0 10 6" fill="none" aria-hidden="true">
					<path d="M1 1l4 4 4-4" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" />
				</svg>
			</button>
		{/if}
		{#if action}
			<span class="group-actions">{@render action()}</span>
		{/if}
	</div>
{/snippet}

{#snippet newProjectAction()}
	<button type="button" class="row-action" aria-label="New project" title="New project" onclick={newProject}>
		<svg width="14" height="14" viewBox="0 0 16 16" fill="none" aria-hidden="true">
			<path d="M8 3.5v9M3.5 8h9" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
		</svg>
	</button>
{/snippet}

{#snippet chatRow(session: ChatSession)}
	{@const url = chatRoute(session)}
	{@const pinned = isPinned(url)}
	<div
		class="panel-row panel-row-has-actions"
		class:active={activeRoute === url}
		role="link"
		tabindex="0"
		title={titleOf(session)}
		onclick={() => openChat(session)}
		onkeydown={(e) => {
			if (e.key === 'Enter' || e.key === ' ') {
				e.preventDefault();
				openChat(session);
			}
		}}
		oncontextmenu={(e) => showMenu(e, chatMenu(session))}
	>
		<span class="panel-row-text">{titleOf(session)}</span>
		<span class="row-actions">
			<button
				type="button"
				class="row-action"
				class:on={pinned}
				aria-label={pinned ? 'Unpin' : 'Pin'}
				title={pinned ? 'Unpin' : 'Pin'}
				onclick={(e) => quickPin(e, { url, label: titleOf(session), icon: session.icon })}
			>
				<Icon icon={pinned ? 'ri:pushpin-fill' : 'ri:pushpin-line'} width="14" />
			</button>
			<button
				type="button"
				class="row-action"
				aria-label="More"
				title="More"
				onclick={(e) => showMenuFromButton(e, chatMenu(session))}
			>
				<Icon icon="ri:more-line" width="14" />
			</button>
		</span>
	</div>
{/snippet}

{#if pins.length > 0}
	{@render groupHead('pinned', 'Pinned')}
	<div class="sidebar-expandable" class:expanded={!folded('pinned')}>
		<div class="sidebar-expandable-inner">
			{#each pins as pin (pin.id)}
				<div
					class="panel-row panel-row-has-actions spine"
					class:active={activeRoute === pin.url}
					role="link"
					tabindex="0"
					title={pinLabel(pin)}
					onclick={() => openPin(pin)}
					onkeydown={(e) => {
						if (e.key === 'Enter' || e.key === ' ') {
							e.preventDefault();
							openPin(pin);
						}
					}}
					oncontextmenu={(e) => showMenu(e, pinMenu(pin, { x: e.clientX, y: e.clientY }))}
				>
					<!-- The glyph if the user picked one, the cloth dot if not: most
					     pins have no natural icon, which is what the dot is for. -->
					<span class="row-glyph" aria-hidden="true">
						{#if pin.icon && isEmoji(pin.icon)}
							<span class="row-emoji">{pin.icon}</span>
						{:else if pin.icon}
							<Icon icon={pin.icon} width="14" style="color: {clothFor(pin)}" />
						{:else}
							<i class="row-dot" style="background: {clothFor(pin)}"></i>
						{/if}
					</span>
					<span class="panel-row-text">{pinLabel(pin)}</span>
					<span class="row-actions">
						<button
							type="button"
							class="row-action"
							aria-label="Unpin"
							title="Unpin"
							onclick={(e) => quickPin(e, { url: pin.url, label: pinLabel(pin), icon: pin.icon })}
						>
							<Icon icon="ri:pushpin-fill" width="14" />
						</button>
						<button
							type="button"
							class="row-action"
							aria-label="More"
							title="More"
							onclick={(e) =>
								showMenuFromButton(e, pinMenu(pin, { x: e.clientX, y: e.clientY }))}
						>
							<Icon icon="ri:more-line" width="14" />
						</button>
					</span>
				</div>
			{/each}
		</div>
	</div>
{/if}

{#if projects.length > 0}
	{@render groupHead('projects', 'Projects', openProjects, newProjectAction)}
	<div class="sidebar-expandable" class:expanded={!folded('projects')}>
		<div class="sidebar-expandable-inner">
			{#each visibleProjects as project (project.id)}
				{@const url = projectRoute(project)}
				{@const pinned = isPinned(url)}
				<div
					class="panel-row panel-row-has-actions spine"
					class:active={activeRoute === url}
					class:carded={card?.project.id === project.id}
					role="link"
					tabindex="0"
					title={project.name}
					onclick={() => openProject(project)}
					onkeydown={(e) => {
						if (e.key === 'Enter' || e.key === ' ') {
							e.preventDefault();
							openProject(project);
						}
					}}
					onmouseenter={(e) => armCard(project, e.currentTarget as HTMLElement)}
					onmouseleave={disarmCard}
					oncontextmenu={(e) => showMenu(e, projectMenu(project))}
				>
					<span class="row-glyph" aria-hidden="true">
						{#if project.icon && isEmoji(project.icon)}
							<span class="row-emoji">{project.icon}</span>
						{:else}
							<Icon
								icon={project.icon || 'ri:folder-3-line'}
								width="15"
								style={accentCss(project.accent_color) ? `color: ${accentCss(project.accent_color)}` : ''}
							/>
						{/if}
					</span>
					<span class="panel-row-text">{project.name}</span>
					<span class="row-actions">
						<button
							type="button"
							class="row-action"
							class:on={pinned}
							aria-label={pinned ? 'Unpin' : 'Pin'}
							title={pinned ? 'Unpin' : 'Pin'}
							onclick={(e) => quickPin(e, { url, label: project.name, icon: project.icon })}
						>
							<Icon icon={pinned ? 'ri:pushpin-fill' : 'ri:pushpin-line'} width="14" />
						</button>
						<button
							type="button"
							class="row-action"
							aria-label="More"
							title="More"
							onclick={(e) => showMenuFromButton(e, projectMenu(project))}
						>
							<Icon icon="ri:more-line" width="14" />
						</button>
					</span>
				</div>
			{/each}
			{#if projects.length > PROJECTS_SHOWN}
				<button type="button" class="panel-row panel-more" onclick={() => (projectsExpanded = !projectsExpanded)}>
					{projectsExpanded ? 'Show less' : 'Show more'}
				</button>
			{/if}
		</div>
	</div>
{/if}

{#each chatGroups as group (group.id)}
	{@render groupHead(group.id, group.label)}
	<div class="sidebar-expandable" class:expanded={!folded(group.id)}>
		<div class="sidebar-expandable-inner">
			{#each group.items as session (session.conversation_id)}
				{@render chatRow(session)}
			{/each}
		</div>
	</div>
{/each}
{#if chatSessions.sessions.length > RECENTS_CAP}
	<button type="button" class="panel-row panel-more" onclick={openAllChats}>All chats</button>
{/if}

{#if card && cardProject}
	{@const url = projectRoute(cardProject)}
	{@const pinned = isPinned(url)}
	<HoverCard anchor={card.anchor} onenter={holdCard} onleave={disarmCard}>
		<div class="card-head">
			<span class="card-glyph" aria-hidden="true">
				{#if cardProject.icon && isEmoji(cardProject.icon)}
					<span class="row-emoji">{cardProject.icon}</span>
				{:else}
					<Icon
						icon={cardProject.icon || 'ri:folder-3-line'}
						width="16"
						style={accentCss(cardProject.accent_color) ? `color: ${accentCss(cardProject.accent_color)}` : ''}
					/>
				{/if}
			</span>
			<span class="card-name">{cardProject.name}</span>
			<button
				type="button"
				class="row-action card-pin"
				class:on={pinned}
				aria-label={pinned ? 'Unpin' : 'Pin'}
				title={pinned ? 'Unpin' : 'Pin'}
				onclick={(e) => quickPin(e, { url, label: cardProject.name, icon: cardProject.icon })}
			>
				<Icon icon={pinned ? 'ri:pushpin-fill' : 'ri:pushpin-line'} width="14" />
			</button>
		</div>
		<div class="card-meta">{cardMeta(cardProject)}</div>
		{#if cardProject.current_status}
			<!-- The catch-up memo: what the room says when you re-enter it. -->
			<div class="card-memo">{cardProject.current_status}</div>
		{/if}
		<div class="card-rule" aria-hidden="true"></div>
		<button type="button" class="card-row" onclick={() => openProject(cardProject)}>
			<Icon icon="ri:folder-open-line" width="15" />
			<span>Open project</span>
		</button>
		<button type="button" class="card-row" onclick={() => newChatIn(cardProject)}>
			<Icon icon="ri:chat-new-line" width="15" />
			<span>New chat here</span>
		</button>
	</HoverCard>
{/if}

<style>
	/* Sentence case, no tracking, no rule — design.md forbids uppercase with
	   wide tracking as something that "signals considered while doing no work",
	   and there are no hairline separators in the sidebar. A group is told
	   from its rows by being SMALLER and quieter, and by the air above it. */
	.group-head {
		display: flex;
		align-items: center;
		height: 24px;
		margin-top: 12px;
		padding-right: 6px;
		user-select: none;
	}

	.group-label {
		display: flex;
		align-items: center;
		gap: 6px;
		height: 100%;
		padding: 0 0 0 12px;
		border: none;
		background: none;
		cursor: pointer;
		font-size: 12px;
		color: var(--color-foreground-subtle);
		border-radius: var(--sidebar-interactive-radius);
		transition: color var(--sidebar-transition-duration) ease;
	}

	.group-label:hover {
		color: var(--color-foreground-muted);
	}

	.group-label:focus-visible,
	.group-fold:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: -2px;
	}

	/* The label that is also a door reads as a word until you reach it. */
	.group-door:hover {
		color: var(--color-foreground);
	}

	.group-fold {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 18px;
		height: 18px;
		margin-left: 2px;
		padding: 0;
		border: none;
		border-radius: 4px;
		background: none;
		cursor: pointer;
		color: var(--color-foreground-subtle);
	}

	.chev {
		opacity: 0;
		transform: rotate(0deg);
		transition:
			opacity 150ms ease,
			transform 220ms var(--ease-premium);
	}

	.group-head:hover .chev,
	.group-toggle:focus-visible .chev,
	.group-fold:focus-visible .chev {
		opacity: 0.8;
	}

	/* Shut: the chevron stays put, pointing at the group it would reopen. */
	.group-head.folded .chev {
		opacity: 0.8;
		transform: rotate(-90deg);
	}

	.group-actions {
		margin-left: auto;
		display: flex;
		align-items: center;
		opacity: 0;
		transition: opacity 150ms ease;
	}

	.group-head:hover .group-actions,
	.group-actions:focus-within {
		opacity: 1;
	}

	@media (prefers-reduced-motion: reduce) {
		.chev {
			transition: none;
		}
	}

	/* The doors sit in the flow above the list, told apart from it by air,
	   not a rule — the same way the groups are told from their rows. */
	.panel-doors {
		display: flex;
		flex-direction: column;
	}

	.panel-door {
		gap: 8px;
		color: var(--color-foreground);
	}

	.panel-row {
		position: relative;
		display: flex;
		align-items: center;
		gap: 8px;
		width: 100%;
		height: var(--sidebar-interactive-height);
		padding: 0 12px;
		border: none;
		border-radius: var(--sidebar-interactive-radius);
		background: none;
		cursor: pointer;
		text-align: left;
		font-size: var(--sidebar-interactive-font-size);
		color: var(--color-foreground);
	}

	.panel-row:hover,
	.panel-row.carded {
		background: var(--sidebar-hover-bg);
	}

	.panel-row.active {
		background: var(--sidebar-active-bg);
	}

	.panel-row:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: -2px;
	}

	.panel-row-text {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	/* Spines: the serif appears in the chrome exactly where ownership does —
	   the names of the user's own things (a pin, a project) get a bookface;
	   a conversation's title is a caption the model wrote, and stays sans.
	   The chrome cut sits beside an icon, and JJannon's own metrics would
	   leave the letters 1.3px above it. */
	.spine {
		font-family: var(--font-serif-ui);
		font-size: 13.5px;
		font-weight: 400;
		letter-spacing: 0.02em;
		-webkit-text-stroke: 0.2px currentColor;
	}

	.spine .panel-row-text {
		/* An integer box: 13.5 × 1.5 = 20.25px, which centers on a half pixel
		   and rounds differently by DPR and scroll position. */
		line-height: 20px;
	}

	.row-glyph {
		width: 16px;
		height: 16px;
		flex: none;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.row-emoji {
		font-size: 12px;
		line-height: 1;
	}

	.row-dot {
		width: 6.5px;
		height: 6.5px;
		border-radius: 999px;
		display: block;
		box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.12);
	}

	/* The controls appear where the pointer is and nowhere else. Kept in the
	   flow (not absolutely positioned) so the label's ellipsis makes room for
	   them; a label under a floating button is a label you cannot read. */
	.row-actions {
		display: none;
		align-items: center;
		gap: 2px;
		flex: none;
		margin-right: -6px;
	}

	.panel-row-has-actions:hover .row-actions,
	.panel-row-has-actions:focus-within .row-actions,
	.panel-row-has-actions.carded .row-actions {
		display: flex;
	}

	.row-action {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 22px;
		height: 22px;
		padding: 0;
		border: none;
		border-radius: 4px;
		background: transparent;
		color: var(--color-foreground-subtle);
		cursor: pointer;
	}

	.row-action:hover {
		background: color-mix(in srgb, var(--wash-ink) 10%, transparent);
		color: var(--color-foreground);
	}

	.row-action.on {
		color: var(--color-foreground);
	}

	.row-action:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: -2px;
	}

	.panel-more {
		color: var(--color-foreground-muted);
	}

	/* ── the card ─────────────────────────────────────────────────────── */

	.card-head {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 6px 6px 2px 8px;
	}

	.card-glyph {
		width: 16px;
		height: 16px;
		flex: none;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.card-name {
		flex: 1;
		min-width: 0;
		font-family: var(--font-serif-ui);
		font-size: 14px;
		font-weight: 400;
		letter-spacing: 0.02em;
		-webkit-text-stroke: 0.2px currentColor;
		line-height: 20px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.card-pin {
		flex: none;
	}

	.card-meta {
		padding: 0 8px 6px 32px;
		font-size: 12px;
		color: var(--color-foreground-subtle);
	}

	.card-memo {
		padding: 0 8px 8px 32px;
		font-size: 12px;
		line-height: 1.4;
		color: var(--color-foreground-muted);
		display: -webkit-box;
		-webkit-line-clamp: 3;
		line-clamp: 3;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}

	/* The one rule the card allows: it divides what the project IS from what
	   you can DO, which is the same seam a menu draws with a divider. */
	.card-rule {
		height: 1px;
		margin: 2px 0 4px;
		background: var(--color-border);
	}

	.card-row {
		display: flex;
		align-items: center;
		gap: 8px;
		width: 100%;
		height: 28px;
		padding: 0 8px;
		border: none;
		border-radius: 6px;
		background: none;
		cursor: pointer;
		text-align: left;
		font-size: var(--sidebar-interactive-font-size);
		color: var(--color-foreground);
	}

	.card-row :global(svg) {
		color: var(--color-foreground-muted);
	}

	.card-row:hover {
		background: var(--sidebar-hover-bg);
	}

	.card-row:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: -2px;
	}
</style>
