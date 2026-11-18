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

import { SETTINGS_MODE, STORAGE_MODE, WIKI_MODE, rowForRoute, type ModeRow } from '$lib/sidebar/modes';

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
	| { kind: 'threads' }
	| { kind: 'thread'; id: string }
	| { kind: 'pages' }
	| { kind: 'applets' }
	/** A sidebar mode's rows: all of them by heading, or one heading's. */
	| { kind: 'mode'; mode: 'settings' | 'wiki' | 'storage'; group?: string };

export const threadsCrumb = (): Crumb => ({ label: 'Threads', route: '/threads', menu: { kind: 'threads' } });

export const threadCrumb = (id: string, name: string): Crumb => ({
	label: name,
	route: `/thread/${id}`,
	menu: { kind: 'thread', id },
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
	{ prefixes: ['/threads', '/thread', '/task'], root: threadsCrumb(), glyph: 'projects' },
	{ prefixes: ['/applets', '/applet'], root: { label: 'Applets', route: '/applets', menu: { kind: 'applets' } }, glyph: 'applets' },
];

/** Wiki entity routes that are not under their row's href. */
const WIKI_ENTITY_ROWS: Record<string, string> = {
	person: '/wiki/people',
	place: '/wiki/places',
	org: '/wiki/orgs',
	thing: '/wiki/things',
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

	// Settings' landing page, where every Settings page is linked.
	if (path === '/virtues') return { glyph: 'settings', crumbs: [{ label: 'Settings' }] };
	const settingsRow = rowForRoute(SETTINGS_MODE, path);
	if (settingsRow) {
		const root: Crumb = { label: 'Settings', route: '/virtues', menu: { kind: 'mode', mode: 'settings' } };
		const crumbs = inRow(root, settingsRow, path, title);
		// The landing page's heading sits between Settings and the page
		// (Settings / Your server / Devices), linked to the landing page,
		// where that heading's links are.
		const group = settingsRow.group;
		if (group && group !== settingsRow.label) {
			crumbs.splice(1, 0, {
				label: group,
				route: '/virtues',
				menu: { kind: 'mode', mode: 'settings', group },
			});
		}
		return { glyph: 'settings', crumbs };
	}

	if (path === '/storage') return { glyph: 'drive', crumbs: [{ label: 'Storage' }] };
	const storageRow = rowForRoute(STORAGE_MODE, path);
	if (storageRow) {
		const root: Crumb = { label: 'Storage', route: '/storage', menu: { kind: 'mode', mode: 'storage' } };
		return { glyph: 'drive', crumbs: inRow(root, storageRow, path, title) };
	}

	const entity = path.match(/^\/(\w+)\//)?.[1];
	const wikiRow =
		rowForRoute(WIKI_MODE, path) ??
		(entity && WIKI_ENTITY_ROWS[entity] ? rowForRoute(WIKI_MODE, WIKI_ENTITY_ROWS[entity]) : null);
	if (wikiRow) {
		return { glyph: 'wiki', crumbs: inRow(wikiCrumb(), wikiRow, path, title) };
	}
	if (under(path, '/wiki')) return { glyph: 'wiki', crumbs: [{ label: path === '/wiki' ? 'Wiki' : title }] };
	if (path.startsWith('/chat/')) return { glyph: fallbackGlyph, crumbs: [], withoutPage: true };

	return { glyph: fallbackGlyph, crumbs: [{ label: title }] };
}
