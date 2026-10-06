/**
 * The rail's rooms.
 *
 * The shell was one 220px column whose contents `sidebarMode` swapped for four
 * privileged destinations while the other seven sat as rows on a shelf beneath
 * a Desk. That had one altitude — seven rooms, a Desk and three footer doors
 * all the same size — and entering a mode took the shelf away entirely.
 *
 * A rail fixes that: a spine that does not move while the panel beside it
 * swaps. Modes stop being an exception granted to four rooms and become the
 * rule for all of them — which is why `SIDEBAR_MODES` now supplies the BODY of
 * a room's panel rather than replacing the sidebar.
 *
 * Ten top-level destinations became eight, then six, because a rail of ten
 * indistinguishable glyphs is a memorisation tax rather than a contents page:
 *
 *   - Notebooks (now projects) folded into Pages, and then moved again: a
 *     project is the room a chat lives in, so it is listed in the Home panel
 *     rather than behind a rail door of its own.
 *   - Bookmarks folded into Files (now Drive). Both are "something from
 *     outside that you kept" — the loosest of the merges, and the first to
 *     revisit.
 *   - Home and Chats became one room (2026-09-21). Home's panel had been the
 *     Desk — the pinned shelf — with Chats a peer tile beneath it; the shelf
 *     now sits at the top of the ground room's panel as Pinned, which is
 *     where a pin is reached for. The room is called Home, because the
 *     panel holds chats, pages, projects and applets, and a user reads the
 *     panel's title as the name of the place they are in — "Chats" over a
 *     list of pages and projects was the room named after one of its
 *     contents. Its tile opens a new chat: home and chat are one place, so
 *     there is no separate home page.
 *   - Applets left the rail the same day. An applet is something you run from
 *     a conversation, so its door is a row at the top of the Home panel,
 *     beside New chat — a rail tile for a list that is opened from chat was
 *     a room nobody walked to.
 *   - Pages left the rail the same day too. A page is written the way a chat
 *     is started, so Pages is a door in the Home panel with a `+` beside it,
 *     the same shape Projects has.
 *
 * Developer is a heading inside Settings. It was folded in once before and
 * came back out, because the fold put a second row of underline tabs under a
 * first; a panel heading groups the consoles without nesting any nav, which
 * was the actual objection.
 *
 * ROUTES ARE UNCHANGED. Only labels and grouping move, so the layout can be
 * judged without also judging a migration.
 *
 * Home sits alone above the first gap: it is the ground rather than a peer, so
 * it gets primacy, not parity, and it is the top of the rail, which is what
 * "the ground" should have meant all along.
 *
 * Below it the library: Wiki, Timeline, Drive - what you read the record
 * through. Timeline is the one room with no panel: the day on a map is a
 * page you go to, and its tile opens that page rather than a list beside it.
 *
 * Settings stands alone at the foot. Sources and Developer had rail doors of
 * their own until 2026-10-05; they are headings in the Settings panel now,
 * because connecting a source and opening a console are both things you do
 * to the server and then leave, and a rail of what you read had grown three
 * doors to its plumbing.
 */

/**
 * `setup` holds one room, Setup, drawn ABOVE Home and only while a step of
 * Setup is not done (skipped counts as not done). It is a room like the
 * others — a tile that swaps the panel — and its panel lists the steps. The
 * steps open at /setup: full screen the first time through, and as a tab in
 * the app once someone is in it (setup/inApp.ts). It leaves the rail once
 * every step is done.
 */
export type RoomGroup = 'setup' | 'primary' | 'library' | 'utility';

/** What fills the panel body under the room's title. */
export type RoomPanel =
	/** The fixed rows of a `SIDEBAR_MODES` entry. */
	| { kind: 'rows'; modeId: string }
	/** The Home panel: doors, Pinned, Projects, the recent chats. */
	| { kind: 'home' }
	/** Setup: the steps, and the way back into them. */
	| { kind: 'setup' }
	/** Nothing live yet — the panel offers the room's full page. */
	| { kind: 'stub' }
	/** No panel: the tile opens the room's page and leaves the sidebar as it was. */
	| { kind: 'none' };

