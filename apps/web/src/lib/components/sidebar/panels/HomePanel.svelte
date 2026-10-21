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
	 *   doors     New chat, Pages, Tasks, Projects, Applets, Wiki. The verb,
	 *             then the places. Each place is one row that opens its
	 *             overview; none of them lists its contents here. Search is
	 *             not a door: it is the ⌘K palette, reached from anywhere.
	 *             Pages, Tasks and Projects carry a `+` on hover: the word is
	 *             the list, the plus is the new one.
	 *   Pinned    what the user chose to keep, in their own order: a chat, a
	 *             page, a project, an applet, a task, anything with a url.
	 *             Everything you want one click away lives here, so the doors
	 *             above never grow lists of their own. A pinned project takes
	 *             a dropped row or tab, which files it there.
	 *   Recent    the chats AND the pages, blended by when they last moved,
	 *             newest first, ten of them: a glance, not an archive. When
	 *             there are more, "See all" ends the list and opens All
	 *             chats. One group: splitting off "Today" cost a heading and
	 *             a fold for a calendar fact nobody navigated by.
	 *             The head's filter narrows it to chats or pages, and the
	 *             label says which.
	 *
	 * Every group folds, and its fold is remembered (`sidebarZones`): a folded
	 * group is a statement about how you want the column to look, not a place
	 * you went. A group with nothing in it is not drawn at all: an empty
	 * "Pinned" is a feature announcing itself.
	 *
	 * Rows reveal their controls on hover — pin, more — rather than carrying
	 * them always: a column of twenty rows with forty buttons is a toolbar,
	 * not a list.
	 *
	 * The row menus offer a pin for a chat, a project, an applet or a page, and
	 * a tab's menu pins whatever the tab shows. A project cannot hold a
	 * project. The server refuses it and the menus never offer it.
	 */
	import { chatSessions, type ChatSession } from '$lib/stores/chatSessions.svelte';
	import { projectStore } from '$lib/stores/project.svelte';
	import { pagesStore } from '$lib/stores/pages.svelte';
	import { pinsStore } from '$lib/stores/pins.svelte';
	import { chatActivity } from '$lib/stores/chatActivity.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { sidebarZones } from '$lib/stores/sidebarZones.svelte';
	import { contextMenu, type ContextMenuItem } from '$lib/stores/contextMenu.svelte';
	import { promptText } from '$lib/stores/dialog.svelte';
	import { deleteChat, type PageSummary, type Pin } from '$lib/api/client';
	import { pinIdentity, renameRef, ownedRef, untitled } from '$lib/refs/identity.svelte';
	import { pinMenuItem, pinIconMenuItem, isPinned, togglePin } from '$lib/pins/pinAction';
	import { getProjectMenuItems } from '$lib/utils/contextMenuItems';
	import { notifyTrashed, routeIfOpen } from '$lib/utils/toasts';
	import { toast } from 'svelte-sonner';
	import { clothFor } from '$lib/sidebar/pin-colors';
	import { isEmoji } from '$lib/utils/iconHelpers';
	import {
		droppedRefUrl,
		fileIntoProject,
		isRefDrag,
		newProject,
		openProjects,
		projectRowMenuItems,
		projectMemberUrl,
		startRefDrag,
	} from '$lib/utils/projectActions';
	import { dndManager, PROJECT_DROP_ATTR } from '$lib/stores/dndManager.svelte';
	import Icon from '$lib/components/Icon.svelte';
	import AtlasIcon from '../AtlasIcon.svelte';
	import SetupRing from '../SetupRing.svelte';
	import { gettingStarted } from '$lib/stores/gettingStarted.svelte';
	import { setup } from '$lib/components/setup/setup.svelte';
	import { SHELL_GLYPH_PX } from '$lib/components/sidebar/AtlasIcon.svelte';
	import { openWebLink } from '$lib/utils/webLinks';

	// Skipped is not done: a step set aside keeps the door, because Finish
	// later and Skip both mean "not now", never "not ever". It goes once every
	// step is done.
	const setupOpen = $derived(gettingStarted.loaded && !gettingStarted.unsupported && !setup.complete);
	const setupCount = $derived(`${setup.doneCount}/${setup.steps.length}`);
	// Setup's own tab, at the step to pick up: the tab carries the steps
	// across its top, so the sidebar does not list them.
	function openSetup() {
		const next = setup.resumeInApp;
		windowShellStore.openTabFromRoute(next ? `/setup/${next}` : '/setup', { label: 'Setup', focusExisting: true });
	}
	const RECENTS_CAP = 10;

	// ── what is where ──────────────────────────────────────────────────────

	const pins = $derived(pinsStore.pins);
	const projects = $derived(projectStore.projects);
	// The pinned project a dragged sidebar row is over. A dragged TAB is
	// tracked by dndManager instead (it is not an HTML drag); either lights
	// the row.
	let rowDropTarget = $state<string | null>(null);
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
			// an old chat must not move it to the top.
			ts: stamp(session.last_message_at || session.last_updated),
			session,
		}));
		const pages: RecentRow[] = pagesStore.pages.map((page) => ({
			kind: 'page',
			key: `page:${page.id}`,
			ts: stamp(page.updated_at),
			page,
		}));
		return [...chats, ...pages].sort((a, b) => b.ts - a.ts);
	});
	// What Recent shows: both kinds, or one. Remembered per browser, like a
	// fold, because it says how you want the column to look.
	type RecentFilter = 'all' | 'chat' | 'page';
	const RECENT_FILTER_KEY = 'virtues-sidebar-recent-filter';
	let recentFilter = $state<RecentFilter>(readRecentFilter());

	function readRecentFilter(): RecentFilter {
		try {
			const v = localStorage.getItem(RECENT_FILTER_KEY);
			return v === 'chat' || v === 'page' ? v : 'all';
		} catch {
			return 'all';
		}
	}

	function setRecentFilter(next: RecentFilter) {
		recentFilter = next;
		try {
			localStorage.setItem(RECENT_FILTER_KEY, next);
		} catch {
			// Storage can be blocked; the filter still applies until reload.
		}
	}

	const recentsFiltered = $derived(
		recentFilter === 'all' ? recents : recents.filter((r) => r.kind === recentFilter),
	);
	const recentsShown = $derived(recentsFiltered.slice(0, RECENTS_CAP));
	const recentsMore = $derived(recentsFiltered.length > RECENTS_CAP);
	const recentLabel = $derived(
		recentFilter === 'chat' ? 'Recent chats' : recentFilter === 'page' ? 'Recent pages' : 'Recent',
	);

	function recentFilterMenu(): ContextMenuItem[] {
		const option = (id: RecentFilter, label: string): ContextMenuItem => ({
			id: `recent-${id}`,
			label,
			role: 'menuitemradio',
			checked: recentFilter === id,
			action: () => setRecentFilter(id),
		});
		return [option('all', 'Chats and pages'), option('chat', 'Chats'), option('page', 'Pages')];
	}

	// "See all" goes to the full list of what Recent is showing.
	function openAllRecents() {
		if (recentFilter === 'page') openPages();
		else windowShellStore.openTabFromRoute('/chat-history', { label: 'All chats', focusExisting: true });
	}

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
		windowShellStore.newChat();
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

	function openTasks() {
		windowShellStore.openTabFromRoute('/tasks', { label: 'Tasks', focusExisting: true });
	}

	async function newTask(e: MouseEvent) {
		e.preventDefault();
		e.stopPropagation();
		const { tasksStore } = await import('$lib/stores/tasks.svelte');
		tasksStore.requestAdd();
		openTasks();
	}

	async function addProject(e: MouseEvent) {
		e.preventDefault();
		e.stopPropagation();
		await newProject();
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

	function openPin(pin: Pin) {
		if (isExternal(pin.url)) {
			void openWebLink(pin.url);
			return;
		}
		windowShellStore.openTabFromRoute(pin.url, { label: pinLabel(pin), focusExisting: true });
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
		contextMenu.show({ x: e.clientX, y: e.clientY }, items);
	}

	function showMenuFromButton(e: MouseEvent, items: ContextMenuItem[]) {
		e.preventDefault();
		e.stopPropagation();
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
</script>

<!-- The doors sit in the flow above the groups, told apart from them by air,
     not a rule. They wear Atlas like every nav door in the shell. Search
     has no door here: it is the ⌘K palette, which has the room for results
     across chats, pages and projects. -->
<div class="panel-doors">
	{#if setupOpen}
		<button type="button" class="panel-row panel-door" onclick={openSetup}>
			<SetupRing size={16} />
			<span class="panel-row-text">Setup</span>
			<span class="door-meta">{setupCount}</span>
		</button>
	{/if}
	<button type="button" class="panel-row panel-door" onclick={newChat}>
		<AtlasIcon name="new-chat" size={SHELL_GLYPH_PX} bare />
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
		<AtlasIcon name="pages" size={SHELL_GLYPH_PX} bare />
		<span class="panel-row-text">Pages</span>
		<span class="row-actions">
			<button type="button" class="row-action" aria-label="New page" title="New page" onclick={newPage}>
				<AtlasIcon name="plus" size={14} bare />
			</button>
		</span>
	</div>
	<!-- No count beside the word, ever: the list never nags from the
	     sidebar (agents/plan/tasks-plan.md). -->
	<div
		class="panel-row panel-door panel-row-has-actions"
		role="link"
		tabindex="0"
		onclick={openTasks}
		onkeydown={(e) => {
			if (e.key === 'Enter' || e.key === ' ') {
				e.preventDefault();
				openTasks();
			}
		}}
	>
		<AtlasIcon name="tasks" size={SHELL_GLYPH_PX} bare />
		<span class="panel-row-text">Tasks</span>
		<span class="row-actions">
			<button type="button" class="row-action" aria-label="New task" title="New task" onclick={newTask}>
				<AtlasIcon name="plus" size={14} bare />
			</button>
		</span>
	</div>
	<div
		class="panel-row panel-door panel-row-has-actions"
		role="link"
		tabindex="0"
		onclick={openProjects}
		onkeydown={(e) => {
			if (e.key === 'Enter' || e.key === ' ') {
				e.preventDefault();
				openProjects();
			}
		}}
	>
		<AtlasIcon name="projects" size={SHELL_GLYPH_PX} bare />
		<span class="panel-row-text">Projects</span>
		<span class="row-actions">
			<button type="button" class="row-action" aria-label="New project" title="New project" onclick={addProject}>
				<AtlasIcon name="plus" size={14} bare />
			</button>
		</span>
	</div>
	<button type="button" class="panel-row panel-door" onclick={openApplets}>
		<AtlasIcon name="applets" size={SHELL_GLYPH_PX} bare />
		<span class="panel-row-text">Applets</span>
	</button>
	<!-- The record, a door like the tools above it. It opens the Wiki's
	     overview, whose index reaches its sections; the sidebar stays Home. -->
	<button
		type="button"
		class="panel-row panel-door"
		onclick={() => windowShellStore.openTabFromRoute('/wiki', { label: 'Wiki', focusExisting: true })}
	>
		<AtlasIcon name="wiki" size={SHELL_GLYPH_PX} bare />
		<span class="panel-row-text">Wiki</span>
	</button>
</div>

<!-- A group's head: the label folds the group; the chevron says so on
     approach and stays while the group is shut, because a folded group has
     to be able to say so — absence reads as missing data, not a decision.
     Every label folds. Projects' label used to be a door to the full list
     with the fold moved onto its chevron, which made it the one head that
     went somewhere when the others folded; the doors now sit behind the
     head's ⋯, and one click means one thing across the column. -->
{#snippet groupHead(id: string, label: string, rows: number, action?: import('svelte').Snippet, actionShown = false)}
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
			<span class="group-actions" class:shown={actionShown}>{@render action()}</span>
		{/if}
	</div>
{/snippet}

<!-- The filter stays in view while it is narrowing the list, so a short
     Recent never looks like missing data. -->
{#snippet recentHeadActions()}
	<button
		type="button"
		class="row-action"
		class:on={recentFilter !== 'all'}
		aria-label="Filter recents"
		title="Filter recents"
		onclick={(e) => showMenuFromButton(e, recentFilterMenu())}
	>
		<AtlasIcon name="filter" size={14} bare />
	</button>
{/snippet}

{#snippet chatRow(session: ChatSession)}
	{@const url = chatRoute(session)}
	{@const pinned = isPinned(url)}
	{@const home = session.project_id ? projectStore.byId(session.project_id) : undefined}
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
		<!-- No glyph: Recent is a list of titles, and they start under the
		     group's label. Ten repeated marks would make it a second block of
		     doors. -->
		<span class="panel-row-text">{titleOf(session)}</span>
		{#if chatActivity.running(session.conversation_id)}
			<span class="row-running" role="img" aria-label="Replying">
				<Icon icon="ri:loader-4-line" width="14" class="spin" />
			</span>
		{:else}
			{@render unreadDot(session.conversation_id)}
		{/if}
		<span class="row-actions">
			<button
				type="button"
				class="row-action"
				class:on={pinned}
				aria-label={pinned ? 'Unpin' : 'Pin'}
				title={pinned ? 'Unpin' : 'Pin'}
				onclick={(e) => quickPin(e, { url, label: titleOf(session), icon: session.icon })}
			>
				<AtlasIcon name={pinned ? 'pinned' : 'pin'} size={14} bare />
			</button>
			<button
				type="button"
				class="row-action"
				aria-label="More"
				title="More"
				onclick={(e) => showMenuFromButton(e, chatMenu(session))}
			>
				<AtlasIcon name="more" size={14} bare />
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
				<AtlasIcon name={pinned ? 'pinned' : 'pin'} size={14} bare />
			</button>
			<button
				type="button"
				class="row-action"
				aria-label="More"
				title="More"
				onclick={(e) => showMenuFromButton(e, pageMenu(page))}
			>
				<AtlasIcon name="more" size={14} bare />
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
				{@const owned = ownedRef(pin.url)}
				{@const pinProject = owned?.kind === 'project' ? owned.id : null}
				<div
					class="panel-row panel-row-has-actions"
					class:active={activeRoute === pin.url}
					class:drop-target={pinProject !== null &&
						(dndManager.projectTarget === pinProject || rowDropTarget === pinProject)}
					{...pinProject ? { [PROJECT_DROP_ATTR]: pinProject } : {}}
					role="link"
					tabindex="0"
					title={look.title}
					draggable={projectMemberUrl(pin.url) ? 'true' : 'false'}
					ondragstart={(e) => startRefDrag(e, pin.url, look.title)}
					ondragover={(e) => {
						if (!pinProject || !isRefDrag(e)) return;
						e.preventDefault();
						if (e.dataTransfer) e.dataTransfer.dropEffect = 'link';
						rowDropTarget = pinProject;
					}}
					ondragleave={(e) => {
						const row = e.currentTarget as HTMLElement;
						if (!e.relatedTarget || !row.contains(e.relatedTarget as Node)) {
							if (rowDropTarget === pinProject) rowDropTarget = null;
						}
					}}
					ondrop={(e) => {
						rowDropTarget = null;
						const dropped = droppedRefUrl(e);
						if (!pinProject || !dropped) return;
						e.preventDefault();
						void fileIntoProject(
							projects.find((x) => x.id === pinProject) ?? { id: pinProject, name: look.title, archived_at: null },
							dropped,
						);
					}}
					onclick={() => openPin(pin)}
					onkeydown={(e) => {
						if (e.key === 'Enter' || e.key === ' ') {
							e.preventDefault();
							openPin(pin);
						}
					}}
					oncontextmenu={(e) => showMenu(e, pinMenu(pin, { x: e.clientX, y: e.clientY }))}
				>
					<!-- The mark the user picked, else a solid circle in the pin's
					     cloth. A filled dot shows its color at 14px where a line
					     glyph shows only a trace of it, so the pinned list reads by
					     color at a glance. -->
					<span class="row-glyph" aria-hidden="true">
						{#if pinChat && chatActivity.running(pinChat)}
							<Icon icon="ri:loader-4-line" width="14" class="spin" />
						{:else if look.icon && isEmoji(look.icon)}
							<span class="row-emoji">{look.icon}</span>
						{:else if look.icon}
							<!-- Drawn at the nav rows' size. An Atlas pick goes through
							     AtlasIcon so it keeps the same 1.25px line as the glyphs
							     above it; through iconify at a smaller box it thinned and
							     shrank. -->
							<span class="row-kind" style="color: {clothFor({ url: pin.url, color: look.color })}">
								{#if look.icon.startsWith('atlas:')}
									<AtlasIcon name={look.icon.slice('atlas:'.length)} size={SHELL_GLYPH_PX} bare />
								{:else}
									<Icon icon={look.icon} width={SHELL_GLYPH_PX} />
								{/if}
							</span>
						{:else}
							<Icon icon="shape:circle" width="14" style="color: {clothFor({ url: pin.url, color: look.color })}" />
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
							<AtlasIcon name="pinned" size={14} bare />
						</button>
						<button
							type="button"
							class="row-action"
							aria-label="More"
							title="More"
							onclick={(e) =>
								showMenuFromButton(e, pinMenu(pin, { x: e.clientX, y: e.clientY }))}
						>
							<AtlasIcon name="more" size={14} bare />
						</button>
					</span>
				</div>
			{/each}
		</div>
	</div>
{/if}

{#if recents.length > 0}
	{@render groupHead('recent', recentLabel, Math.max(1, recentsShown.length), recentHeadActions, recentFilter !== 'all')}
	<div class="sidebar-expandable fold" class:expanded={!folded('recent')} style={foldStyle(recentsShown.length)}>
		<div class="sidebar-expandable-inner">
			{#if recentsShown.length === 0}
				<div class="panel-row panel-empty">{recentFilter === 'page' ? 'No pages yet' : 'No chats yet'}</div>
			{/if}
			{#each recentsShown as row (row.key)}
				{#if row.kind === 'chat'}
					{@render chatRow(row.session)}
				{:else}
					{@render pageRow(row.page)}
				{/if}
			{/each}
		</div>
	</div>
{/if}

<!-- Hidden while Recent is folded: "See all" under a closed heading would
     point at nothing on screen. -->
{#if recentsMore && !folded('recent')}
	<button type="button" class="panel-row panel-more" onclick={openAllRecents}>See all</button>
{/if}

<div bind:this={spacerEl} class="fold-spacer" style="height: {foldSpacer}px" aria-hidden="true"></div>

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
		transition: color var(--motion-hover-out) var(--ease-premium);
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

	/* A door's glyph is muted and its word is ink, like the header's controls
	   above it; pointing at the door brings the glyph up to ink. */
	.panel-door > :global(.atlas-icon) {
		color: var(--color-foreground-muted);
		transition: color var(--motion-hover-out) var(--ease-premium);
	}

	.panel-door:hover > :global(.atlas-icon) {
		color: var(--color-foreground);
		transition-duration: var(--motion-hover-in);
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

	.panel-row:hover {
		background: var(--hover-bg);
		transition-duration: var(--motion-hover-in);
	}

	.panel-row.active {
		background: var(--active-bg);
	}

	/* A pinned project with something held over it: the row says it will take
	   it, in the same ring the focus state draws, so a drop is never a
	   guess about which row is under the pointer. */
	.panel-row.drop-target {
		background: var(--active-bg);
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

	.row-glyph {
		width: 16px;
		height: 16px;
		flex: none;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.row-emoji {
		font-size: 14px;
		line-height: 1;
	}

	.row-kind {
		display: flex;
	}


	.row-running {
		display: flex;
		flex: none;
		color: var(--color-foreground-muted);
	}

	.row-unread {
		width: 7px;
		height: 7px;
		margin-right: 2px;
		flex: none;
		border-radius: 999px;
		background: var(--color-success);
	}

	.panel-row-has-actions:hover :is(.row-unread, .row-running),
	.panel-row-has-actions:focus-within :is(.row-unread, .row-running) {
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
	.panel-row-has-actions:focus-within .row-actions {
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

	.panel-empty {
		cursor: default;
		color: var(--color-foreground-subtle);
	}

	.panel-empty:hover {
		background: none;
	}

	.door-meta {
		margin-left: auto;
		font-size: 12px;
		color: var(--color-foreground-subtle);
		font-variant-numeric: tabular-nums;
	}
</style>
