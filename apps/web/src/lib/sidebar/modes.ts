/**
 * Sidebar sub-navigation modes.
 *
 * A mode swaps the sidebar's contents wholesale: the normal destinations slide
 * out to the left and the mode's own rows slide in. You leave through the path
 * mast — `∴ Virtues / Settings`, root clickable — not a bespoke exit row.
 * Home's panel is its own; Settings and Wiki are the consumers.
 *
 * Deliberately a *sidebar* state and not derived from which tab has focus.
 * Deriving it would be incoherent with split panes — settings in the left pane
 * and a chat in the right would leave the sidebar with no correct answer.
 * Entered and left on purpose, it's well-defined no matter what the panes show.
 */

import { getLocalDateSlug } from '$lib/utils/dateUtils';

export interface ModeRow {
	id: string;
	label: string;
	/**
	 * A Remix name. NOT rendered by the mode panel — kept because the rows are
	 * also the one written list of what each room contains, and a few other
	 * surfaces read it.
	 */
	icon: string;
	/**
	 * An `AtlasIcon` glyph — the shell's own set, the one the rail is drawn in.
	 * The panel renders THIS and never `icon`: a column of Remix glyphs hanging
	 * off an Atlas rail tile is two icon languages in one sidebar, which is how
	 * the panel ended up with no glyphs at all.
	 *
	 * Omitted means no glyph, and the label carries the row alone. Every mode
	 * has its glyphs as of 2026-09-25. A new row needs its glyph drawn into
	 * `AtlasIcon` first; until then it renders as a label alone, which is
	 * better than naming a glyph that does not exist and getting `pages`.
	 */
	glyph?: string;
	/** Route opened when the row is clicked. */
	href: string;
	/**
	 * Overrides the active mark's default rule, which is "the route is `href`
	 * or sits under it". Needed by exactly one kind of row: a door whose href
	 * RESOLVES to somewhere else. `/day` lands on today, and under the default
	 * rule the row then stayed lit on every other day of your life — a row
	 * labelled Today, highlighted, while you read September 2nd.
	 */
	activeWhen?: (route: string) => boolean;
	/**
	 * Heading this row sits under. Rows carrying the same group must be
	 * ADJACENT — the panel emits a heading wherever the group changes, so a
	 * group interrupted and resumed prints its heading twice.
	 *
	 * Omitted means no heading: the row stands apart at the foot, after a rule.
	 * A mode whose rows all omit it renders exactly as it did before groups
	 * existed, which is every mode but the wiki.
	 */
	group?: string;
}

export interface SidebarMode {
	id: string;
	/** The path mast's tail while this mode is open: `∴ Virtues / <title>`. */
	title: string;
	rows: ModeRow[];
}

/**
 * Settings' pages under the landing page's three headings (LANDING in
 * lib/settings), for the crumbs on those pages: You, Your data, Your server.
 */
// TODO(2026-11-17): migrate the crumbs off these rows. Settings' pages are
// listed in lib/settings (its landing links to each) and Sources is its own
// room with tabs; these rows stay only because tabs/crumbs.ts builds
// Settings' crumbs from them.
export const SETTINGS_MODE: SidebarMode = {
	id: 'settings',
	title: 'Settings',
	rows: [
		// "Profile", not "You": the wiki has the other "You", the narrative
		// identity, and two rows with one label in two rooms is a question the
		// user has to answer by clicking both.
		{ id: 'you', label: 'Profile', icon: 'mynaui:user', glyph: 'profile', href: '/virtues/profile', group: 'You' },
		{ id: 'assistant', label: 'Assistant', icon: 'mynaui:sparkles', glyph: 'assistant', href: '/virtues/assistant', group: 'You' },
		{ id: 'appearance', label: 'Appearance', icon: 'mynaui:sun', href: '/virtues/appearance', group: 'You' },
		// Subscription, balance and limits; what AI has cost is its sub-page.
		{ id: 'billing', label: 'Billing', icon: 'mynaui:credit-card', glyph: 'billing', href: '/virtues/billing', group: 'You' },
		// The named keys applets use. Values show only when asked for.
		{ id: 'keys', label: 'Keys', icon: 'mynaui:key', glyph: 'keys', href: '/virtues/keys', group: 'Your data' },
		// Every link the owner has shared, live or ended.
		{ id: 'shared', label: 'Shared links', icon: 'mynaui:share', glyph: 'shared', href: '/virtues/shared', group: 'Your data' },
		{ id: 'backup', label: 'Backup', icon: 'mynaui:archive', href: '/virtues/backup', group: 'Your data' },
		{ id: 'server', label: 'Updates and network', icon: 'mynaui:refresh', href: '/virtues/server', group: 'Your server' },
		// The machine itself, measured.
		{ id: 'system', label: "How it's doing", icon: 'mynaui:servers', glyph: 'system', href: '/virtues/system', group: 'Your server' },
		// Where search runs (NPU, CPU, or the owner's server) and what it covers.
		{ id: 'search', label: 'Search', icon: 'mynaui:search', glyph: 'search', href: '/virtues/search', group: 'Your server' },
		// Every participant, the server first: the release it runs is a fact
		// about a device.
		{ id: 'devices', label: 'Devices', icon: 'mynaui:mobile', glyph: 'devices', href: '/virtues/devices', group: 'Your server' },
		// Every applet on the server, built-in ones included: when each runs and
		// how its runs went. The Applets page in Home is where you use them;
		// this is where you check them.
		{ id: 'applets', label: 'Applets', icon: 'atlas:applets', glyph: 'applets', href: '/virtues/applets', group: 'Your server' },
	],
};

