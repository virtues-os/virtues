/**
 * Sidebar sub-navigation modes.
 *
 * A mode swaps the sidebar's contents wholesale: the normal destinations slide
 * out to the left and the mode's own rows slide in. You leave through the path
 * mast — `∴ Virtues / Settings`, root clickable — not a bespoke exit row.
 * Settings, Developer, Wiki, and Sources are the consumers.
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
 * Settings. Developer used to be a section in here with its own second row of
 * underline tabs — two stacked underline rows being the smell that said the nav
 * had outgrown its container. It's now its own mode (below).
 */
export const SETTINGS_MODE: SidebarMode = {
	id: 'settings',
	title: 'Settings',
	rows: [
		// "Profile", not "You". `/virtues/you` renders ProfileView — a name, an
		// avatar, an account. The wiki has the other "You", the narrative
		// identity, and two rows with one label in two rooms is a question the
		// user has to answer by clicking both.
		{ id: 'you', label: 'Profile', icon: 'ri:user-line', glyph: 'profile', href: '/virtues/you' },
		{
			id: 'assistant',
			label: 'Assistant',
			icon: 'ri:sparkling-line',
			glyph: 'assistant',
			href: '/virtues/assistant',
		},
		// No Models row. The catalog table lives at the bottom of Assistant,
		// under the slot pickers it serves (2026-09-25). As its own room it was
		// a second place holding the same setting, with its own words for it.
		// `/virtues/models` still opens Assistant, for links already out there.
		// Billing and Usage were two rows answering one question. The balance
		// sat on one page and the calls that drew it down on another, so
		// neither could answer "is that number going where I think it is?" —
		// which is the only reason anyone opens either. (Usage was "Telemetry"
		// under Developer before that: it is the owner's own AI spend, not a
		// developer console, and nothing about it is sent anywhere.)
		//
		// The merged room was called "Plan" for three days, which named a
		// choice that does not exist: there is one subscription at one price,
		// so "Plan" invited a reader to go looking for the other plans. What
		// the room actually holds is what AI costs and how it gets paid for —
		// standing, balance, the endpoint you can route around us to, the
		// Stripe door, and the call log. That is billing.
		{ id: 'billing', label: 'Billing', icon: 'ri:bank-card-line', glyph: 'billing', href: '/virtues/billing' },
		// Was one door, "Box", which was a container rather than a subject: it
		// stacked a Wi-Fi picker, an update installer, an 8-chapter telemetry
		// console and a revoke-everything button on one scroll, and two of those
		// duplicated chapters of the console below them. Three subjects, three
		// doors — the machine, its connection, and the code it runs.
		// System owns the machine itself and everything physically attached to
		// it: its readings, the network it is on, the screen bolted to it.
		// Network and Display were their own rows and are now pages UNDER this
		// one (/virtues/system/network, /virtues/system/display) — one row in
		// the sidebar, still a page each, because appending four chapters of
		// screen settings to eight of telemetry makes a scroll nobody reads.
		{ id: 'system', label: 'System', icon: 'ri:server-line', glyph: 'system', href: '/virtues/system' },
		// Devices owns every participant, and the SERVER is the first of them.
		// "Software" was its own row describing the release the server runs —
		// which is a fact about a device, on a page that could not show you the
		// device. Splitting them is what let a collector claim 1.0.0 next to an
		// app claiming 1.0.25 with neither screen able to say which was wrong.
		{ id: 'devices', label: 'Devices', icon: 'ri:device-line', glyph: 'devices', href: '/virtues/devices' },
		// The screen on the server itself. Its own room, not a page under System:
		// it is four chapters about a physical panel — what it shows, its hours,
		// other screens — which is a subject someone comes to deliberately, not a
		// reading they glance at while checking temperatures.
		{ id: 'display', label: 'Display', icon: 'ri:tv-2-line', glyph: 'display', href: '/virtues/display' },
		// SQL, Terminal and Lake are NOT here. They lived in this list for a
		// while, on the argument that they already sit under `/virtues/*` —
		// true of the route, and beside the point for the nav: they are tools,
		// not preferences, and putting them here forced Settings' in-page nav to
		// grow a second row of tabs to hold their sub-sections. They have their
		// own rail door again (DEVELOPER_MODE below, `developer` in rooms.ts).
	],
};

/**
 * Developer. A rail door of its own, beside Sources and Settings at the foot.
 *
 * This constant sat unreferenced for a while, after the three rows were folded
 * into Settings — it is live again rather than rewritten, because the fold
 * changed nothing about what belongs in here.
 */
export const DEVELOPER_MODE: SidebarMode = {
	id: 'developer',
	title: 'Developer',
	rows: [
		{ id: 'sql', label: 'SQL', icon: 'ri:terminal-box-line', glyph: 'sql', href: '/virtues/developer/sql' },
		{
			id: 'terminal',
			label: 'Terminal',
			icon: 'ri:terminal-line',
			glyph: 'terminal',
			href: '/virtues/developer/terminal',
		},
		{ id: 'lake', label: 'Lake', icon: 'ri:database-2-line', glyph: 'lake', href: '/virtues/developer/lake' },
		// Telemetry moved out and became Settings → Usage. Activity — the
		// auth-audit log — is gone; what it reported on (what is paired, what
		// you can revoke) is Devices' job, and it had a second reading of the
		// word "activity" that already meant something else in Sources.
	],
};

