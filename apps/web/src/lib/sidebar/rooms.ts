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
 * Ten top-level destinations became eight, because a rail of ten
 * indistinguishable glyphs is a memorisation tax rather than a contents page:
 *
 *   - Notebooks folded into Pages. A notebook is a scoping device over pages,
 *     and nobody in week one could say what distinguished the two rooms.
 *   - Bookmarks folded into Files. Both are "something from outside that you
 *     kept" — the loosest of the merges, and the first to revisit.
 *
 * Developer was folded into Settings too, and came back out — see its entry.
 * The two merges above survive because each pair answers the same question;
 * that one did not, because "what preference is this?" and "run a query" are
 * not the same question.
 *
 * ROUTES ARE UNCHANGED. Only labels and grouping move, so the layout can be
 * judged without also judging a migration.
 *
 * Chats sits alone above the first gap: it is the ground rather than a peer, so
 * it gets primacy, not parity.
 */

export type RoomGroup = 'primary' | 'library' | 'utility';

/** What fills the panel body under the room's title. */
export type RoomPanel =
	/** The fixed rows of a `SIDEBAR_MODES` entry. */
	| { kind: 'rows'; modeId: string }
	/** The live conversation list. */
	| { kind: 'chats' }
	/** The Desk: what the user pinned, in their own order. */
	| { kind: 'desk' }
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
	/** The `+` in the panel's title row. */
	quickAdd?: 'chat' | 'page';
}

export const ROOMS: Room[] = [
	{
		// The Desk comes back here, and the rail is what makes that possible.
		// design.md retired pins because "a pinned 'Pages' and a nav 'Pages'
		// render identically", and asked that any return be "a different SHAPE,
		// not a tinted one". A 72px column of icons beside a 208px column of
		// serif spines cannot render identically; the shape is the rail itself.
		id: 'home',
		label: 'Home',
		icon: 'home',
		chord: '⌥⌘H',
		href: '/home',
		owns: ['/home', '/day'],
		panel: { kind: 'desk' },
		group: 'primary',
	},
	{
		id: 'chats',
		label: 'Chats',
		icon: 'chats',
		chord: '⌥⌘C',
		href: '/chat-history',
		owns: ['/', '/chat', '/chat-history'],
		panel: { kind: 'chats' },
		group: 'primary',
		quickAdd: 'chat',
	},
	{
		id: 'pages',
		label: 'Pages',
		icon: 'pages',
		chord: '⌥⌘P',
		href: '/page',
		owns: ['/page', '/pages', '/notebook', '/notebooks'],
		panel: { kind: 'stub' },
		group: 'library',
		quickAdd: 'page',
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
		id: 'files',
		label: 'Files',
		icon: 'drive',
		chord: '⌥⌘F',
		href: '/storage',
		owns: ['/storage', '/bookmarks', '/asset'],
		panel: { kind: 'stub' },
		group: 'library',
	},
	{
		id: 'routines',
		label: 'Applets',
		icon: 'applets',
		chord: '⌥⌘U',
		href: '/applets',
		owns: ['/applets'],
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

export const DEFAULT_ROOM_ID = 'chats';

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