export interface Room {
	id: string;
	/** Leads on the rail. Icons assist; labels lead. */
	label: string;
	/** The panel's title when the rail's one-word label is not the room's
	 *  name. */
	title?: string;
	/** An `AtlasIcon` glyph name. */
	icon: string;
	/**
	 * Display only. Deliberately NOT ⌘1-9: those already address tabs across
	 * both panes, and a rail that stole them would break the one shortcut power
	 * users actually have.
	 */
	chord: string;
	/** The room's full page. */
	href: string;
	/**
	 * Route prefixes this room owns, for the occupied mark. Longest match wins,
	 * so order in this array — and in ROOMS — is not load-bearing; specificity
	 * is.
	 */
	owns: string[];
	panel: RoomPanel;
	group: RoomGroup;
}

export const ROOMS: Room[] = [
	{
		// One word for the process everywhere: the flow, the tile, the panel.
		// It owns /setup: in the app, the steps open as a tab there.
		id: 'setup',
		label: 'Setup',
		icon: 'setup',
		chord: '⌥⌘G',
		href: '/setup',
		owns: ['/setup'],
		panel: { kind: 'setup' },
		group: 'setup',
	},
	{
		id: 'home',
		label: 'Home',
		icon: 'home',
		chord: '⌥⌘H',
		href: '/chat',
		// Chats, projects, applets and pages are all reached from this panel,
		// so the pane that holds one lights this tile: the rail is a lens over
		// where you are, and where you are is "in something Home led you to".
		//
		// `/day` is NOT here any more — see Wiki. A day is a wiki article, its
		// components live in `components/wiki/`, and the panel that lists days
		// is the wiki's. Home owning it meant reading yesterday lit the Home
		// tile while the Wiki panel sat there with the day's own index in it.
		owns: [
			'/',
			'/chat',
			'/chat-history',
			'/project',
			'/projects',
			'/applets',
			'/applet',
			'/page',
			'/pages',
		],
		panel: { kind: 'home' },
		group: 'primary',
	},
	{
		id: 'record',
		label: 'Wiki',
		icon: 'wiki',
		chord: '⌥⌘R',
		href: '/wiki',
		// `/day` and `/year` are wiki articles that happen to live at short
		// routes; `/person`, `/place` and `/org` are the entity articles. All of
		// them are reached from this panel and all of them render a wiki page,
		// so all of them light this tile. Home held `/day` until 2026-09-22,
		// which is why reading a day used to light the wrong room.
		owns: ['/wiki', '/day', '/year', '/person', '/place', '/org'],
		panel: { kind: 'rows', modeId: 'wiki' },
		group: 'library',
	},
	{
		// One day on the box's own map. No panel: the page is the room.
		id: 'timeline',
		label: 'Timeline',
		icon: 'timeline',
		chord: '⌥⌘T',
		href: '/timeline',
		owns: ['/timeline'],
		panel: { kind: 'none' },
		group: 'library',
	},
	{
		// "Drive", not "Files": the room already had the drive glyph and the
		// /storage route, and "Files" named the contents rather than the place.
		// The id stays `files` so a stored rail selection survives the relabel.
		id: 'files',
		label: 'Drive',
		icon: 'drive',
		chord: '⌥⌘F',
		href: '/storage',
		owns: ['/storage', '/bookmarks', '/asset', '/trash'],
		panel: { kind: 'rows', modeId: 'drive' },
		group: 'library',
	},
	{
		id: 'settings',
		label: 'Settings',
		icon: 'settings',
		chord: '⌥⌘,',
		href: '/virtues/you',
		// Sources and the developer consoles are rows in this panel, so their
		// pages light this tile.
		owns: ['/virtues', '/sources'],
		panel: { kind: 'rows', modeId: 'settings' },
		group: 'utility',
	},
];

export const DEFAULT_ROOM_ID = 'home';

export function roomById(id: string | null | undefined): Room | null {
	if (!id) return null;
	return ROOMS.find((r) => r.id === id) ?? null;
}

export function roomsInGroup(group: RoomGroup): Room[] {
	return ROOMS.filter((r) => r.group === group);
}

/**
 * Which room a route belongs to, or null.
 *
 * Segment-aware: `/pages` must not be claimed by a room owning `/page`, and the
 * root must match only itself — a naive `startsWith('/')` would hand every
 * route in the app to Chats.
 */
export function roomForRoute(route: string | null | undefined): Room | null {
	if (!route) return null;
	const path = route.split('?')[0].split('#')[0];

	let best: Room | null = null;
	let bestLen = -1;

	for (const room of ROOMS) {
		for (const own of room.owns) {
			const hit = own === '/' ? path === '/' : path === own || path.startsWith(own + '/');
			if (hit && own.length > bestLen) {
				best = room;
				bestLen = own.length;
			}
		}
	}
	return best;
}