/**
 * Wiki. The third mode, and the first one that isn't a settings surface — the
 * wiki outgrew a row of underline tabs the same way Developer did.
 *
 * TWO AXES, NAMED. Eleven rows in one flat list was the rail's own problem
 * moved a level down: the rail was cut from ten tiles to six because "a rail of
 * ten indistinguishable glyphs is a memorisation tax rather than a contents
 * page" (see `rooms.ts`), and the wiki panel then grew to eleven. A list that
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
			icon: 'ri:sun-line',
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
		{ id: 'days', label: 'Days', icon: 'ri:calendar-line', glyph: 'calendar', href: '/wiki/days', group: 'Time' },
		{
			id: 'years',
			label: 'Years',
			icon: 'ri:calendar-2-line',
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
			icon: 'ri:contacts-book-2-line',
			glyph: 'chapters',
			href: '/wiki/chapters',
			group: 'Time',
		},
		// The shape of the record before you read a word of it — and it needs no
		// articles and no model, which is the point. Last in Time because it is
		// the widest lens, not the first thing you reach for.
		{
			id: 'lifeline',
			label: 'Lifeline',
			icon: 'ri:pulse-line',
			glyph: 'lifeline',
			href: '/wiki/lifeline',
			group: 'Time',
		},
		{
			id: 'identity',
			// "You", not "Narrative Identity": that is the name of the artifact,
			// not the name of the subject, and the room is the owner's own page.
			label: 'You',
			icon: 'ri:user-star-line',
			glyph: 'identity',
			href: '/wiki/identity',
			group: 'Subjects',
		},
		{
			id: 'people',
			label: 'People',
			icon: 'ri:user-line',
			glyph: 'people',
			href: '/wiki/people',
			group: 'Subjects',
		},
		{
			id: 'places',
			label: 'Places',
			icon: 'ri:map-pin-line',
			glyph: 'places',
			href: '/wiki/places',
			group: 'Subjects',
		},
		{
			// "Organizations", spelled out. The panel has the width, and "Orgs" was
			// the only abbreviation in a product that writes in sentences.
			id: 'orgs',
			label: 'Organizations',
			icon: 'ri:building-line',
			glyph: 'organizations',
			href: '/wiki/orgs',
			group: 'Subjects',
		},
		// A subject, not a span — which is exactly what `api/stories.rs` says
		// separates a story from a chapter: "Piano & Composition" is a thing the
		// record is about, "the Berlin years" is a stretch of it. Both are named
		// by the person rather than derived, which is why they used to sit
		// together; the two axes split them, and the split is the right one.
		{
			id: 'stories',
			label: 'Stories',
			icon: 'ri:book-2-line',
			glyph: 'stories',
			href: '/wiki/stories',
			group: 'Subjects',
		},
	],
};

/**
 * Sources. Its own door rather than a row inside Settings, where it had been
 * one line between Assistant and Billing — a hard place to find for the room
 * that decides whether the record has anything in it at all.
 *
 * Not on the Library shelf either: the shelf holds what you author and read
 * (chats, pages, notebooks, the wiki), and this is the plumbing under it. Same
 * argument that put Developer in the footer.
 *
 * Three rows, in the order the questions get asked: is anything broken right
 * now (Overview), what can I plug in and what is already plugged in (Catalog),
 * and what has actually been running (Activity).
 */
export const SOURCES_MODE: SidebarMode = {
	id: 'sources',
	title: 'Sources',
	rows: [
		{ id: 'overview', label: 'Overview', icon: 'ri:dashboard-line', glyph: 'overview', href: '/sources' },
		{ id: 'catalog', label: 'Catalog', icon: 'ri:apps-line', glyph: 'catalog', href: '/sources/catalog' },
		{ id: 'activity', label: 'Activity', icon: 'ri:history-line', glyph: 'history', href: '/sources/activity' },
	],
};

/**
 * Drive's panel: the Storage room's sections as rows, plus Bookmarks, which
 * folded into Drive when the rail went to six rooms. Trash is last because it
 * is the one section you walk to on purpose — everything deleted anywhere in
 * the app (chats, pages, projects, files) waits there 30 days.
 */
export const DRIVE_MODE: SidebarMode = {
	id: 'drive',
	title: 'Drive',
	rows: [
		{ id: 'files', label: 'Files', icon: 'ri:hard-drive-2-line', glyph: 'files', href: '/storage' },
		{ id: 'streams', label: 'Streams', icon: 'ri:database-2-line', glyph: 'streams', href: '/storage/streams' },
		{ id: 'media', label: 'App Media', icon: 'ri:image-2-line', glyph: 'media', href: '/storage/media' },
		{ id: 'bookmarks', label: 'Bookmarks', icon: 'ri:bookmark-line', glyph: 'bookmarks', href: '/bookmarks' },
		{ id: 'trash', label: 'Recently deleted', icon: 'ri:delete-bin-line', glyph: 'trash', href: '/storage/trash' },
	],
};

export const SIDEBAR_MODES: Record<string, SidebarMode> = {
	drive: DRIVE_MODE,
	settings: SETTINGS_MODE,
	developer: DEVELOPER_MODE,
	wiki: WIKI_MODE,
	sources: SOURCES_MODE,
};
