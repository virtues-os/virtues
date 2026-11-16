/**
 * The path the action toolbar shows for a tab: the room it sits in, then the
 * page. A segment with a menu opens it (`crumbMenus.ts`: what is inside the
 * segment); one with only a route is a link the toolbar opens in place.
 *
 * Derived from the route and the sidebar's own lists (`sidebar/modes.ts`),
 * so a page is placed where the sidebar puts it. A view that knows more,
 * such as the project a chat is filed in, publishes its own path with
 * `paneActions.setCrumbs`.
 */

import { SETTINGS_MODE, WIKI_MODE, rowForRoute, type ModeRow } from '$lib/sidebar/modes';

export interface Crumb {
	label: string;
	/** Opened in the tab when the segment is clicked. The last one has none. */
	route?: string;
	/** Run instead of opening a route, for a view that moves within itself
	 *  (Drive's folders). */
	run?: () => void;
	/** What is inside the segment, listed when it is clicked. */
	menu?: CrumbMenu;
}

export type CrumbMenu =
	| { kind: 'projects' }
	| { kind: 'project'; id: string }
	| { kind: 'pages' }
	| { kind: 'applets' }
	/** A sidebar mode's rows: all of them by heading, or one heading's. */
	| { kind: 'mode'; mode: 'settings' | 'wiki'; group?: string };

export const projectsCrumb = (): Crumb => ({ label: 'Projects', route: '/projects', menu: { kind: 'projects' } });

export const projectCrumb = (id: string, name: string): Crumb => ({
	label: name,
	route: `/project/${id}`,
	menu: { kind: 'project', id },
});

export const wikiCrumb = (): Crumb => ({ label: 'Wiki', route: '/wiki', menu: { kind: 'mode', mode: 'wiki' } });

export interface CrumbPath {
	/** AtlasIcon glyph drawn before the first segment. */
	glyph: string;
	crumbs: Crumb[];
	/** The path stops where the page is filed and leaves the page out. A
	 *  chat's tab already carries its name, so its bar says where the chat
	 *  is instead: its project, or nothing. */
	withoutPage?: boolean;
}

/** A door in the sidebar and the routes that sit under it. */
const DOORS: { prefixes: string[]; root: Crumb; glyph: string }[] = [
	{ prefixes: ['/page'], root: { label: 'Pages', route: '/page', menu: { kind: 'pages' } }, glyph: 'pages' },
	{ prefixes: ['/tasks'], root: { label: 'Tasks', route: '/tasks' }, glyph: 'tasks' },
	{ prefixes: ['/projects', '/project', '/notebook'], root: projectsCrumb(), glyph: 'projects' },
	{ prefixes: ['/applets', '/applet'], root: { label: 'Applets', route: '/applets', menu: { kind: 'applets' } }, glyph: 'applets' },
];

/** Wiki entity routes that are not under their row's href. */
const WIKI_ENTITY_ROWS: Record<string, string> = {
	person: '/wiki/people',
	place: '/wiki/places',
	org: '/wiki/orgs',
	day: '/wiki/days',
	year: '/wiki/years',
};

function under(path: string, prefix: string): boolean {
	return path === prefix || path.startsWith(prefix + '/');
}

/** The root, the row the page sits in (unless the page is the row), then the page. */
function inRow(root: Crumb, row: ModeRow, path: string, title: string): Crumb[] {
	const crumbs: Crumb[] = [root];
	if (path === row.href) {
		crumbs.push({ label: row.label });
		return crumbs;
	}
	crumbs.push({ label: row.label, route: row.href });
	if (title && title !== row.label) crumbs.push({ label: title });
	return crumbs;
}

export function crumbsForRoute(route: string, title: string, fallbackGlyph = 'chats'): CrumbPath {
	const path = route.split('?')[0];

	for (const door of DOORS) {
		if (!door.prefixes.some((p) => under(path, p))) continue;
		const crumbs: Crumb[] = [door.root];
		if (path !== door.root.route && title && title !== door.root.label) crumbs.push({ label: title });
		else crumbs[0] = { label: door.root.label };
		return { glyph: door.glyph, crumbs };
	}

	const settingsRow = rowForRoute(SETTINGS_MODE, path);
	if (settingsRow) {
		const root: Crumb = { label: 'Settings', route: SETTINGS_MODE.rows[0].href, menu: { kind: 'mode', mode: 'settings' } };
		const crumbs = inRow(root, settingsRow, path, title);
		// The section heading sits between Settings and the row (Sources /
		// Catalog), linked to the heading's first row.
		const group = settingsRow.group;
		const first = SETTINGS_MODE.rows.find((r) => r.group === group);
		if (group && first && group !== settingsRow.label) {
			crumbs.splice(1, 0, {
				label: group,
				route: first.href === path ? undefined : first.href,
				menu: { kind: 'mode', mode: 'settings', group },
			});
		}
		return { glyph: 'settings', crumbs };
	}

	const entity = path.match(/^\/(\w+)\//)?.[1];
	const wikiRow =
		rowForRoute(WIKI_MODE, path) ??
		(entity && WIKI_ENTITY_ROWS[entity] ? rowForRoute(WIKI_MODE, WIKI_ENTITY_ROWS[entity]) : null);
	if (wikiRow) {
		if (wikiRow.group === 'Drive') {
			const root: Crumb = { label: 'Drive', route: '/storage', menu: { kind: 'mode', mode: 'wiki', group: 'Drive' } };
			if (wikiRow.href === '/storage') {
				return { glyph: 'drive', crumbs: path === '/storage' ? [{ label: 'Drive' }] : [root, { label: title }] };
			}
			return { glyph: 'drive', crumbs: inRow(root, wikiRow, path, title) };
		}
		return { glyph: 'wiki', crumbs: inRow(wikiCrumb(), wikiRow, path, title) };
	}
	if (under(path, '/wiki')) return { glyph: 'wiki', crumbs: [{ label: path === '/wiki' ? 'Wiki' : title }] };
	if (path.startsWith('/chat/')) return { glyph: fallbackGlyph, crumbs: [], withoutPage: true };

	return { glyph: fallbackGlyph, crumbs: [{ label: title }] };
}
