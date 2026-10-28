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
	 *             a dropped row or tab, which files it there. A pin moves by
	 *             drag, by Alt and an arrow, or by Move up and Move down.
	 *   Recent    chats, pages, projects and applets, blended by when
	 *             they last changed or you last opened them, newest first,
	 *             ten of them, with pinned things left out (sidebar/recents).
	 *             When there are more, "See all" ends the list. One group:
	 *             splitting off "Today" cost a heading and a fold for a
	 *             calendar fact nobody navigated by. The head's filter
	 *             narrows it to one kind, and the label says which.
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
	import { onMount, tick, untrack } from 'svelte';
	import { crossfade, slide } from 'svelte/transition';
	import { flip } from 'svelte/animate';
	import { prefersReducedMotion } from 'svelte/motion';
	import { easePremium, motionMs } from '$lib/utils/motion';
	import { chatSessions, type ChatSession } from '$lib/stores/chatSessions.svelte';
	import { projectStore } from '$lib/stores/project.svelte';
	import { pagesStore } from '$lib/stores/pages.svelte';
	import { pinsStore } from '$lib/stores/pins.svelte';
	import { sidebarDoors, type DoorId } from '$lib/stores/sidebarDoors.svelte';
	import { chatActivity } from '$lib/stores/chatActivity.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { sidebarZones } from '$lib/stores/sidebarZones.svelte';
	import { contextMenu, type ContextMenuItem } from '$lib/stores/contextMenu.svelte';
	import { deleteChat, type PageSummary, type Pin, type ProjectSummary } from '$lib/api/client';
	import { pinIdentity, renameRef, ownedRef, untitled, isUntitled } from '$lib/refs/identity.svelte';
	import {
		loadRecentSources,
		recentRows,
		RECENT_KIND_LABEL,
		type RecentKind,
		type RecentRow,
	} from '$lib/sidebar/recents.svelte';
	import ThinkingMark from '$lib/components/ThinkingMark.svelte';
	import { pinMenuItem, pinIconMenuItem, isPinned, togglePin, canPin, pinAtGap } from '$lib/pins/pinAction';
	import { getProjectMenuItems } from '$lib/utils/contextMenuItems';
	import { newPage as makePage, newTask as makeTask } from '$lib/utils/newActions';
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
		REF_DRAG_TYPE,
		startRefDrag,
		setMentionDrag,
	} from '$lib/utils/projectActions';
	import { dndManager, PIN_DROP_ATTR, PROJECT_DROP_ATTR, pinGapAt } from '$lib/stores/dndManager.svelte';
	import Icon from '$lib/components/Icon.svelte';
	import AtlasIcon from '../AtlasIcon.svelte';
	import SetupRing from '../SetupRing.svelte';
	import { gettingStarted } from '$lib/stores/gettingStarted.svelte';
	import { setup } from '$lib/components/setup/setup.svelte';
	import { SHELL_GLYPH_PX } from '$lib/components/sidebar/AtlasIcon.svelte';
	import { openWebLink } from '$lib/utils/webLinks';
	import { openMenuItems } from '$lib/utils/openMenuItems';

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

	// The tree under the Projects door: every project, in your order. It
	// folds like a group, and opens by itself when you open a project from
	// anywhere else, so the line to it is on screen.
	const openProjectIndex = $derived(projects.findIndex((p) => activeRoute === `/project/${p.id}`));
	const openProjectId = $derived(openProjectIndex >= 0 ? projects[openProjectIndex].id : null);
	$effect(() => {
		if (openProjectId && untrack(() => folded('projects'))) sidebarZones.toggle(zoneId('projects'));
	});

	function toggleProjectTree(e: MouseEvent) {
		e.preventDefault();
		e.stopPropagation();
		const door = (e.currentTarget as HTMLElement).closest('.panel-door') as HTMLElement | null;
		if (door) toggleGroup('projects', door);
	}

	function openProject(p: ProjectSummary) {
		windowShellStore.openTabFromRoute(`/project/${p.id}`, { label: p.name, focusExisting: true });
	}
	// The pinned project a dragged sidebar row is over. A dragged TAB is
	// tracked by dndManager instead (it is not an HTML drag); either lights
	// the row.
	let rowDropTarget = $state<string | null>(null);
	// A pin being dragged within Pinned, and the gap a drag over Pinned would
	// land in: before pin number `pinGap`, or at the end when it equals
	// pins.length. A dragged tab is tracked by dndManager; either draws the line.
	let draggingPinId = $state<string | null>(null);
	let pinGap = $state<number | null>(null);
	const shownGap = $derived(dndManager.pinGap ?? pinGap);
	// Where the thing in hand already sits in Pinned: the pin being dragged,
	// or the pin of a dragged tab's page. Dropping it beside itself changes
	// nothing, so those two gaps draw no line.
	const pinFrom = $derived.by(() => {
		if (draggingPinId !== null) return pins.findIndex((p) => p.id === draggingPinId);
		const route = dndManager.session?.item.tab?.route;
		return route ? pins.findIndex((p) => p.url === route) : -1;
	});
	const pinLine = $derived(
		shownGap !== null && pinFrom >= 0 && (shownGap === pinFrom || shownGap === pinFrom + 1) ? null : shownGap,
	);
	// A sidebar row is in the air, so an empty Pinned shows where to drop it.
	// A drop that pins puts a provisional pin in the list before this clears
	// (pinsStore.insertAt), so Pinned stays open while the box answers.
	let refDragging = $state(false);
	const tabCanPin = $derived(!!dndManager.session?.item.tab?.route && canPin(dndManager.session.item.tab.route));
	const pinnedOpenForDrop = $derived(pins.length === 0 && (refDragging || tabCanPin));
	// A Recent row in the air, dimmed in its place like a pin or a door.
	let draggingRecent = $state<string | null>(null);

	// ── one drag at a time ─────────────────────────────────────────────────
	// The browser takes its picture of a dragged row after dragstart returns.
	// The row wears its lifted look for that moment (liftRow), and the next
	// task takes it off so the dimmed source shows instead. Anything that
	// changes the layout, like opening an empty Pinned, waits for that task
	// too: moving rows under a starting drag can cancel it.
	let dragSource: HTMLElement | null = null;
	let liftTimer: ReturnType<typeof setTimeout> | undefined;
	let refTimer: ReturnType<typeof setTimeout> | undefined;
	// The browser keeps the row's :hover from when the drag began until the
	// pointer next moves, which can be well after the drop. Rows ignore hover
	// until then (`.stale-hover`).
	let staleHover = $state(false);

	function liftRow(e: DragEvent) {
		const row = e.currentTarget as HTMLElement;
		clearTimeout(liftTimer);
		dragSource?.classList.remove('lifting');
		dragSource = row;
		row.classList.add('lifting');
		staleHover = true;
		liftTimer = setTimeout(() => row.classList.remove('lifting'));
	}

	/** Every mark a drag left: the line, the tint, the dimming. */
	function endDrag() {
		clearTimeout(liftTimer);
		clearTimeout(refTimer);
		cancelAnimationFrame(pinLeaveFrame);
		cancelAnimationFrame(doorLeaveFrame);
		if (dragSource) {
			dragSource.classList.remove('lifting');
			// Pressing a row to drag it focused it, and a focused row shows its
			// controls. A pointer's focus is not worth keeping; a keyboard's is.
			if (dragSource.matches(':focus:not(:focus-visible)')) dragSource.blur();
		}
		dragSource = null;
		draggingPinId = null;
		draggingDoorId = null;
		draggingRecent = null;
		pinGap = null;
		doorGap = null;
		rowDropTarget = null;
		refDragging = false;
	}

	$effect(() => {
		const start = (e: DragEvent) => {
			if (!isRefDrag(e)) return;
			dragSource ??= e.target instanceof HTMLElement ? e.target : null;
			clearTimeout(refTimer);
			refTimer = setTimeout(() => (refDragging = true));
		};
		// A source row that unmounted mid-drag gets its dragend, if the browser
		// sends one at all, outside the document. No pointer events fire during
		// a native drag, so the first one after it ends clears what was left.
		const moved = () => {
			if (staleHover) staleHover = false;
			if (dragSource && !dragSource.isConnected) endDrag();
		};
		document.addEventListener('dragstart', start);
		document.addEventListener('dragend', endDrag);
		document.addEventListener('drop', endDrag);
		document.addEventListener('pointermove', moved, { passive: true });
		document.addEventListener('pointerdown', moved, { passive: true });
		return () => {
			document.removeEventListener('dragstart', start);
			document.removeEventListener('dragend', endDrag);
			document.removeEventListener('drop', endDrag);
			document.removeEventListener('pointermove', moved);
			document.removeEventListener('pointerdown', moved);
			endDrag();
		};
	});

	// Leaving a group takes its line away a frame later, unless a dragover
	// inside it comes first. dragleave fires on every child boundary, and
	// WebKit can report its relatedTarget as null even then, so clearing at
	// once made the line blink between rows.
	let pinLeaveFrame = 0;
	let doorLeaveFrame = 0;
	// The page list is loaded by the rooms that show pages; this panel is
	// mounted before any of them, so it asks once. The store dedupes nothing,
	// hence the guard on both the list and the in-flight flag.
	if (!pagesStore.pages.length && !pagesStore.pagesLoading) pagesStore.loadPages();

	// Recent: chats, pages, projects and applets, newest first, with
	// pinned things left out (sidebar/recents).
	loadRecentSources();
	const recentsLive = $derived(recentRows());

	// While the pointer or the keyboard is in the sidebar, Recent keeps the
	// order it had when you arrived: opening a row moves it to the top a few
	// seconds later, and a list that reorders while you click down it moves
	// the row you were reaching for. Something new still joins at the top.
	// Leave the sidebar and the order catches up, with the rows sliding.
	let heldOrder = $state<Map<string, number> | null>(null);
	const recents = $derived.by(() => {
		const held = heldOrder;
		if (!held) return recentsLive;
		return [...recentsLive].sort((a, b) => (held.get(a.url) ?? -1) - (held.get(b.url) ?? -1));
	});

	$effect(() => {
		const column = spacerEl?.closest<HTMLElement>('.sidebar-column');
		if (!column) return;
		const hold = () => {
			if (!heldOrder) heldOrder = new Map(untrack(() => recentsLive).map((r, i) => [r.url, i]));
		};
		const release = () => {
			// focusout fires before focus lands on its next element.
			setTimeout(() => {
				if (!column.matches(':hover') && !column.contains(document.activeElement)) heldOrder = null;
			});
		};
		column.addEventListener('pointerenter', hold);
		column.addEventListener('pointerleave', release);
		column.addEventListener('focusin', hold);
		column.addEventListener('focusout', release);
		return () => {
			column.removeEventListener('pointerenter', hold);
			column.removeEventListener('pointerleave', release);
			column.removeEventListener('focusin', hold);
			column.removeEventListener('focusout', release);
		};
	});

	// What Recent shows: every kind, or one. Remembered per browser, like a
	// fold, because it says how you want the column to look.
	type RecentFilter = 'all' | RecentKind;
	const RECENT_FILTERS: { id: RecentFilter; label: string; plural: string; seeAll: () => void }[] = [
		{ id: 'all', label: 'Everything', plural: 'things', seeAll: () => openAllChats() },
		{ id: 'chat', label: 'Chats', plural: 'chats', seeAll: () => openAllChats() },
		{ id: 'page', label: 'Pages', plural: 'pages', seeAll: () => openPages() },
		{ id: 'project', label: 'Projects', plural: 'projects', seeAll: () => openProjects() },
		{ id: 'applet', label: 'Applets', plural: 'applets', seeAll: () => openApplets() },
	];
	const RECENT_FILTER_KEY = 'virtues-sidebar-recent-filter';
	let recentFilter = $state<RecentFilter>(readRecentFilter());
	const filterDef = $derived(RECENT_FILTERS.find((f) => f.id === recentFilter) ?? RECENT_FILTERS[0]);

	function readRecentFilter(): RecentFilter {
		try {
			const v = localStorage.getItem(RECENT_FILTER_KEY);
			return RECENT_FILTERS.some((f) => f.id === v) ? (v as RecentFilter) : 'all';
		} catch {
			return 'all';
		}
	}

	function setRecentFilter(next: RecentFilter) {
		// A different set of rows, not a change to this one: swap without motion.
		holdMotion();
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
	const recentLabel = $derived(recentFilter === 'all' ? 'Recent' : `Recent ${filterDef.plural}`);

	function recentFilterMenu(): ContextMenuItem[] {
		return RECENT_FILTERS.map((f) => ({
			id: `recent-${f.id}`,
			label: f.label,
			role: 'menuitemradio',
			checked: recentFilter === f.id,
			action: () => setRecentFilter(f.id),
		}));
	}

	function openAllChats() {
		windowShellStore.openTabFromRoute('/chat-history', { label: 'All chats', focusExisting: true });
	}

	// ── motion ─────────────────────────────────────────────────────────────
	// Rows move only when you did something: a new chat or page slides in at
	// the top, and a pinned row travels from Recent to Pinned and back. The
	// lists arriving on load, and a change of filter, swap in place.

	// Rows that move on a change slide on --motion-state and --ease-premium,
	// like everything else in the shell that changes state.
	const MOVE_MS = motionMs('--motion-state', 150);
	let motionOn = $state(false);
	let motionTimer: ReturnType<typeof setTimeout> | undefined;
	// Read on every move, so turning reduced motion on or off applies at once.
	const moving = () => motionOn && !prefersReducedMotion.current;

	/** Hold motion off until the lists settle. */
	function holdMotion(ms = 400) {
		motionOn = false;
		clearTimeout(motionTimer);
		motionTimer = setTimeout(() => (motionOn = true), ms);
	}
	// The stores fill over the first second or so, one after another.
	holdMotion(1500);

	const [sendRow, receiveRow] = crossfade({
		duration: MOVE_MS,
		easing: easePremium,
		// A row with no partner (a new chat, one past Recent's ten) opens its
		// own height instead of crossing the column.
		fallback: (node) => slide(node, { duration: moving() ? MOVE_MS : 0, easing: easePremium }),
	});

	const rowOut = (node: Element, key: string) =>
		moving() ? sendRow(node, { key }) : { duration: 0 };
	const rowIn = (node: Element, key: string) =>
		moving() ? receiveRow(node, { key }) : { duration: 0 };
	const rowFlip = (node: Element, rects: { from: DOMRect; to: DOMRect }) =>
		flip(node, rects, { duration: moving() ? MOVE_MS : 0, easing: easePremium });
	// An empty Pinned opens for a drop and closes after one: it slides, so
	// the rows under it are not thrown down a group's height in one frame.
	const zoneSlide = (node: Element) => slide(node, { duration: moving() ? MOVE_MS : 0, easing: easePremium });

	/** The glyph a pin with no chosen mark wears, by what it opens. */
	function pinKindGlyph(url: string): string {
		if (/^https?:\/\//i.test(url)) return 'wiki';
		if (url.startsWith('/chat/')) return 'chats';
		if (url.startsWith('/project/') || url.startsWith('/notebook/')) return 'projects';
		if (url.startsWith('/applet')) return 'applets';
		if (url.startsWith('/tasks')) return 'tasks';
		if (url.startsWith('/wiki')) return 'wiki';
		if (url.startsWith('/day/')) return 'day';
		return 'pages';
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
		await makePage((route, label) => windowShellStore.openTabFromRoute(route, { label }));
	}

	function openTasks() {
		windowShellStore.openTabFromRoute('/tasks', { label: 'Tasks', focusExisting: true });
	}

	async function newTask(e: MouseEvent) {
		e.preventDefault();
		e.stopPropagation();
		await makeTask(openTasks);
	}

	async function addProject(e: MouseEvent) {
		e.preventDefault();
		e.stopPropagation();
		await newProject();
	}

	function openApplets() {
		windowShellStore.openTabFromRoute('/applets', { label: 'Applets', focusExisting: true });
	}

	function openWiki() {
		windowShellStore.openTabFromRoute('/wiki', { label: 'Wiki', focusExisting: true });
	}

	/**
	 * What each door does. Pages, Tasks and Projects carry a + on hover: the
	 * word is the list, the plus is a new one. Tasks never shows a count: the
	 * list does not nag from the sidebar (agents/plan/tasks-plan.md). The
	 * Wiki opens its overview, whose index reaches its sections.
	 */
	const doorActions: Record<DoorId, { open: () => void; add?: { label: string; key?: string; run: (e: MouseEvent) => void } }> = {
		'new-chat': { open: newChat },
		pages: { open: openPages, add: { label: 'New page', key: 'mod+shift+n', run: newPage } },
		tasks: { open: openTasks, add: { label: 'New task', key: 'mod+shift+e', run: newTask } },
		projects: { open: openProjects, add: { label: 'New project', run: addProject } },
		applets: { open: openApplets },
		wiki: { open: openWiki },
	};

	// ── arranging the doors ────────────────────────────────────────────────

	const doors = $derived(sidebarDoors.visible);
	onMount(() => void sidebarDoors.load());
	// The hand-drawn rule after a group's label. Three strokes, each a little
	// off level in its own way, picked by the group's id so a group always
	// draws the same one and neighbours rarely match.
	const RULE_STROKES = [
		'M0 2.3C14 1.6 27 2.7 41 2.2S71 1.5 85 2.1S96 2.4 100 1.9',
		'M0 1.8C11 2.5 26 1.7 44 2.3S69 2.6 83 1.9S95 1.6 100 2.2',
		'M0 2.1C17 1.5 31 2.4 50 2S78 1.6 91 2.4S98 2.1 100 2',
	];
	function ruleStroke(id: string): string {
		let sum = 0;
		for (const ch of id) sum += ch.charCodeAt(0);
		return RULE_STROKES[sum % RULE_STROKES.length];
	}

	// A door being dragged, and the gap it would land in among the doors.
	let draggingDoorId = $state<DoorId | null>(null);
	let doorGap = $state<number | null>(null);
	// Beside the dragged door itself a drop changes nothing, so no line there.
	const doorLine = $derived.by(() => {
		const from = doors.findIndex((d) => d.id === draggingDoorId);
		return doorGap !== null && from >= 0 && (doorGap === from || doorGap === from + 1) ? null : doorGap;
	});
	// A door carries its id under its own type. In text/plain, a door dropped
	// in the composer or a text field would type its label there.
	const DOOR_DRAG_TYPE = 'application/x-virtues-door';

	function moveDoor(from: number, to: number) {
		sidebarDoors.move(from, to).catch(() => toast.error("Couldn't move that. Try again."));
	}

	function setDoorHidden(id: DoorId, hide: boolean) {
		sidebarDoors.setHidden(id, hide).catch(() => toast.error("Couldn't change the sidebar. Try again."));
	}

	/**
	 * Every door with a check beside the shown ones: the way back to a hidden
	 * door. The last shown door cannot be unchecked, so a door, and this menu
	 * on it, is always there.
	 */
	function customizeSidebarItem(divider = true): ContextMenuItem {
		const lastShown = sidebarDoors.visible.length === 1;
		return {
			id: 'customize-sidebar',
			label: 'Customize sidebar',
			icon: 'mynaui:panel-left',
			dividerBefore: divider,
			submenu: [
				...sidebarDoors.all.map((d): ContextMenuItem => {
					const shown = !sidebarDoors.hidden.has(d.id);
					return {
						id: `door-${d.id}`,
						label: d.label,
						role: 'menuitemcheckbox',
						checked: shown,
						disabled: shown && lastShown,
						action: () => setDoorHidden(d.id, shown),
					};
				}),
				{
					id: 'doors-reset',
					label: 'Reset to default',
					dividerBefore: true,
					disabled: !sidebarDoors.customized,
					action: () => {
						sidebarDoors.reset().catch(() => toast.error("Couldn't change the sidebar. Try again."));
					},
				},
			],
		};
	}

	function doorMenu(id: DoorId, index: number): ContextMenuItem[] {
		const items: ContextMenuItem[] = [];
		if (index > 0) {
			items.push({ id: 'door-up', label: 'Move up', icon: 'mynaui:arrow-up', shortcut: '⌥↑', action: () => moveDoor(index, index - 1) });
		}
		if (index < doors.length - 1) {
			items.push({ id: 'door-down', label: 'Move down', icon: 'mynaui:arrow-down', shortcut: '⌥↓', action: () => moveDoor(index, index + 1) });
		}
		items.push({
			id: 'door-hide',
			label: 'Hide',
			icon: 'mynaui:eye-off',
			dividerBefore: items.length > 0,
			disabled: doors.length === 1,
			action: () => setDoorHidden(id, true),
		});
		items.push(customizeSidebarItem());
		return items;
	}

// ── opening rows ───────────────────────────────────────────────────────

	/**
	 * Move a pin, from a drop or the keyboard. The store moves it at once and
	 * puts it back if the box refuses, so a failure needs saying.
	 */
	function movePin(from: number, to: number) {
		pinsStore.reorder(from, to).catch(() => toast.error("Couldn't move that pin. Try again."));
	}

	/**
	 * A drag over Pinned: a pin moving, or a Recent row (or any sidebar row)
	 * arriving. Over a pinned project's row, the middle files the dragged
	 * thing into the project and the edges pin beside it.
	 */
	function overPinned(e: DragEvent, project: string | null = null, self = false) {
		const ref = isRefDrag(e);
		if (draggingPinId === null && !ref) return;
		cancelAnimationFrame(pinLeaveFrame);
		const at = pinGapAt(e.currentTarget as Element, e.clientY, ref && !!project && !self);
		e.preventDefault();
		if (at === 'file') {
			pinGap = null;
			rowDropTarget = project;
			if (e.dataTransfer) e.dataTransfer.dropEffect = 'link';
			return;
		}
		rowDropTarget = null;
		pinGap = at;
		if (e.dataTransfer) e.dataTransfer.dropEffect = draggingPinId !== null ? 'move' : 'link';
	}

	/** A drop on Pinned at the line: move the pin, or pin what arrived. True if it took the drop. */
	function dropOnPinned(e: DragEvent): boolean {
		const gap = pinGap;
		pinGap = null;
		if (gap === null) return false;
		e.preventDefault();
		if (draggingPinId !== null) {
			const from = pins.findIndex((p) => p.id === draggingPinId);
			if (from >= 0) movePin(from, from < gap ? gap - 1 : gap);
			return true;
		}
		const url = e.dataTransfer?.getData(REF_DRAG_TYPE);
		if (url) {
			const label = e.dataTransfer?.getData('text/plain') || null;
			void pinAtGap({ url, label }, gap).catch(() => toast.error("Couldn't pin that. Try again."));
		}
		return true;
	}

	/** Leaving Pinned altogether takes the line and the tint away. */
	function leavePinned(e: DragEvent) {
		const area = e.currentTarget as HTMLElement;
		if (e.relatedTarget && area.contains(e.relatedTarget as Node)) return;
		cancelAnimationFrame(pinLeaveFrame);
		pinLeaveFrame = requestAnimationFrame(() => {
			pinGap = null;
			rowDropTarget = null;
		});
	}

	function leaveDoors(e: DragEvent) {
		const area = e.currentTarget as HTMLElement;
		if (e.relatedTarget && area.contains(e.relatedTarget as Node)) return;
		cancelAnimationFrame(doorLeaveFrame);
		doorLeaveFrame = requestAnimationFrame(() => (doorGap = null));
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
	// Rename happens in the row: its title becomes a field. Enter or leaving
	// the field saves, Escape puts the old name back. A stand-in name starts
	// the field empty, so typing does not have to delete "Untitled" first.
	let renaming = $state<{ url: string; draft: string; original: string } | null>(null);

	function startRename(url: string, current: string, kind = '') {
		renaming = { url, draft: isUntitled(current, kind) ? '' : current, original: current };
	}

	async function commitRename() {
		const r = renaming;
		renaming = null;
		const title = r?.draft.trim();
		if (!r || !title || title === r.original) return;
		try {
			await renameRef(r.url, title);
		} catch (e) {
			console.error('[HomePanel] rename failed:', e);
			toast.error(`Your server couldn't rename "${r.original}"`, { description: 'It keeps its old name. Try again' });
		}
	}

	/** Chats, pages and projects own their names; a web or app-screen pin owns its own. */
	function canRename(url: string): boolean {
		if (ownedRef(url)) return true;
		const pin = pins.find((p) => p.url === url);
		return !!pin && ['web', 'route'].includes(pinIdentity(pin).kind);
	}

	/** A row's open: ⌘-click (Ctrl on Windows and Linux) opens it in a new tab. */
	function openFromRow(e: MouseEvent, url: string, title: string, open: () => void) {
		if (renaming?.url === url) return;
		if ((e.metaKey || e.ctrlKey) && !isExternal(url)) windowShellStore.openInNewTab(url, title);
		else open();
	}

	/** Double-click or F2 renames, where the thing has a name to change. */
	function renameFromRow(e: Event, url: string, title: string, kind = '') {
		if (!canRename(url)) return;
		e.preventDefault();
		startRename(url, title, kind);
	}

	function focusAndSelect(node: HTMLInputElement) {
		node.focus();
		node.select();
	}

	function renameChat(s: ChatSession) {
		startRename(chatRoute(s), titleOf(s), 'chat');
	}

	function renamePage(p: PageSummary) {
		startRename(pageRoute(p), pageTitle(p), 'page');
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
			...openMenuItems(url, label),
			{ id: 'rename', label: 'Rename', icon: 'mynaui:edit', action: () => renameChat(s) },
			...getProjectMenuItems(url, label),
			pinMenuItem({ url, label, icon: s.icon }),
			{
				id: 'delete',
				label: 'Delete',
				icon: 'mynaui:trash',
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
			...openMenuItems(url, label),
			{ id: 'rename', label: 'Rename', icon: 'mynaui:edit', action: () => renamePage(p) },
			...getProjectMenuItems(url, label),
			pinMenuItem({ url, label, icon: p.icon }),
			{
				id: 'delete',
				label: 'Delete',
				icon: 'mynaui:trash',
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
		const items = pinMenuItems(pin, at);
		// Moving is the keyboard's drag, and the menu is where it is found.
		const index = pins.findIndex((p) => p.id === pin.id);
		const moves: ContextMenuItem[] = [];
		if (index > 0) {
			moves.push({ id: 'pin-up', label: 'Move up', icon: 'mynaui:arrow-up', shortcut: '⌥↑', action: () => movePin(index, index - 1) });
		}
		if (index >= 0 && index < pins.length - 1) {
			moves.push({ id: 'pin-down', label: 'Move down', icon: 'mynaui:arrow-down', shortcut: '⌥↓', action: () => movePin(index, index + 1) });
		}
		if (moves.length) moves[0] = { ...moves[0], dividerBefore: true };
		return [...items, ...moves];
	}

	function pinMenuItems(pin: Pin, at: { x: number; y: number }): ContextMenuItem[] {
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
					icon: 'mynaui:pin',
					dividerBefore: true,
					action: () => void pinsStore.remove(pin.id),
				});
			}
			return base;
		}

		// No record behind it: an external URL or an app screen, where the pin
		// is the thing, so its name and icon are the pin's own.
		const items: ContextMenuItem[] = [];
		if (!isExternal(pin.url)) items.push(...openMenuItems(pin.url, pinLabel(pin)));
		if (look.kind === 'web' || look.kind === 'route') {
			items.push({
				id: 'rename',
				label: 'Rename',
				icon: 'mynaui:edit',
				action: () => startRename(pin.url, look.title),
			});
		}
		// The picker holds the icon AND the color, so one entry, not two.
		if (iconItem) items.push(iconItem);
		items.push({
			id: 'unpin',
			label: 'Unpin',
			icon: 'mynaui:pin',
			dividerBefore: items.length > 0,
			action: () => {
				void pinsStore.remove(pin.id);
			},
		});
		return items;
	}

	/** A Recent row opens its thing, in the tab already showing it if there is one. */
	function openRecent(row: RecentRow) {
		windowShellStore.openTabFromRoute(row.url, { label: row.title, focusExisting: true });
	}

	/** A Recent row's menu is its thing's menu; a task or applet gets the shared verbs. */
	function recentMenu(row: RecentRow): ContextMenuItem[] {
		if (row.kind === 'chat') return chatMenu(row.session);
		if (row.kind === 'page') return pageMenu(row.page);
		if (row.kind === 'project') return projectRowMenuItems(row.project);
		return [
			...openMenuItems(row.url, row.title),
			pinMenuItem({ url: row.url, label: row.title }),
		];
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
<div
	class="panel-doors"
	role="group"
	aria-label="Doors"
	ondragleave={leaveDoors}
	oncontextmenu={(e) => {
		// Between the rows, or under them when every door is hidden: the
		// way back to a hidden door.
		if (e.target === e.currentTarget) showMenu(e, [customizeSidebarItem(false)]);
	}}
>
	{#if setupOpen}
		<button type="button" class="panel-row panel-door" onclick={openSetup}>
			<SetupRing size={16} />
			<span class="panel-row-text">Setup</span>
			<span class="door-meta">{setupCount}</span>
		</button>
	{/if}
	{#each doors as door, index (door.id)}
		{@const act = doorActions[door.id]}
		<!-- The slot moves as one when doors are reordered, so the Projects
		     door carries its tree with it. -->
		<div class="door-slot" animate:rowFlip>
		<!-- A div, not a button, so a door's + can be a real button inside it. -->
		<div
			class="panel-row panel-door drop-slot"
			class:panel-row-has-actions={!!act.add}
			class:drop-before={doorLine === index}
			class:drop-after={doorLine === doors.length && index === doors.length - 1}
			class:dragging={draggingDoorId === door.id}
			class:stale-hover={staleHover}
			role="link"
			tabindex="0"
			aria-keyshortcuts="Alt+ArrowUp Alt+ArrowDown"
			draggable="true"
			ondragstart={(e) => {
				liftRow(e);
				draggingDoorId = door.id;
				e.dataTransfer?.setData(DOOR_DRAG_TYPE, door.id);
				if (e.dataTransfer) e.dataTransfer.effectAllowed = 'move';
			}}
			ondragend={endDrag}
			ondragover={(e) => {
				if (draggingDoorId === null) return;
				e.preventDefault();
				cancelAnimationFrame(doorLeaveFrame);
				const box = (e.currentTarget as HTMLElement).getBoundingClientRect();
				doorGap = e.clientY - box.top < box.height / 2 ? index : index + 1;
				if (e.dataTransfer) e.dataTransfer.dropEffect = 'move';
			}}
			ondrop={(e) => {
				const from = doors.findIndex((d) => d.id === draggingDoorId);
				const gap = doorGap;
				if (from < 0 || gap === null) return;
				e.preventDefault();
				endDrag();
				moveDoor(from, from < gap ? gap - 1 : gap);
			}}
			onclick={act.open}
			onkeydown={(e) => {
				// The row's own keys only: Enter on its + presses the +.
				if ((e.key === 'Enter' || e.key === ' ') && e.target === e.currentTarget) {
					e.preventDefault();
					act.open();
					return;
				}
				// The keyboard's drag, as on a pin.
				if (e.altKey && (e.key === 'ArrowUp' || e.key === 'ArrowDown')) {
					e.preventDefault();
					const to = index + (e.key === 'ArrowUp' ? -1 : 1);
					if (to < 0 || to >= doors.length) return;
					const row = e.currentTarget as HTMLElement;
					moveDoor(index, to);
					void tick().then(() => row.focus());
				}
			}}
			oncontextmenu={(e) => showMenu(e, doorMenu(door.id, index))}
		>
			<AtlasIcon name={door.glyph} size={SHELL_GLYPH_PX} bare />
			{#if door.id === 'projects' && projects.length}
				{@const shut = folded('projects')}
				<span class="panel-row-text door-label">
					{door.label}
					<button
						type="button"
						class="door-chev"
						class:shut
						aria-expanded={!shut}
						aria-label={shut ? 'Show projects' : 'Hide projects'}
						data-tip={shut ? 'Show projects' : 'Hide projects'}
						onclick={toggleProjectTree}
					>
						<svg class="chev" width="9" height="6" viewBox="0 0 10 6" fill="none" aria-hidden="true">
							<path d="M1 1l4 4 4-4" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" />
						</svg>
					</button>
				</span>
				{#if !shut}
					<span class="tree-stem" class:lit={openProjectId !== null} aria-hidden="true"></span>
				{/if}
			{:else}
				<span class="panel-row-text">{door.label}</span>
			{/if}
			{#if act.add}
				{@const add = act.add}
				<span class="row-actions">
					<button type="button" class="row-action" aria-label={add.label} data-tip={add.label} data-tip-key={add.key} onclick={add.run}>
						<AtlasIcon name="plus" size={14} bare />
					</button>
				</span>
			{/if}
		</div>
		{#if door.id === 'projects' && projects.length}
			<div class="sidebar-expandable fold" class:expanded={!folded('projects')} style={foldStyle(projects.length)}>
				<div class="sidebar-expandable-inner">
					{@render projectTree()}
				</div>
			</div>
		{/if}
		</div>
	{/each}
</div>

<!-- Projects as a tree under their door. The rows start at the door's word.
     A faint line runs from the door's glyph down the tree, and the stretch
     of it that leads to the open project is drawn in ink and curves into
     that row. -->
{#snippet projectTree()}
	<div class="project-tree" role="group" aria-label="Projects">
		<span class="tree-guide" aria-hidden="true"></span>
		{#each projects as p, i (p.id)}
			{@const url = `/project/${p.id}`}
			<div
				class="panel-row tree-row"
				class:active={activeRoute === url}
				class:stale-hover={staleHover}
				role="link"
				tabindex="0"
				data-tip={p.name}
				onclick={() => openProject(p)}
				onkeydown={(e) => {
					if ((e.key === 'Enter' || e.key === ' ') && e.target === e.currentTarget) {
						e.preventDefault();
						openProject(p);
					}
				}}
				oncontextmenu={(e) => showMenu(e, projectRowMenuItems(p))}
			>
				{#if activeRoute === url}
					<span class="tree-branch" style="--tree-i: {i}" aria-hidden="true"></span>
				{/if}
				<span class="panel-row-text">{p.name}</span>
			</div>
		{/each}
	</div>
{/snippet}

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
			data-tip={folded(id) ? `Show ${label}` : `Hide ${label}`}
			onclick={(e) => toggleGroup(id, e.currentTarget.parentElement as HTMLElement)}
		>
			<span>{label}</span>
			<svg class="chev" width="9" height="6" viewBox="0 0 10 6" fill="none" aria-hidden="true">
				<path d="M1 1l4 4 4-4" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" />
			</svg>
		</button>
		<svg class="group-rule" viewBox="0 0 100 4" preserveAspectRatio="none" aria-hidden="true">
			<path d={ruleStroke(id)} vector-effect="non-scaling-stroke" />
		</svg>
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
		data-tip="Filter recents"
		onclick={(e) => showMenuFromButton(e, recentFilterMenu())}
	>
		<AtlasIcon name="filter" size={14} bare />
	</button>
{/snippet}

<!-- One row for every kind. No glyph: Recent is a list of titles, and they
     start under the group's label. Ten repeated marks would make it a second
     block of doors. The tooltip says what kind of thing it is. -->
{#snippet recentRow(row: RecentRow)}
	{@const pinned = isPinned(row.url)}
	{@const chatId = row.kind === 'chat' ? row.session.conversation_id : null}
	{@const home = row.kind === 'chat' && row.session.project_id ? projectStore.byId(row.session.project_id) : undefined}
	<div
		class="panel-row panel-row-has-actions"
		class:active={activeRoute === row.url}
		class:dragging={draggingRecent === row.url}
		class:stale-hover={staleHover}
		role="link"
		tabindex="0"
		data-tip={`${row.title} · ${home ? home.name : RECENT_KIND_LABEL[row.kind]}`}
		data-tip-side="right"
		draggable={renaming?.url !== row.url}
		ondragstart={(e) => {
			liftRow(e);
			draggingRecent = row.url;
			if (projectMemberUrl(row.url)) startRefDrag(e, row.url, row.title);
			else e.dataTransfer?.setData('text/plain', row.title);
			setMentionDrag(e, row.url, row.title);
		}}
		ondragend={endDrag}
		onclick={(e) => openFromRow(e, row.url, row.title, () => openRecent(row))}
		ondblclick={(e) => renameFromRow(e, row.url, row.title, row.kind)}
		onkeydown={(e) => {
			// The row's own keys only: Enter on Pin or More presses that button.
			if ((e.key === 'Enter' || e.key === ' ') && e.target === e.currentTarget) {
				e.preventDefault();
				openRecent(row);
			} else if (e.key === 'F2') {
				renameFromRow(e, row.url, row.title, row.kind);
			}
		}}
		oncontextmenu={(e) => showMenu(e, recentMenu(row))}
	>
		{@render rowTitle(row.url, row.title, row.untitled)}
		{#if chatId && chatActivity.running(chatId)}
			<span class="row-running" role="img" aria-label="Replying">
				<ThinkingMark depth={3} size={14} />
			</span>
		{:else}
			{@render unreadDot(chatId)}
		{/if}
		<span class="row-actions">
			<button
				type="button"
				class="row-action"
				class:on={pinned}
				aria-label={pinned ? 'Unpin' : 'Pin'}
				data-tip={pinned ? 'Unpin' : 'Pin'}
				onclick={(e) => quickPin(e, { url: row.url, label: row.title })}
			>
				<AtlasIcon name={pinned ? 'pinned' : 'pin'} size={14} bare />
			</button>
			<button
				type="button"
				class="row-action"
				aria-label="More"
				data-tip="More"
				onclick={(e) => showMenuFromButton(e, recentMenu(row))}
			>
				<AtlasIcon name="more" size={14} bare />
			</button>
		</span>
	</div>
{/snippet}

<!-- A row's title, or the field that renames it. The field keeps every key
     to itself: the row's own Enter and Space would open the thing. -->
{#snippet rowTitle(url: string, title: string, untitledTitle: boolean)}
	{#if renaming?.url === url}
		<input
			class="row-rename"
			aria-label="Name"
			bind:value={renaming.draft}
			placeholder={title}
			use:focusAndSelect
			onclick={(e) => e.stopPropagation()}
			ondblclick={(e) => e.stopPropagation()}
			onkeydown={(e) => {
				e.stopPropagation();
				if (e.key === 'Enter') {
					e.preventDefault();
					void commitRename();
				} else if (e.key === 'Escape') {
					e.preventDefault();
					renaming = null;
				}
			}}
			onblur={() => void commitRename()}
		/>
	{:else}
		<span class="panel-row-text" class:untitled={untitledTitle}>{title}</span>
	{/if}
{/snippet}

<!-- A reply landed while the chat was off screen. It sits where the row's
     controls appear, and gives way to them on hover. -->
{#snippet unreadDot(chatId: string | null)}
	{#if chatId && chatActivity.unread(chatId)}
		<span class="row-unread" role="img" aria-label="New reply"></span>
	{/if}
{/snippet}

{#if pins.length > 0 || pinnedOpenForDrop}
<!-- Pinned takes drops anywhere in it: a pin moving, a Recent row or a tab
     being pinned. The head pins at the top; an empty Pinned shows only while
     something is being dragged, as a row to drop on. -->
<div class="pinned-zone" role="group" aria-label="Pinned" ondragleave={leavePinned} transition:zoneSlide>
	<!-- Folded, the rows are out of reach and every drop lands at the top,
	     so the head draws that gap's line under itself. -->
	<div
		class="pin-head"
		class:drop-under={pinLine === 0 && pins.length > 0 && folded('pinned')}
		{...{ [PIN_DROP_ATTR]: 'head' }}
		ondragover={(e) => overPinned(e)}
		ondrop={(e) => dropOnPinned(e)}
	>
		{@render groupHead('pinned', 'Pinned', Math.max(1, pins.length))}
	</div>
	{#if pins.length === 0}
		<div
			class="panel-row pin-drop-empty"
			class:drop-here={shownGap === 0}
			{...{ [PIN_DROP_ATTR]: 'head' }}
			ondragover={(e) => overPinned(e)}
			ondrop={(e) => dropOnPinned(e)}
		>
			<span class="panel-row-text">Drop here to pin</span>
		</div>
	{/if}
	<div class="sidebar-expandable fold" class:expanded={!folded('pinned')} style={foldStyle(pins.length)}>
		<div class="sidebar-expandable-inner">
			{#each pins as pin, index (pinsStore.keyOf(pin))}
				{@const pinChat = pin.url.startsWith('/chat/') ? pin.url.slice('/chat/'.length) : null}
				{@const look = pinIdentity(pin)}
				{@const owned = ownedRef(pin.url)}
				{@const pinProject = owned?.kind === 'project' ? owned.id : null}
				<div
					in:rowIn={pin.url}
					out:rowOut={pin.url}
					animate:rowFlip
					class="panel-row panel-row-has-actions drop-slot"
					class:active={activeRoute === pin.url}
					class:drop-target={pinProject !== null &&
						(dndManager.projectTarget === pinProject || rowDropTarget === pinProject)}
					class:drop-before={pinLine === index}
					class:drop-after={pinLine === pins.length && index === pins.length - 1}
					class:dragging={draggingPinId === pin.id}
					class:stale-hover={staleHover}
					{...pinProject ? { [PROJECT_DROP_ATTR]: pinProject } : {}}
					{...{ [PIN_DROP_ATTR]: String(index) }}
					role="link"
					tabindex="0"
					data-tip={look.title}
					data-tip-side="right"
					aria-keyshortcuts="Alt+ArrowUp Alt+ArrowDown"
					draggable={renaming?.url !== pin.url}
					ondragstart={(e) => {
						liftRow(e);
						draggingPinId = pin.id;
						// A thing that can be filed also drags as a row, so it can
						// still be dropped on a project or into a page.
						if (projectMemberUrl(pin.url)) startRefDrag(e, pin.url, look.title);
						else e.dataTransfer?.setData('text/plain', look.title);
						setMentionDrag(e, pin.url, look.title);
						if (e.dataTransfer) e.dataTransfer.effectAllowed = 'linkMove';
					}}
					ondragend={endDrag}
					ondragover={(e) => overPinned(e, pinProject, draggingPinId === pin.id)}
					ondrop={(e) => {
						rowDropTarget = null;
						if (dropOnPinned(e)) return;
						const dropped = droppedRefUrl(e);
						if (!pinProject || !dropped) return;
						e.preventDefault();
						void fileIntoProject(
							projects.find((x) => x.id === pinProject) ?? { id: pinProject, name: look.title, archived_at: null },
							dropped,
						);
					}}
					onclick={(e) => openFromRow(e, pin.url, look.title, () => openPin(pin))}
					ondblclick={(e) => renameFromRow(e, pin.url, look.title, look.kind)}
					onkeydown={(e) => {
						// The row's own keys only: Enter on Unpin or More presses that button.
						if ((e.key === 'Enter' || e.key === ' ') && e.target === e.currentTarget) {
							e.preventDefault();
							openPin(pin);
							return;
						}
						if (e.key === 'F2') {
							renameFromRow(e, pin.url, look.title, look.kind);
							return;
						}
						// The keyboard's drag: Alt and an arrow move the pin a row,
						// as Alt-arrows move a line on a page. Focus stays on it.
						if (e.altKey && (e.key === 'ArrowUp' || e.key === 'ArrowDown')) {
							e.preventDefault();
							const to = index + (e.key === 'ArrowUp' ? -1 : 1);
							if (to < 0 || to >= pins.length) return;
							const row = e.currentTarget as HTMLElement;
							movePin(index, to);
							void tick().then(() => row.focus());
						}
					}}
					oncontextmenu={(e) => showMenu(e, pinMenu(pin, { x: e.clientX, y: e.clientY }))}
				>
					<!-- The mark the user picked, else the glyph of what it opens, in
					     the pin's cloth. A solid dot read as a status light: red and
					     amber said error and warning about a page that had neither. -->
					<span class="row-glyph" aria-hidden="true">
						{#if pinChat && chatActivity.running(pinChat)}
							<ThinkingMark depth={3} size={14} />
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
							<span class="row-kind" style="color: {clothFor({ url: pin.url, color: look.color })}">
								<AtlasIcon name={pinKindGlyph(pin.url)} size={SHELL_GLYPH_PX} bare />
							</span>
						{/if}
					</span>
					{@render rowTitle(pin.url, look.title, isUntitled(look.title, look.kind))}
					{@render unreadDot(pinChat)}
					<span class="row-actions">
						<button
							type="button"
							class="row-action"
							aria-label="Unpin"
							data-tip="Unpin"
							onclick={(e) => quickPin(e, { url: pin.url, label: look.title })}
						>
							<AtlasIcon name="pinned" size={14} bare />
						</button>
						<button
							type="button"
							class="row-action"
							aria-label="More"
							data-tip="More"
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
</div>
{/if}

{#if recents.length > 0}
	{@render groupHead('recent', recentLabel, Math.max(1, recentsShown.length), recentHeadActions, recentFilter !== 'all')}
	<div class="sidebar-expandable fold" class:expanded={!folded('recent')} style={foldStyle(recentsShown.length)}>
		<div class="sidebar-expandable-inner">
			{#if recentsShown.length === 0}
				<div class="panel-row panel-empty">No recent {filterDef.plural}</div>
			{/if}
			{#each recentsShown as row (row.url)}
				<div class="row-slot" in:rowIn={row.url} out:rowOut={row.url} animate:rowFlip>
					{@render recentRow(row)}
				</div>
			{/each}
		</div>
	</div>
{/if}

<!-- Hidden while Recent is folded: "See all" under a closed heading would
     point at nothing on screen. -->
{#if recentsMore && !folded('recent')}
	<button type="button" class="panel-row panel-more" onclick={() => filterDef.seeAll()}>See all</button>
{/if}

<div bind:this={spacerEl} class="fold-spacer" style="height: {foldSpacer}px" aria-hidden="true"></div>

<style>
	/* Sentence case, no tracking — design.md forbids uppercase with wide
	   tracking as something that "signals considered while doing no work". A
	   group is told from its rows by being SMALLER and quieter, by the air
	   above it, and by the hand-drawn rule that runs on from its label. */
	.group-head {
		display: flex;
		align-items: center;
		/* More air above the label than below it, so it reads as the head of
		   the rows under it and not the tail of the ones above. */
		height: 22px;
		margin-top: 16px;
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
	.door-chev,
	.fold {
		--fold-open: var(--fold-ms, 200ms);
		--fold-close: calc(var(--fold-ms, 200ms) * 0.85);
		--fold-ease: cubic-bezier(0.3, 0.1, 0.2, 1);
	}

	/* At rest the chevron takes no room, so the rule starts right after the
	   label. Pointing at the head opens its slot, and the rule shortens to
	   make way for it. The negative margin cancels the label's gap. */
	.group-head .chev {
		width: 0;
		margin-left: -6px;
		opacity: 0;
		transform: rotate(0deg);
		transition:
			width 180ms var(--fold-ease),
			margin-left 180ms var(--fold-ease),
			opacity 150ms ease,
			transform var(--fold-open) var(--fold-ease);
	}

	.group-head.folded .chev {
		transition:
			width 180ms var(--fold-ease),
			margin-left 180ms var(--fold-ease),
			opacity 150ms ease,
			transform var(--fold-close) var(--fold-ease);
	}

	.group-head:hover .chev,
	.group-toggle:focus-visible .chev,
	.group-head.folded .chev {
		width: 9px;
		margin-left: 0;
		opacity: 0.8;
	}

	/* Shut: the chevron stays put, pointing at the group it would reopen. */
	.group-head.folded .chev {
		transform: rotate(-90deg);
	}

	/* The rule fills the room between the label and the head's actions, so
	   it shortens at both ends as the chevron and the actions open their
	   slots. It stays out of the hit area: the label is the toggle, the rule
	   is only ink. */
	.group-rule {
		flex: 1;
		min-width: 0;
		height: 4px;
		margin: 0 8px;
		fill: none;
		stroke: var(--color-foreground-subtle);
		stroke-width: 1;
		stroke-linecap: round;
		opacity: 0.45;
		pointer-events: none;
	}

	/* The head's actions open their slot the same way, from the right. */
	.group-actions {
		margin-left: auto;
		display: flex;
		align-items: center;
		max-width: 0;
		overflow: hidden;
		opacity: 0;
		transition:
			max-width 180ms var(--fold-ease),
			opacity 150ms ease;
	}

	.group-head:hover .group-actions,
	.group-actions:focus-within,
	.group-actions.shown {
		max-width: 28px;
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

	/* The Projects door's word carries its fold. The chevron shows when you
	   point at the door, and turns to say which way the tree is. */
	.door-label {
		display: flex;
		align-items: center;
		gap: 4px;
	}

	.door-chev {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 20px;
		height: 20px;
		padding: 0;
		border: none;
		border-radius: var(--radius-sm);
		background: none;
		color: var(--color-foreground-muted);
		cursor: pointer;
		opacity: 0;
		transition: opacity 150ms ease;
	}

	.panel-door:not(.stale-hover):hover .door-chev,
	.door-chev:focus-visible {
		opacity: 0.8;
	}

	.door-chev:hover {
		background: var(--hover-bg);
	}

	.door-chev .chev {
		transition: transform var(--fold-open) var(--fold-ease);
	}

	.door-chev.shut .chev {
		transform: rotate(-90deg);
		transition-duration: var(--fold-close);
	}

	/* The tree's lines run down the centre of the door's glyph. The stem is
	   the stretch inside the door row, from under the glyph to the row's
	   foot; the tree's own box clips while it folds, so the stem lives here. */
	.tree-stem,
	.tree-guide,
	.tree-branch {
		position: absolute;
		left: calc(12px + var(--sidebar-interactive-icon-size) / 2);
		border-left: 1px solid var(--color-border);
		pointer-events: none;
	}

	.tree-stem {
		top: calc(50% + var(--sidebar-interactive-icon-size) / 2);
		bottom: 0;
	}

	/* Ink from the glyph to the open project, faint everywhere else. */
	.tree-stem.lit {
		border-left-color: var(--color-foreground-muted);
	}

	.project-tree {
		position: relative;
	}

	/* Down the whole tree, to the middle of the last row. */
	.tree-guide {
		top: 0;
		bottom: calc(var(--sidebar-interactive-height) / 2);
	}

	/* From the top of the tree to the open project's middle, then round into
	   its word: a row's height for each project above it. */
	.tree-branch {
		top: calc(-1 * var(--tree-i) * var(--sidebar-interactive-height));
		bottom: 50%;
		width: calc(var(--sidebar-interactive-icon-size) / 2 + var(--sidebar-interactive-gap) - 4px);
		border-left-color: var(--color-foreground-muted);
		border-bottom: 1px solid var(--color-foreground-muted);
		border-bottom-left-radius: var(--radius-md);
	}

	/* A project in the tree starts where its door's word does, in a quieter
	   ink than the doors. The open one is ink on the selected wash. */
	.panel-row.tree-row {
		padding-left: calc(12px + var(--sidebar-interactive-icon-size) + var(--sidebar-interactive-gap));
		color: var(--color-foreground-muted);
	}

	.panel-row.tree-row.active,
	.panel-row.tree-row.active:hover {
		background: var(--active-bg);
		box-shadow: none;
		color: var(--color-foreground);
	}

	.panel-door {
		gap: var(--sidebar-interactive-gap);
		color: var(--color-foreground);
	}

	/* A door's glyph is muted and its word is ink, like the header's controls
	   above it; pointing at the door brings the glyph up to ink. */
	.panel-door > :global(.atlas-icon) {
		color: var(--color-foreground-muted);
		transition: color var(--motion-hover-out) var(--ease-premium);
	}

	.panel-door:not(.stale-hover):hover > :global(.atlas-icon) {
		color: var(--color-foreground);
		transition-duration: var(--motion-hover-in);
	}

	.panel-row {
		position: relative;
		display: flex;
		align-items: center;
		gap: var(--sidebar-interactive-gap);
		width: 100%;
		height: var(--sidebar-interactive-height);
		padding: 0 12px;
		border: none;
		border-radius: var(--sidebar-interactive-radius);
		background: none;
		cursor: pointer;
		text-align: left;
		font-size: var(--chrome-label-font-size);
		color: var(--color-foreground);
		/* A drag starts from a row, so a press-and-move must not select its
		   text instead, which WebKit tries first. */
		user-select: none;
		-webkit-user-select: none;
		/* The scale's motion (design.md, "The scale"), as on the tab strip:
		   hover in fast and out slow. */
		transition:
			background-color var(--motion-hover-out) var(--ease-premium),
			color var(--motion-hover-out) var(--ease-premium),
			opacity var(--motion-hover-out) var(--ease-premium);
	}

	.panel-row:not(.stale-hover):hover {
		background: var(--hover-bg);
		transition-duration: var(--motion-hover-in);
	}

	/* Where you are: the page's own white, edged by a hairline, so the row
	   is made of the same stuff as the card it opened. Hover stays a wash. */
	.panel-row.active,
	.panel-row.active:hover {
		background: var(--color-page);
		box-shadow: inset 0 0 0 1px var(--color-border);
	}

	/* A pinned project with something held over it: the row takes the drop
	   tint, the same one a pane shows for a tab it will take. It outranks
	   the active row's fill, which would otherwise hide it. */
	.panel-row.drop-target,
	.panel-row.drop-target:hover {
		background: var(--drop-tint);
		transition-duration: var(--motion-hover-in);
	}

	/* Where a dragged pin or door will land: a line in the gap above or below
	   the row it is over. It is drawn inside the row, along its top or bottom
	   edge, because the fold that holds Pinned clips anything outside its
	   rows. Both lines are always there, at zero opacity, so they fade. */
	.drop-slot::before,
	.drop-slot::after,
	.pin-head::after {
		content: '';
		position: absolute;
		left: 6px;
		right: 6px;
		height: var(--drop-line-width);
		border-radius: calc(var(--drop-line-width) / 2);
		background: var(--drop-line);
		pointer-events: none;
		opacity: 0;
		transition: opacity var(--motion-hover-out) var(--ease-premium);
	}
	.drop-slot::before {
		top: 0;
	}
	.drop-slot::after,
	.pin-head::after {
		bottom: 0;
	}
	.drop-slot.drop-before::before,
	.drop-slot.drop-after::after,
	.pin-head.drop-under::after {
		opacity: 1;
		transition-duration: var(--motion-hover-in);
	}

	.pin-head {
		position: relative;
	}

	/* Only while something is dragged and nothing is pinned yet: the row to
	   drop on, in the meta ink, tinted like a drop target when it is under
	   the pointer. */
	.pin-drop-empty {
		color: var(--color-foreground-subtle);
		cursor: default;
	}
	.pin-drop-empty.drop-here {
		background: var(--drop-tint);
		transition-duration: var(--motion-hover-in);
	}

	/* The row in hand stays in its place, dimmed, until it lands. */
	.panel-row.dragging {
		opacity: var(--drag-source-opacity);
	}

	/* The row as the browser pictures it when a drag starts: the whole pill
	   in the page's fill, with no hover wash, no controls and no fade on its
	   title. liftRow sets it during dragstart and takes it off in the next
	   task. The browser still holds the row's :hover and :focus-within while
	   it takes the picture, hence !important. */
	.panel-row:global(.lifting) {
		background: var(--color-page) !important;
		color: var(--color-foreground) !important;
		opacity: 1 !important;
		border-radius: var(--card-radius);
		box-shadow: none !important;
		outline: 1px solid var(--card-edge) !important;
		outline-offset: -1px;
		transition: none !important;
	}
	.panel-row:global(.lifting) .row-actions {
		visibility: hidden;
	}
	.panel-row:global(.lifting) .panel-row-text {
		mask-image: none !important;
	}
	.panel-row:global(.lifting) :is(.row-unread, .row-running) {
		opacity: 1 !important;
		transition: none;
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
		/* Dates stack: "13" sits over "11". */
		font-variant-numeric: tabular-nums;
	}

	/* The rename field sits exactly where the title was, in its face and size. */
	.row-rename {
		flex: 1;
		min-width: 0;
		height: 22px;
		margin: 0 -4px;
		padding: 0 4px;
		border: none;
		border-radius: var(--radius-sm);
		background: var(--color-page);
		box-shadow: inset 0 0 0 1px var(--color-border);
		color: var(--color-foreground);
		font: inherit;
		outline: none;
	}

	.row-rename:focus {
		box-shadow: inset 0 0 0 1.5px var(--color-primary);
	}

	/* A stand-in name ("Untitled", "New chat") looks unnamed. */
	.panel-row-text.untitled {
		color: var(--color-foreground-subtle);
	}

	/* Recent's rows sit in a slot that carries their motion. */
	.row-slot {
		display: flex;
		flex-direction: column;
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

	/* It arrives once, and then holds still: a dot that kept pulsing would
	   say something is still happening. */
	.row-unread {
		width: 7px;
		height: 7px;
		margin-right: 2px;
		flex: none;
		border-radius: var(--radius-full);
		/* The accent, as on the header mark's inbox dot: "something new" has
		   one color everywhere. Green would say something succeeded. */
		background: var(--color-primary);
		animation: unread-in 180ms var(--ease-premium);
	}

	@keyframes unread-in {
		from {
			transform: scale(0.6);
			opacity: 0;
		}
	}

	/* The marks keep their place under the controls, so the title's width
	   does not change on hover. */
	.row-unread,
	.row-running {
		transition: opacity var(--motion-hover-out) var(--ease-premium);
	}

	.panel-row-has-actions:not(.stale-hover):is(:hover, :focus-within) :is(.row-unread, .row-running) {
		opacity: 0;
	}

	/* The controls appear where the pointer is and nowhere else. They float
	   over the end of the row, and the title fades out under them, so the
	   title never moves: reflowing it to make room shortened it and jumped its
	   ellipsis on every hover. */
	.row-actions {
		position: absolute;
		top: 50%;
		right: 6px;
		display: flex;
		align-items: center;
		gap: 4px;
		transform: translateY(-50%);
		opacity: 0;
		pointer-events: none;
		transition: opacity var(--motion-hover-out) var(--ease-premium);
	}

	.panel-row-has-actions:not(.stale-hover):is(:hover, :focus-within) .row-actions {
		opacity: 1;
		pointer-events: auto;
		transition-duration: var(--motion-hover-in);
	}

	/* The last 48px of the row hold the controls: the title is clear up to
	   20px before them and gone under them. */
	.panel-row-has-actions:not(.stale-hover):is(:hover, :focus-within) .panel-row-text {
		mask-image: linear-gradient(to left, transparent 42px, #000 62px);
	}

	.row-action {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 22px;
		height: 22px;
		padding: 0;
		border: none;
		border-radius: var(--radius-md);
		background: transparent;
		color: var(--color-foreground-subtle);
		cursor: pointer;
	}

	/* A control inside a hovered row takes the next step up, the active
	   fill, so the ladder has two rungs and no in-between. */
	.row-action:hover {
		background: var(--active-bg);
		color: var(--color-foreground);
	}

	.row-action:active {
		background: var(--press-bg);
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

	.panel-row.panel-empty:hover {
		background: none;
	}

	/* Avenir's regular is thin at row size on a 1x display; its medium holds up. */
	@media (max-resolution: 1.5dppx) {
		.panel-row {
			font-weight: 500;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.row-unread {
			animation: none;
		}
	}

	.door-meta {
		margin-left: auto;
		font-size: 12px;
		color: var(--color-foreground-subtle);
		font-variant-numeric: tabular-nums;
	}
</style>
