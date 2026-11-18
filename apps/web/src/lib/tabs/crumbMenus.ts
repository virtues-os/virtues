/**
 * What a path segment lists when you click it: what is inside it. Threads
 * lists the door's views, a thread its chats and pages, Settings its rooms by
 * heading. The newest come first and the one you are in is checked. Under
 * them, a row opens the segment itself, and a list that can run long ends in
 * "Search …", which opens the palette already narrowed to that kind.
 */
import type { ContextMenuItem } from '$lib/stores/contextMenu.svelte';
import type { Crumb, CrumbMenu } from '$lib/tabs/crumbs';
import { recentRows, type RecentKind, type RecentRow } from '$lib/sidebar/recents.svelte';
import { appletsStore } from '$lib/stores/applets.svelte';
import { SETTINGS_MODE, STORAGE_MODE, WIKI_MODE, rowForRoute, type ModeRow } from '$lib/sidebar/modes';
import { search } from '$lib/stores/search.svelte';
import { windowShellStore } from '$lib/stores/window-shell.svelte';

const LIST_ROWS = 10;

const GLYPH: Record<RecentKind, string> = {
	chat: 'atlas:chats',
	page: 'atlas:pages',
	thread: 'atlas:projects',
	applet: 'atlas:applets',
};

/** The list kinds: the noun for "All …" and the palette scope. */
const LISTS = {
	pages: { kind: 'page', noun: 'pages' },
	applets: { kind: 'applet', noun: 'applets' },
} as const;

export function crumbMenuItems(
	crumb: Crumb,
	menu: CrumbMenu,
	/** The route of the page the path belongs to. */
	here: string,
	open: (route: string) => void,
): ContextMenuItem[] {
	const current = here.split(/[?#]/)[0];
	const recordRow = (row: RecentRow): ContextMenuItem => ({
		id: row.url,
		label: row.title,
		icon: GLYPH[row.kind],
		checked: row.url === current,
		action: () => open(row.url),
	});

	if (menu.kind === 'mode') return modeItems(menu.mode, menu.group, current, open);

	if (menu.kind === 'threads') return threadViewItems(here, open);

	if (menu.kind === 'thread') {
		const members = recentRows({ pinned: true }).filter(
			(r) =>
				(r.kind === 'chat' && r.session.thread_id === menu.id) ||
				(r.kind === 'page' && (r.page.threads ?? []).some((t) => t.id === menu.id)),
		);
		return withFooter(members.slice(0, LIST_ROWS).map(recordRow), [
			{ id: 'open', label: 'Open thread', icon: 'atlas:projects', action: () => open(`/thread/${menu.id}`) },
		]);
	}

	const list = LISTS[menu.kind];
	const rows =
		menu.kind === 'applets'
			? appletsStore.mine.slice(0, LIST_ROWS).map(
					(a): ContextMenuItem => ({
						id: a.id,
						label: a.name,
						icon: GLYPH.applet,
						checked: `/applet/${a.id}` === current,
						action: () => open(`/applet/${a.id}`),
					}),
				)
			: recentRows({ pinned: true })
					.filter((r) => r.kind === list.kind)
					.slice(0, LIST_ROWS)
					.map(recordRow);
	const footer: ContextMenuItem[] = [];
	if (crumb.route) footer.push({ id: 'all', label: `All ${list.noun}`, icon: GLYPH[list.kind], action: () => open(crumb.route!) });
	footer.push({ id: 'search', label: `Search ${list.noun}…`, icon: 'atlas:search', action: () => search.show(`#${list.noun} `) });
	return withFooter(rows, footer);
}

/**
 * The Threads door's views, the one you are on checked, each with its G
 * shortcut. The door keeps the view in its route (`?view=`), so on the door
 * a view changes that tab's route the way the door's own G keys do;
 * anywhere else under it, the view opens.
 */
function threadViewItems(here: string, open: (route: string) => void): ContextMenuItem[] {
	const [path, query = ''] = here.split('?');
	const on = path === '/threads' ? (new URLSearchParams(query).get('view') ?? 'overview') : null;
	const views: { id: string; label: string; key: string }[] = [
		{ id: 'overview', label: 'Overview', key: 'G O' },
		{ id: 'tasks', label: 'Tasks', key: 'G T' },
		{ id: 'threads', label: 'All threads', key: 'G A' },
		{ id: 'insights', label: 'Insights', key: 'G I' },
	];
	return views.map((v) => ({
		id: `view-${v.id}`,
		label: v.label,
		shortcut: v.key,
		checked: on === v.id,
		action: () => {
			const route = v.id === 'overview' ? '/threads' : `/threads?view=${v.id}`;
			const door = on ? windowShellStore.findTab((t) => t.route === here) : null;
			if (door) windowShellStore.updateTab(door.tab.id, { route });
			else open(route);
		},
	}));
}

function withFooter(rows: ContextMenuItem[], footer: ContextMenuItem[]): ContextMenuItem[] {
	if (rows.length && footer.length) footer[0] = { ...footer[0], dividerBefore: true };
	return [...rows, ...footer];
}

/** A mode's rows. The whole mode puts each heading's rows in a submenu and
 *  the rows with no heading after them; one heading lists its rows. */
function modeItems(
	which: 'settings' | 'wiki' | 'storage',
	group: string | undefined,
	current: string,
	open: (route: string) => void,
): ContextMenuItem[] {
	const mode = which === 'settings' ? SETTINGS_MODE : which === 'storage' ? STORAGE_MODE : WIKI_MODE;
	const at = rowForRoute(mode, current)?.id;
	const item = (row: ModeRow): ContextMenuItem => ({
		id: row.id,
		label: row.label,
		icon: row.glyph ? `atlas:${row.glyph}` : undefined,
		checked: row.id === at,
		action: () => open(row.href),
	});

	if (group) return mode.rows.filter((r) => r.group === group).map(item);

	const items: ContextMenuItem[] = [];
	const groups = [...new Set(mode.rows.map((r) => r.group).filter((g): g is string => !!g))];
	for (const g of groups) {
		const rows = mode.rows.filter((r) => r.group === g);
		items.push({ id: `group-${g}`, label: g, submenu: rows.map(item) });
	}
	const loose = mode.rows.filter((r) => !r.group).map(item);
	if (loose.length && items.length) loose[0] = { ...loose[0], dividerBefore: true };
	return [...items, ...loose];
}
