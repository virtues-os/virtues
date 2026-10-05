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
	 *   Projects  the rooms a chat can live in. Five, then "Show more". The
	 *             label folds the group like every other label; the full
	 *             list is behind the head's ⋯ (it used to be the label
	 *             itself, which made Projects the one head that navigated
	 *             when the others folded — a click that did a different
	 *             thing in one place). A row's hover card carries what the
	 *             row cannot: counts, the memo, the verbs.
	 *   Today / Recent
	 *             the chats AND the pages, blended by when they last moved,
	 *             newest first, capped. No "All chats" door under the list:
	 *             the panel ends where the recents end, and the full lists
	 *             are rooms of their own (Pages above, All chats via search).
	 *             Two groups, not three: "Yesterday" was a calendar fact
	 *             nobody navigated by, and it cost a heading and a fold for
	 *             one day's worth of rows. Grouping is not reordering — a
	 *             row only ever moves down from Today into Recent. The
	 *             archive page keeps the grid, the multi-select and the
	 *             columns a 208px column can never carry.
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
	import { pagesStore } from '$lib/stores/pages.svelte';
	import { pinsStore } from '$lib/stores/pins.svelte';
	import { chatActivity } from '$lib/stores/chatActivity.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { sidebarZones } from '$lib/stores/sidebarZones.svelte';
	import { search } from '$lib/stores/search.svelte';
	import { contextMenu, type ContextMenuItem } from '$lib/stores/contextMenu.svelte';
	import { promptText } from '$lib/stores/dialog.svelte';
	import { deleteChat, type PageSummary, type Pin, type ProjectSummary } from '$lib/api/client';
	import { pinIdentity, renameRef, ownedRef, untitled } from '$lib/refs/identity.svelte';
	import { pinMenuItem, pinIconMenuItem, isPinned, togglePin } from '$lib/pins/pinAction';
	import { getProjectMenuItems } from '$lib/utils/contextMenuItems';
	import { notifyTrashed, routeIfOpen } from '$lib/utils/toasts';
	import { toast } from 'svelte-sonner';
	import { clothFor, projectColor } from '$lib/sidebar/pin-colors';
	import { isEmoji, PROJECT_ICON } from '$lib/utils/iconHelpers';
	import {
		droppedRefUrl,
		fileIntoProject,
		isRefDrag,
		memberIcon,
		memberName,
		newChatInProject,
		newProject,
		openProjects,
		projectRowMenuItems,
		projectMemberUrl,
		startRefDrag,
	} from '$lib/utils/projectActions';
	import { dndManager, PROJECT_DROP_ATTR } from '$lib/stores/dndManager.svelte';
	import ProjectGlyph from '$lib/components/ProjectGlyph.svelte';
	import Icon from '$lib/components/Icon.svelte';
	import AtlasIcon from '../AtlasIcon.svelte';
	import HoverCard from '../HoverCard.svelte';

	const PROJECTS_SHOWN = 5;
	const RECENTS_CAP = 20;

	// ── what is where ──────────────────────────────────────────────────────

	const pins = $derived(pinsStore.pins);
	const projects = $derived(projectStore.projects);
	let projectsExpanded = $state(false);
	const visibleProjects = $derived(
		projectsExpanded ? projects : projects.slice(0, PROJECTS_SHOWN),
	);
	// The page list is loaded by the rooms that show pages; this panel is
	// mounted before any of them, so it asks once. The store dedupes nothing,
	// hence the guard on both the list and the in-flight flag.
	if (!pagesStore.pages.length && !pagesStore.pagesLoading) pagesStore.loadPages();

	/** One row of the recents: a chat or a page, with when it last moved. */
	type RecentRow =
		| { kind: 'chat'; key: string; ts: number; session: ChatSession }
		| { kind: 'page'; key: string; ts: number; page: PageSummary };

	function stamp(iso: string | null | undefined): number {
		if (!iso) return 0;
		const t = Date.parse(iso);
		return Number.isNaN(t) ? 0 : t;
	}

	/** Chats and pages, newest first, capped as one list. */
	const recents = $derived.by<RecentRow[]>(() => {
		const chats: RecentRow[] = chatSessions.sessions.map((session) => ({
			kind: 'chat',
			key: `chat:${session.conversation_id}`,
			// The last message, not the row's last touch: filing or renaming
			// an old chat must not move it into Today.
			ts: stamp(session.last_message_at || session.last_updated),
			session,
		}));
		const pages: RecentRow[] = pagesStore.pages.map((page) => ({
			kind: 'page',
			key: `page:${page.id}`,
			ts: stamp(page.updated_at),
			page,
		}));
		return [...chats, ...pages].sort((a, b) => b.ts - a.ts).slice(0, RECENTS_CAP);
	});

	/** Midnight-relative, so "today" means the calendar day, not 24 hours. */
	function isToday(ts: number): boolean {
		if (!ts) return false;
		const startOfToday = new Date();
		startOfToday.setHours(0, 0, 0, 0);
		return ts >= startOfToday.getTime();
	}

	const recentGroups = $derived.by(() => {
		const today: RecentRow[] = [];
		const recent: RecentRow[] = [];
		for (const r of recents) (isToday(r.ts) ? today : recent).push(r);
		return [
			{ id: 'today', label: 'Today', items: today },
			{ id: 'recent', label: 'Recent', items: recent },
		].filter((g) => g.items.length > 0);
	});


	/** Fold state, keyed so a future group costs one string. */
	const zoneId = (id: string) => `chats.${id}`;
	const folded = (id: string) => sidebarZones.isCollapsed(zoneId(id));

	/* The fold's duration scales with the group's height, clamped. At one
	   fixed duration a 20-row group moved four times as fast as a 5-row one
	   and its first frames jumped 100px+, which reads as strobing, not motion.
	   Head and body take the same value so chevron and fold start and stop
	   together. */
	const foldStyle = (rows: number) =>
		`--fold-ms: ${Math.min(280, Math.max(180, 140 + rows * 8))}ms`;

	/* Folding near the foot of a scrolled column shortens the content under the
	   scroll position, and the browser clamps scrollTop down every frame to
	   match. Everything above — including the head just clicked — slid down
	   under the pointer while the group folded. The spacer takes up the lost
	   height so the head stays where the click was, and gives it back as the
	   user scrolls up, where giving it back cannot move anything on screen. */
	let foldSpacer = $state(0);
	let spacerEl: HTMLDivElement | undefined = $state();
	const scroller = () => spacerEl?.closest<HTMLElement>('.panel-body') ?? null;

	function toggleGroup(id: string, head: HTMLElement) {
		const el = scroller();
		const body = head.nextElementSibling as HTMLElement | null;
		if (el && body && !folded(id)) {
			const lost = body.offsetHeight;
			const needed = el.scrollTop + el.clientHeight - (el.scrollHeight - foldSpacer - lost);
			foldSpacer = Math.max(foldSpacer, needed);
		}
		sidebarZones.toggle(zoneId(id));
	}

	/** Shrink the spacer to what the current scroll position still stands on. */
	function releaseSpacer() {
		const el = scroller();
		if (!el || foldSpacer === 0) return;
		const needed = el.scrollTop + el.clientHeight - (el.scrollHeight - foldSpacer);
		foldSpacer = Math.max(0, Math.min(foldSpacer, needed));
	}

	$effect(() => {
		const el = scroller();
		if (!el) return;
		el.addEventListener('scroll', releaseSpacer, { passive: true });
		return () => el.removeEventListener('scroll', releaseSpacer);
	});

	const activeRoute = $derived.by(() => {
		const pane = windowShellStore.activePane;
		const tab = pane?.tabs.find((t) => t.id === pane.activeTabId);
		return tab?.route ?? null;
	});

	function titleOf(s: ChatSession): string {
		return s.title?.trim() || untitled('chat');
	}

	function chatRoute(s: ChatSession): string {
		return `/chat/${s.conversation_id}`;
	}

	function projectRoute(p: ProjectSummary): string {
		return `/project/${p.id}`;
	}

	function pageTitle(p: PageSummary): string {
		return p.title?.trim() || untitled('page');
	}

	function pageRoute(p: PageSummary): string {
		return `/page/${p.id}`;
	}

	function isExternal(url: string): boolean {
		return /^https?:\/\//i.test(url);
	}

	/** A pin's name is its thing's name (refs/identity). */
	function pinLabel(pin: Pin): string {
		return pinIdentity(pin).title;
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

// ── opening rows ───────────────────────────────────────────────────────

	function openChat(s: ChatSession) {
		windowShellStore.openTabFromRoute(chatRoute(s), { label: titleOf(s), focusExisting: true });
	}

	function openPage(p: PageSummary) {
		windowShellStore.openTabFromRoute(pageRoute(p), { label: pageTitle(p), focusExisting: true });
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
		newChatInProject(p);
	}

// ── the verbs, per kind ────────────────────────────────────────────────

	/** One rename for anything with a url: the thing, its tabs, its pin (refs/identity). */
	async function renameUrl(url: string, current: string, dialogTitle: string) {
		const next = await promptText({ title: dialogTitle, initialValue: current, confirmLabel: 'Rename' });
		const title = next?.trim();
		if (!title || title === current) return;
		try {
			await renameRef(url, title);
		} catch (e) {
			console.error('[HomePanel] rename failed:', e);
			toast.error(`Your server couldn't rename "${current}"`, { description: 'It keeps its old name. Try again' });
		}
	}

	function renameChat(s: ChatSession) {
		return renameUrl(chatRoute(s), titleOf(s), 'Rename chat');
	}

	function renamePage(p: PageSummary) {
		return renameUrl(pageRoute(p), pageTitle(p), 'Rename page');
	}

	// The three deletes below ask nothing. Each is a trip to Recently deleted
	// and each toast carries the Undo, so a dialog here only stands between you
	// and a reversible act.
	async function removePage(p: PageSummary) {
		// Before the delete: `removePage` closes the tabs, so this is the only
		// moment we can tell whether Undo has a tab to put back.
		const reopen = routeIfOpen(pageRoute(p));
		try {
			await pagesStore.removePage(p.id);
			notifyTrashed({ kind: 'page', id: p.id, name: pageTitle(p), reopen });
		} catch (e) {
			console.error('[HomePanel] Failed to delete page:', e);
			toast.error(`Your server couldn't delete "${pageTitle(p)}"`, {
				description: "It's still here. Try again",
			});
		}
	}

	async function removeChat(s: ChatSession) {
		const reopen = routeIfOpen(chatRoute(s));
		try {
			windowShellStore.closeTabsByRoute(chatRoute(s));
			await deleteChat(s.conversation_id);
			chatSessions.remove(s.conversation_id);
			windowShellStore.invalidateViewCache('chat');
			notifyTrashed({
				kind: 'chat',
				id: s.conversation_id,
				name: titleOf(s),
				reopen,
			});
		} catch (e) {
			console.error('[HomePanel] Failed to delete chat:', e);
			toast.error(`Your server couldn't delete "${titleOf(s)}"`, {
				description: "It's still here. Try again",
			});
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

	function pageMenu(p: PageSummary): ContextMenuItem[] {
		const url = pageRoute(p);
		const label = pageTitle(p);
		return [
			{
				id: 'open-beside',
				label: 'Open beside',
				icon: 'ri:layout-column-line',
				action: () => {
					windowShellStore.openRouteBeside(url, label);
				},
			},
			{ id: 'rename', label: 'Rename', icon: 'ri:edit-line', action: () => renamePage(p) },
			...getProjectMenuItems(url, label),
			pinMenuItem({ url, label, icon: p.icon }),
			{
				id: 'delete',
				label: 'Delete',
				icon: 'ri:delete-bin-line',
				variant: 'destructive',
				dividerBefore: true,
				action: () => removePage(p),
			},
		];
	}

	/** The head's ⋯: the list's own doors, off the label so the label can fold. */
	function projectsHeadMenu(): ContextMenuItem[] {
		return [
			{ id: 'all', label: 'All projects', icon: PROJECT_ICON, action: openProjects },
			{ id: 'new', label: 'New project', icon: 'ri:add-line', action: newProject },
		];
	}

	/** The shared project menu; the hover card closes first, one floating thing at a time. */
	function projectMenu(p: ProjectSummary): ContextMenuItem[] {
		return projectRowMenuItems(p).map((item) => ({
			...item,
			action: item.action
				? () => {
						closeCard();
						return item.action?.();
					}
				: undefined,
		}));
	}

	/**
	 * A pin's menu is its thing's menu — rename, file, delete or archive act on
	 * the chat, page or project, as they do from its own row — with the icon
	 * picker added, and Unpin always there. A thing this client has not loaded
	 * (an old chat, a page past the list) gets a stand-in row built from the
	 * pin's resolved name; the verbs only need the id.
	 */
	function pinMenu(pin: Pin, at: { x: number; y: number }): ContextMenuItem[] {
		const owned = ownedRef(pin.url);
		const look = pinIdentity(pin);
		const iconItem = pinIconMenuItem(pin.url, { ...at, width: 0, height: 0 });
		let base: ContextMenuItem[] | null = null;
		if (owned?.kind === 'chat') {
			const s = chatSessions.sessions.find((c) => c.conversation_id === owned.id);
			base = chatMenu(
				s ?? {
					conversation_id: owned.id,
					title: look.title,
					icon: look.icon,
					last_updated: null,
					first_message_at: '',
					last_message_at: '',
					message_count: 0,
					model_used: null,
					provider: '',
				},
			);
		} else if (owned?.kind === 'page') {
			const p = pagesStore.pages.find((x) => x.id === owned.id);
			base = pageMenu(
				p ?? {
					id: owned.id,
					title: look.title,
					project_id: null,
					icon: look.icon,
					icon_color: look.color,
					cover_url: null,
					tags: null,
					created_at: '',
					updated_at: '',
				},
			);
		} else if (owned?.kind === 'project') {
			const p = projects.find((x) => x.id === owned.id);
			base = projectRowMenuItems(p ?? { id: owned.id, name: look.title, icon: look.icon, archived_at: null });
		}
		if (base) {
			const rename = base.findIndex((i) => i.id === 'rename');
			if (iconItem) base.splice(rename >= 0 ? rename + 1 : base.length, 0, iconItem);
			if (!base.some((i) => i.id === 'pin-sidebar')) {
				base.push({
					id: 'unpin',
					label: 'Unpin',
					icon: 'ri:unpin-line',
					dividerBefore: true,
					action: () => void pinsStore.remove(pin.id),
				});
			}
			return base;
		}

		// No record behind it: an external URL or an app screen, where the pin
		// is the thing, so its name and icon are the pin's own.
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
		if (look.kind === 'web' || look.kind === 'route') {
			items.push({
				id: 'rename',
				label: 'Rename',
				icon: 'ri:edit-line',
				action: () => void renameUrl(pin.url, look.title, 'Rename pin'),
			});
		}
		// The picker holds the icon AND the color, so one entry, not two.
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

	// Quick enough to feel like the row opening, slow enough that a pointer
	// crossing the list on its way somewhere doesn't pop a card per row. Once
	// one is up, the next row's opens at once: you are reading cards now.
	const OPEN_AFTER_MS = 90;
	const CLOSE_AFTER_MS = 180;

	let card = $state<{ project: ProjectSummary; anchor: HTMLElement } | null>(null);

	// The project row a dragged sidebar row is over. A dragged TAB is tracked
	// by dndManager instead (it is not an HTML drag); either lights the row.
	let rowDropTarget = $state<string | null>(null);
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
		// Start reading what is inside now, so it is there when the card is.
		void projectStore.get(project.id).catch(() => {});
		if (card) {
			card = { project, anchor };
			return;
		}
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

	/**
	 * What the card lists: the few most recent chats and the first few files
	 * and pages, one click from anywhere. It is the reason the card exists: a
	 * way into the thing you were after without landing on the project first.
	 */
	const CARD_ROWS = 3;
	const cardDetail = $derived(card ? projectStore.getCached(card.project.id) : undefined);
	const cardChats = $derived((cardDetail?.chats ?? []).slice(0, CARD_ROWS));
	const cardItems = $derived(
		(cardDetail?.items ?? []).filter((i) => !i.url.startsWith('/chat/')).slice(0, CARD_ROWS),
	);

	function openFromCard(route: string, label: string) {
		closeCard();
		if (isExternal(route)) {
			window.open(route, '_blank', 'noopener,noreferrer');
			return;
		}
		windowShellStore.openTabFromRoute(route, { label, focusExisting: true });
	}

	/** The live copy of the card's project, so counts move while it is up. */
	const cardProject = $derived(
		card ? (projects.find((p) => p.id === card!.project.id) ?? card.project) : null,
	);

	function cardMeta(p: ProjectSummary): string {
		const parts: string[] = [];
		if (p.chat_count > 0) parts.push(p.chat_count === 1 ? '1 chat' : `${p.chat_count} chats`);
		if (p.item_count > 0) parts.push(p.item_count === 1 ? '1 item' : `${p.item_count} items`);
		return parts.join(' · ') || 'Empty';
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
     Every label folds. Projects' label used to be a door to the full list
     with the fold moved onto its chevron, which made it the one head that
     went somewhere when the others folded; the doors now sit behind the
     head's ⋯, and one click means one thing across the column. -->
{#snippet groupHead(id: string, label: string, rows: number, action?: import('svelte').Snippet)}
	<div class="group-head" class:folded={folded(id)} style={foldStyle(rows)}>
		<button
			type="button"
			class="group-label group-toggle"
			aria-expanded={!folded(id)}
			title={folded(id) ? `Show ${label}` : `Hide ${label}`}
			onclick={(e) => toggleGroup(id, e.currentTarget.parentElement as HTMLElement)}
		>
			<span>{label}</span>
			<svg class="chev" width="9" height="6" viewBox="0 0 10 6" fill="none" aria-hidden="true">
				<path d="M1 1l4 4 4-4" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" />
			</svg>
		</button>
		{#if action}
			<span class="group-actions">{@render action()}</span>
		{/if}
	</div>
{/snippet}

{#snippet projectsHeadActions()}
	<button type="button" class="row-action" aria-label="New project" title="New project" onclick={newProject}>
		<svg width="14" height="14" viewBox="0 0 16 16" fill="none" aria-hidden="true">
			<path d="M8 3.5v9M3.5 8h9" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
		</svg>
	</button>
	<button
		type="button"
		class="row-action"
		aria-label="More"
		title="More"
		onclick={(e) => showMenuFromButton(e, projectsHeadMenu())}
	>
		<Icon icon="ri:more-line" width="14" />
	</button>
{/snippet}

{#snippet chatRow(session: ChatSession)}
	{@const url = chatRoute(session)}
	{@const pinned = isPinned(url)}
	{@const home = session.project_id ? projectStore.byId(session.project_id) : undefined}
	{@const tint = home ? projectColor(home) : null}
	<div
		class="panel-row panel-row-has-actions"
		class:active={activeRoute === url}
		role="link"
		tabindex="0"
		title={home ? `${titleOf(session)} · ${home.name}` : titleOf(session)}
		draggable="true"
		ondragstart={(e) => startRefDrag(e, url, titleOf(session))}
		onclick={() => openChat(session)}
		onkeydown={(e) => {
			if (e.key === 'Enter' || e.key === ' ') {
				e.preventDefault();
				openChat(session);
			}
		}}
		oncontextmenu={(e) => showMenu(e, chatMenu(session))}
	>
		<!-- A chat in a project wears the project's color on its bubble, so the
		     filed ones read at a glance in Today and Recent. The bubble stays a
		     bubble: the row is still a chat, the color says whose. -->
		<span class="row-glyph" aria-hidden="true" style={tint ? `color: ${tint}` : undefined}>
			{#if chatActivity.running(session.conversation_id)}
				<Icon icon="ri:loader-4-line" width="14" class="spin" />
			{:else}
				<AtlasIcon name="chats" size={15} bare />
			{/if}
		</span>
		<span class="panel-row-text">{titleOf(session)}</span>
		{@render unreadDot(session.conversation_id)}
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

<!-- A reply landed while the chat was off screen. It sits where the row's
     controls appear, and gives way to them on hover. -->
{#snippet unreadDot(chatId: string | null)}
	{#if chatId && chatActivity.unread(chatId)}
		<span class="row-unread" role="img" aria-label="New reply"></span>
	{/if}
{/snippet}

{#snippet pageRow(page: PageSummary)}
	{@const url = pageRoute(page)}
	{@const pinned = isPinned(url)}
	<div
		class="panel-row panel-row-has-actions"
		class:active={activeRoute === url}
		role="link"
		tabindex="0"
		title={pageTitle(page)}
		draggable="true"
		ondragstart={(e) => startRefDrag(e, url, pageTitle(page))}
		onclick={() => openPage(page)}
		onkeydown={(e) => {
			if (e.key === 'Enter' || e.key === ' ') {
				e.preventDefault();
				openPage(page);
			}
		}}
		oncontextmenu={(e) => showMenu(e, pageMenu(page))}
	>
		<span class="row-glyph" aria-hidden="true">
			{#if page.icon && isEmoji(page.icon)}
				<span class="row-emoji">{page.icon}</span>
			{:else}
				<AtlasIcon name="pages" size={15} bare />
			{/if}
		</span>
		<span class="panel-row-text">{pageTitle(page)}</span>
		<span class="row-actions">
			<button
				type="button"
				class="row-action"
				class:on={pinned}
				aria-label={pinned ? 'Unpin' : 'Pin'}
				title={pinned ? 'Unpin' : 'Pin'}
				onclick={(e) => quickPin(e, { url, label: pageTitle(page), icon: page.icon })}
			>
				<Icon icon={pinned ? 'ri:pushpin-fill' : 'ri:pushpin-line'} width="14" />
			</button>
			<button
				type="button"
				class="row-action"
				aria-label="More"
				title="More"
				onclick={(e) => showMenuFromButton(e, pageMenu(page))}
			>
				<Icon icon="ri:more-line" width="14" />
			</button>
		</span>
	</div>
{/snippet}

{#if pins.length > 0}
	{@render groupHead('pinned', 'Pinned', pins.length)}
	<div class="sidebar-expandable fold" class:expanded={!folded('pinned')} style={foldStyle(pins.length)}>
		<div class="sidebar-expandable-inner">
			{#each pins as pin (pin.id)}
				{@const pinChat = pin.url.startsWith('/chat/') ? pin.url.slice('/chat/'.length) : null}
				{@const look = pinIdentity(pin)}
				<div
					class="panel-row panel-row-has-actions spine"
					class:active={activeRoute === pin.url}
					role="link"
					tabindex="0"
					title={look.title}
					draggable={projectMemberUrl(pin.url) ? 'true' : 'false'}
					ondragstart={(e) => startRefDrag(e, pin.url, look.title)}
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
						{#if pinChat && chatActivity.running(pinChat)}
							<Icon icon="ri:loader-4-line" width="14" class="spin" />
						{:else if look.icon && isEmoji(look.icon)}
							<span class="row-emoji">{look.icon}</span>
						{:else if look.icon}
							<Icon icon={look.icon} width="14" style="color: {clothFor({ url: pin.url, color: look.color })}" />
						{:else}
							<i class="row-dot" style="background: {clothFor({ url: pin.url, color: look.color })}"></i>
						{/if}
					</span>
					<span class="panel-row-text">{look.title}</span>
					{@render unreadDot(pinChat)}
					<span class="row-actions">
						<button
							type="button"
							class="row-action"
							aria-label="Unpin"
							title="Unpin"
							onclick={(e) => quickPin(e, { url: pin.url, label: look.title })}
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
	{@const projectRows = visibleProjects.length + (projects.length > PROJECTS_SHOWN ? 1 : 0)}
	{@render groupHead('projects', 'Projects', projectRows, projectsHeadActions)}
	<div class="sidebar-expandable fold" class:expanded={!folded('projects')} style={foldStyle(projectRows)}>
		<div class="sidebar-expandable-inner">
			{#each visibleProjects as project (project.id)}
				{@const url = projectRoute(project)}
				{@const pinned = isPinned(url)}
				<div
					class="panel-row panel-row-has-actions spine"
					class:active={activeRoute === url}
					class:carded={card?.project.id === project.id}
					class:drop-target={dndManager.projectTarget === project.id || rowDropTarget === project.id}
					{...{ [PROJECT_DROP_ATTR]: project.id }}
					role="link"
					tabindex="0"
					title={project.name}
					ondragover={(e) => {
						if (!isRefDrag(e)) return;
						e.preventDefault();
						if (e.dataTransfer) e.dataTransfer.dropEffect = 'link';
						rowDropTarget = project.id;
					}}
					ondragleave={(e) => {
						const row = e.currentTarget as HTMLElement;
						if (!e.relatedTarget || !row.contains(e.relatedTarget as Node)) {
							if (rowDropTarget === project.id) rowDropTarget = null;
						}
					}}
					ondrop={(e) => {
						rowDropTarget = null;
						const dropped = droppedRefUrl(e);
						if (!dropped) return;
						e.preventDefault();
						void fileIntoProject(project, dropped);
					}}
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
						<ProjectGlyph {project} size={15} />
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
{:else if projectStore.loaded}
	<!-- No projects: the head alone, with its + in view. No "No projects
	     yet" line under it, and nothing to fold, so the label is the door
	     to the projects page, the house shape for a door (the word is the
	     list, the + is the new one). Without it nothing in the sidebar led
	     to a first project. Once one exists this is the group above. -->
	<div class="group-head">
		<button type="button" class="group-label" title="All projects" onclick={openProjects}>
			<span>Projects</span>
		</button>
		<span class="group-actions shown">
			<button type="button" class="row-action" aria-label="New project" title="New project" onclick={newProject}>
				<svg width="14" height="14" viewBox="0 0 16 16" fill="none" aria-hidden="true">
					<path d="M8 3.5v9M3.5 8h9" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
				</svg>
			</button>
		</span>
	</div>
{/if}

{#each recentGroups as group (group.id)}
	{@render groupHead(group.id, group.label, group.items.length)}
	<div class="sidebar-expandable fold" class:expanded={!folded(group.id)} style={foldStyle(group.items.length)}>
		<div class="sidebar-expandable-inner">
			{#each group.items as row (row.key)}
				{#if row.kind === 'chat'}
					{@render chatRow(row.session)}
				{:else}
					{@render pageRow(row.page)}
				{/if}
			{/each}
		</div>
	</div>
{/each}

<div bind:this={spacerEl} class="fold-spacer" style="height: {foldSpacer}px" aria-hidden="true"></div>

{#if card && cardProject}
	{@const url = projectRoute(cardProject)}
	{@const pinned = isPinned(url)}
	<HoverCard anchor={card.anchor} onenter={holdCard} onleave={disarmCard}>
		<div class="card-head">
			<span class="card-glyph" aria-hidden="true">
				<ProjectGlyph project={cardProject} size={16} />
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
		{#if cardChats.length > 0 || cardItems.length > 0}
			<div class="card-rule" aria-hidden="true"></div>
			{#each cardChats as chat (chat.id)}
				<button
					type="button"
					class="card-row card-item"
					onclick={() => openFromCard(`/chat/${chat.id}`, chat.title || 'Chat')}
				>
					<span class="card-item-glyph" style={`color: ${projectColor(cardProject)}`}>
						<AtlasIcon name="chats" size={14} bare />
					</span>
					<span class="card-item-text">{chat.title || untitled('chat')}</span>
				</button>
			{/each}
			{#each cardItems as item (item.url)}
				<button
					type="button"
					class="card-row card-item"
					onclick={() => openFromCard(item.url, memberName(item))}
				>
					<span class="card-item-glyph"><Icon icon={memberIcon(item.url)} width="14" /></span>
					<span class="card-item-text">{memberName(item)}</span>
				</button>
			{/each}
		{/if}
		<div class="card-rule" aria-hidden="true"></div>
		<button type="button" class="card-row" onclick={() => openProject(cardProject)}>
			<Icon icon={PROJECT_ICON} width="15" />
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

	.group-label:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: -2px;
	}

	/* ONE MOTION for the fold: chevron and body share a duration (--fold-ms,
	   set per group) and a curve, so they start and stop on the same frame.
	   Closing runs at 0.85x — what leaves should get out of the way.
	   The curve eases in slightly and decelerates without a long tail. The
	   old --ease-premium over 150ms put 88% of the travel in the first 75ms,
	   so the move strobed in a few big jumps and then crept for another 75ms. */
	.group-head,
	.fold {
		--fold-open: var(--fold-ms, 200ms);
		--fold-close: calc(var(--fold-ms, 200ms) * 0.85);
		--fold-ease: cubic-bezier(0.3, 0.1, 0.2, 1);
	}

	.chev {
		opacity: 0;
		transform: rotate(0deg);
		transition:
			opacity 150ms ease,
			transform var(--fold-open) var(--fold-ease);
	}

	.group-head.folded .chev {
		transition:
			opacity 150ms ease,
			transform var(--fold-close) var(--fold-ease);
	}

	.group-head:hover .chev,
	.group-toggle:focus-visible .chev {
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
	.group-actions:focus-within,
	.group-actions.shown {
		opacity: 1;
	}

	/* A fold, not a blind. The shared .sidebar-expandable wipes a clipping edge
	   over rows that stand still, slicing text mid-glyph every frame. Here the
	   rows are pinned to the bottom of the clip (flex-end overflows upward), so
	   they travel with the edge and tuck away under their head while fading.
	   A transition takes the rule of the state it is heading to: .expanded
	   carries the opening timings, the base rule the closing ones. */
	.fold {
		transition: grid-template-rows var(--fold-close) var(--fold-ease);
	}

	.fold.expanded {
		transition-duration: var(--fold-open);
	}

	.fold > :global(.sidebar-expandable-inner) {
		justify-content: flex-end;
		opacity: 0;
		/* Out over the first 40%, so nothing is legible by the time it is cut. */
		transition: opacity calc(var(--fold-close) * 0.4) ease-out;
	}

	.fold.expanded > :global(.sidebar-expandable-inner) {
		opacity: 1;
		/* In a beat late, once there is room for the rows to be read. */
		transition: opacity calc(var(--fold-open) * 0.6) ease-in calc(var(--fold-open) * 0.15);
	}

	/* Rows are flex items in a box the fold squeezes: without this they shrank
	   toward their text height mid-fold, so each row visibly compressed on top
	   of the move. A fold should move rows, never resize them. */
	.fold > :global(.sidebar-expandable-inner) > :global(*) {
		flex-shrink: 0;
	}

	@media (prefers-reduced-motion: reduce) {
		.chev,
		.group-head.folded .chev,
		.fold,
		.fold > :global(.sidebar-expandable-inner),
		.fold.expanded > :global(.sidebar-expandable-inner) {
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

	/* A project row with something held over it: the row says it will take
	   it, in the same ring the focus state draws, so a drop is never a
	   guess about which row is under the pointer. */
	.panel-row.drop-target {
		background: var(--sidebar-active-bg);
		outline: 2px solid var(--color-primary);
		outline-offset: -2px;
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
	   The chrome cut sits beside an icon, where the serif's own metrics
	   would leave the letters off-center against it. */
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
		/* A hairline, not a shadow, and in the panel's own ink rather than a
		   black the theme never declared: on a dark theme the old rgba ring
		   was invisible against the ground it was meant to separate from. */
		border: 1px solid color-mix(in srgb, var(--color-foreground) 12%, transparent);
		box-sizing: border-box;
	}

	.row-unread {
		width: 7px;
		height: 7px;
		margin-right: 2px;
		flex: none;
		border-radius: 999px;
		background: var(--color-success);
	}

	.panel-row-has-actions:hover .row-unread,
	.panel-row-has-actions:focus-within .row-unread,
	.panel-row-has-actions.carded .row-unread {
		display: none;
	}

	/* The controls appear where the pointer is and nowhere else. Kept in the
	   flow (not absolutely positioned) so the label's ellipsis makes room for
	   them; a label under a floating button is a label you cannot read. */
	.row-actions {
		display: none;
		align-items: center;
		gap: 4px;
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
		border-radius: 6px;
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
		padding: 6px 6px 4px 8px;
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

/* The one rule the card allows: it divides what the project IS from what
	   you can DO, which is the same seam a menu draws with a divider. */
	.card-rule {
		height: 1px;
		margin: 4px 0;
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

	/* The project's contents: quieter than the verbs below them, because
	   they are places to go, not things to do. */
	.card-item {
		color: var(--color-foreground-muted);
	}
	.card-item:hover {
		color: var(--color-foreground);
	}
	.card-item-glyph {
		display: flex;
		flex: none;
		width: 16px;
		justify-content: center;
		color: var(--color-foreground-subtle);
	}
	.card-item-text {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
</style>
