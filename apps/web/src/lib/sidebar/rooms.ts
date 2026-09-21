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
 *     contents. Its page is `/home`, the Daily Office, so the tile leads
 *     somewhere real.
 *   - Applets left the rail the same day. An applet is something you run from
 *     a conversation, so its door is a row at the top of the Home panel,
 *     beside New chat — a rail tile for a list that is opened from chat was
 *     a room nobody walked to.
 *   - Pages left the rail the same day too. A page is written the way a chat
 *     is started, so Pages is a door in the Home panel with a `+` beside it,
 *     the same shape Projects has.
 *
 * Developer was folded into Settings too, and came back out — see its entry.
 * The merges above survive because each pair answers the same question;
 * that one did not, because "what preference is this?" and "run a query" are
 * not the same question.
 *
 * ROUTES ARE UNCHANGED. Only labels and grouping move, so the layout can be
 * judged without also judging a migration.
 *
 * Home sits alone above the first gap: it is the ground rather than a peer, so
 * it gets primacy, not parity, and it is the top of the rail, which is what
 * "the ground" should have meant all along.
 */

export type RoomGroup = 'primary' | 'library' | 'utility';

/** What fills the panel body under the room's title. */
export type RoomPanel =
	/** The fixed rows of a `SIDEBAR_MODES` entry. */
	| { kind: 'rows'; modeId: string }
	/** The Home panel: doors, Pinned, Projects, the recent chats. */
	| { kind: 'home' }
	/** Nothing live yet — the panel offers the room's full page. */
	| { kind: 'stub' };

export interface Room {
	id: string;
	/** Leads on the rail. Icons assist; labels lead. */
	label: string;
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
	 * so `/virtues/developer` resolves to Developer (which owns exactly that)
	 * rather than to Settings' shorter `/virtues`. Order in this array — and in
	 * ROOMS — is therefore not load-bearing; specificity is.
	 */
	owns: string[];
	panel: RoomPanel;
	group: RoomGroup;
}

export const ROOMS: Room[] = [
	{
		id: 'home',
		label: 'Home',
		icon: 'home',
		chord: '⌥⌘H',
		href: '/home',
		// Chats, projects, applets and pages are all reached from this panel,
		// so the pane that holds one lights this tile: the rail is a lens over
		// where you are, and where you are is "in something Home led you to".
		owns: [
			'/',
			'/home',
			'/day',
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
		owns: ['/wiki'],
		panel: { kind: 'rows', modeId: 'wiki' },
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
		owns: ['/storage', '/bookmarks', '/asset'],
		panel: { kind: 'stub' },
		group: 'library',
	},
	{
		// Between Sources and Settings, not after them. Settings is the room
		// muscle memory reaches for at the very foot of a rail — every desktop
		// app it borrows from puts it there — so the new door takes the middle
		// slot rather than pushing Settings off the anchor.
		id: 'sources',
		label: 'Sources',
		icon: 'sources',
		chord: '⌥⌘O',
		href: '/sources',
		owns: ['/sources'],
		panel: { kind: 'rows', modeId: 'sources' },
		group: 'utility',
	},
	{
		// Its own door again, after a spell folded into Settings as three rows.
		// The fold was argued as "three rooms for the handful of people who open
		// a SQL console is not worth a rail slot" — but the slot is not what it
		// cost. Buried, the consoles were four clicks deep behind a noun they do
		// not belong to (a SQL terminal is not a preference), and Settings' own
		// nav had to grow a SECOND row of tabs to hold them, which is the exact
		// smell that split Developer out the first time.
		//
		// `owns` is LONGER than Settings' `/virtues`, and `roomForRoute` takes
		// the longest match — that is what keeps `/virtues/developer/sql`
		// lighting this room rather than Settings.
		id: 'developer',
		label: 'Developer',
		icon: 'developer',
		chord: '⌥⌘D',
		href: '/virtues/developer/sql',
		owns: ['/virtues/developer'],
		panel: { kind: 'rows', modeId: 'developer' },
		group: 'utility',
	},
	{
		id: 'settings',
		label: 'Settings',
		icon: 'settings',
		chord: '⌥⌘,',
		href: '/virtues/you',
		owns: ['/virtues'],
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