/**
 * Wiki. The first mode that isn't a settings surface — the wiki outgrew a row
 * of underline tabs the way Settings had.
 *
 * TWO AXES, NAMED. Eleven rows in one flat list was the rail's own problem
 * moved a level down: the rail was cut from ten tiles to six because "a rail of
 * ten indistinguishable glyphs is a memorisation tax rather than a contents
 * page", and the wiki's list then grew to eleven. A list that
 * long is not read, it is scanned for the word you already wanted.
 *
 * The record only ever answers two questions, so the panel asks them in the
 * user's order:
 *
 *   TIME — when did it happen. A day, a year, an era you named, the whole line.
 *   SUBJECTS — who and what it involved. You first, because the narrative
 *     identity is a subject of this wiki like any other, and because the
 *     owner is the subject every other one is oriented around.
 *
 * DRIVE sits under them: the files, media, bookmarks and passwords you
 * kept, and Recently deleted. It had its own rail door until 2026-10-21.
 * Recently deleted is last because it is the one section you walk to on
 * purpose: everything deleted anywhere in the app (chats, pages, projects,
 * files) waits there 30 days.
 *
 * WHAT ALSO LEFT: "History", the log of every edit the record made to its own
 * prose (`/wiki/history`, still there, still linked from an article). It sat
 * alone at the foot under a rule and read as mystery meat — it is neither a
 * time nor a subject, so neither heading claimed it, and on a box where the
 * editor has never run it opens on nothing. It earns a row back when the
 * editor is switched on and there is something in it: the bargain for granting
 * maintenance is that every resulting edit is visible, and that bargain needs
 * a door only once edits exist.
 *
 * WHAT LEFT: "Overview". Its href was `/wiki`, which is also the Wiki tile's
 * own destination on the rail — the tile and the first row of its panel were
 * the same click. The page is untouched; only the second door to it is gone.
 *
 * WHAT ARRIVED: "Today", the day page. The day is the most-read page in the
 * product and the panel had no row for it — only "Days", the index. The index
 * is still there, one row down, for the day that is not today.
 *
 * People/Places/Organizations stay three rows even though the content folds
 * them into one entities index (the legacy segment presets its type filter —
 * see `LEGACY_TYPE` in WikiView). Under a heading that names what they have in
 * common, three doors to one filtered room reads as a choice rather than as
 * three rooms that turn out to be one.
 */
export const WIKI_MODE: SidebarMode = {
	id: 'wiki',
	title: 'Wiki',
	rows: [
		{
			id: 'today',
			label: 'Today',
			icon: 'mynaui:sun',
			glyph: 'day',
			// Not `/day/<date>`: `/day` resolves to the current day on its own,
			// and a date baked in here would be stale by morning.
			href: '/day',
			// Which is also why the mark needs its own rule — the route you end
			// up on is dated, and only TODAY's date is this row.
			activeWhen: (route) =>
				route === '/day' || route === `/day/day_${getLocalDateSlug(new Date())}`,
			group: 'Time',
		},
		// The day on the box's own map. It was a rail room of its own; with the
		// rail gone it is the Time group's live view, beside the days it draws.
		{ id: 'timeline', label: 'Timeline', icon: 'mynaui:path', glyph: 'timeline', href: '/timeline', group: 'Time' },
		{ id: 'days', label: 'Days', icon: 'mynaui:calendar', glyph: 'calendar', href: '/wiki/days', group: 'Time' },
		{
			id: 'years',
			label: 'Years',
			icon: 'mynaui:calendar',
			glyph: 'years',
			href: '/wiki/years',
			group: 'Time',
		},
		// The life's own partition — authored in the interview, never inferred.
		// Its own row: wiki_chapters is structure, not part of the identity
		// document.
		{
			id: 'chapters',
			label: 'Chapters',
			icon: 'mynaui:book-user',
			glyph: 'chapters',
			href: '/wiki/chapters',
			group: 'Time',
		},
		{
			id: 'identity',
			// "You", not "Narrative Identity": that is the name of the artifact,
			// not the name of the subject, and the room is the owner's own page.
			label: 'You',
			icon: 'mynaui:user-circle',
			glyph: 'identity',
			href: '/wiki/identity',
			group: 'Subjects',
		},
		{
			id: 'people',
			label: 'People',
			icon: 'mynaui:user',
			glyph: 'people',
			href: '/wiki/people',
			group: 'Subjects',
		},
		{
			id: 'places',
			label: 'Places',
			icon: 'mynaui:map-pin',
			glyph: 'places',
			href: '/wiki/places',
			group: 'Subjects',
		},
		{
			// "Organizations", spelled out. The panel has the width, and "Orgs" was
			// the only abbreviation in a product that writes in sentences.
			id: 'orgs',
			label: 'Organizations',
			icon: 'mynaui:building',
			glyph: 'organizations',
			href: '/wiki/orgs',
			group: 'Subjects',
		},
		// What goes on in a life and the person named: "Sailing", "The garden".
		// A subject like a person or a place, with one difference: nothing in
		// the record resolves to it, so only the person adds one.
		{
			id: 'things',
			label: 'Things',
			icon: 'mynaui:box',
			glyph: 'things',
			href: '/wiki/things',
			group: 'Subjects',
		},
	],
};

/**
 * Storage: what your server keeps that you file or save, apart from the
 * wiki's story of your life. Its door opens the front page at `/storage`,
 * which links to these rows.
 */
export const STORAGE_MODE: SidebarMode = {
	id: 'storage',
	title: 'Storage',
	rows: [
		{
			id: 'files',
			label: 'Files',
			icon: 'mynaui:hard-drive',
			glyph: 'files',
			href: '/storage/drive',
		},
		{
			id: 'bookmarks',
			label: 'Bookmarks',
			icon: 'mynaui:bookmark',
			glyph: 'bookmarks',
			href: '/bookmarks',
			activeWhen: (route) => route === '/bookmarks' || route.startsWith('/bookmark/'),
		},
		{ id: 'media', label: 'App Media', icon: 'mynaui:image', glyph: 'media', href: '/storage/media' },
		// Saved logins. Kept on the box and never read by the assistant
		// (api/passwords.rs); API keys stay in Settings → Keys.
		{
			id: 'passwords',
			label: 'Passwords',
			icon: 'mynaui:lock-password',
			glyph: 'passwords',
			href: '/passwords',
		},
		{
			id: 'trash',
			label: 'Recently deleted',
			icon: 'mynaui:trash',
			glyph: 'trash',
			href: '/storage/trash',
		},
	],
};

/**
 * The row a route belongs to, or null: a row's own rule if it has one, else
 * its href and everything under it. Longest match wins, so
 * `/virtues/developer/terminal` is Terminal, not a shorter prefix.
 */
export function rowForRoute(mode: SidebarMode, route: string | null | undefined): ModeRow | null {
	if (!route) return null;
	const path = route.split('?')[0];
	let best: ModeRow | null = null;
	for (const row of mode.rows) {
		const hit = row.activeWhen ? row.activeWhen(path) : path === row.href || path.startsWith(row.href + '/');
		if (hit && (!best || row.href.length > best.href.length)) best = row;
	}
	return best;
}
