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

export interface ModeRow {
	id: string;
	label: string;
	icon: string;
	/** Route opened when the row is clicked. */
	href: string;
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
		{ id: 'you', label: 'You', icon: 'ri:user-line', href: '/virtues/you' },
		{
			id: 'assistant',
			label: 'Assistant',
			icon: 'ri:sparkling-line',
			href: '/virtues/assistant',
		},
		// The full gateway catalog as a table — prices, capabilities, retention.
		// Assistant keeps the per-slot pickers (the pinning control); this is
		// the room for comparing ~240 models, which no dropdown can host.
		{ id: 'models', label: 'Models', icon: 'ri:cpu-line', href: '/virtues/models' },
		// Billing and Usage were two rows answering one question. The balance
		// sat on one page and the calls that drew it down on another, so
		// neither could answer "is that number going where I think it is?" —
		// which is the only reason anyone opens either. (Usage was "Telemetry"
		// under Developer before that: it is the owner's own AI spend, not a
		// developer console, and nothing about it is sent anywhere.)
		{ id: 'plan', label: 'Plan', icon: 'ri:bank-card-line', href: '/virtues/plan' },
		// Was one door, "Box", which was a container rather than a subject: it
		// stacked a Wi-Fi picker, an update installer, an 8-chapter telemetry
		// console and a revoke-everything button on one scroll, and two of those
		// duplicated chapters of the console below them. Three subjects, three
		// doors — the machine, its connection, and the code it runs.
		{ id: 'system', label: 'System', icon: 'ri:server-line', href: '/virtues/system' },
		{ id: 'network', label: 'Network', icon: 'ri:wifi-line', href: '/virtues/network' },
		{
			id: 'software',
			label: 'Software',
			// A package, not a download. The point of naming this section
			// "Software" rather than "Updates" was that it stays true when
			// nothing is pending — a download-cloud glyph put the pending state
			// back into the nav permanently, which is the thing the name avoids.
			// (The remix icon is called `box-3`; unrelated to the old Box door.)
			icon: 'ri:box-3-line',
			href: '/virtues/software',
		},
		{ id: 'devices', label: 'Devices', icon: 'ri:device-line', href: '/virtues/devices' },
		// The screen on the box itself. Under Devices but not IN it: the panel
		// is part of the box's body, not a paired thing — it cannot be revoked,
		// only tended.
		{ id: 'display', label: 'Display', icon: 'ri:tv-2-line', href: '/virtues/display' },
	],
};

export const DEVELOPER_MODE: SidebarMode = {
	id: 'developer',
	title: 'Developer',
	rows: [
		{ id: 'sql', label: 'SQL', icon: 'ri:terminal-box-line', href: '/virtues/developer/sql' },
		{
			id: 'terminal',
			label: 'Terminal',
			icon: 'ri:terminal-line',
			href: '/virtues/developer/terminal',
		},
		{ id: 'lake', label: 'Lake', icon: 'ri:database-2-line', href: '/virtues/developer/lake' },
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
 * Ordered as the record reads rather than alphabetically: what it is
 * (Overview), what you wrote about it (Stories, Narrative Identity), when it
 * happened (Days, Years), and who/where/what it involved (People, Places,
 * Orgs). People/Places/Orgs were one "Entities" tab with a filter; at eight
 * rows there is room to name them.
 */
export const WIKI_MODE: SidebarMode = {
	id: 'wiki',
	title: 'Wiki',
	rows: [
		{ id: 'overview', label: 'Overview', icon: 'ri:book-2-line', href: '/wiki' },
		// The shape of the record before you read a word of it — and it needs no
		// articles and no model, which is the point.
		{ id: 'lifeline', label: 'Lifeline', icon: 'ri:pulse-line', href: '/wiki/lifeline' },
		{
			id: 'identity',
			label: 'Narrative Identity',
			icon: 'ri:compass-3-line',
			href: '/wiki/identity',
		},
		{ id: 'days', label: 'Days', icon: 'ri:calendar-line', href: '/wiki/days' },
		{ id: 'years', label: 'Years', icon: 'ri:calendar-2-line', href: '/wiki/years' },
		{ id: 'people', label: 'People', icon: 'ri:user-line', href: '/wiki/people' },
		{ id: 'places', label: 'Places', icon: 'ri:map-pin-line', href: '/wiki/places' },
		{ id: 'orgs', label: 'Orgs', icon: 'ri:building-line', href: '/wiki/orgs' },
		// The review surface. `auto_update` is the consent; this is where you
		// see what that consent produced — without it the record edits its own
		// prose in a room nobody visits.
		{ id: 'history', label: 'History', icon: 'ri:history-line', href: '/wiki/history' },
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
		{ id: 'overview', label: 'Overview', icon: 'ri:dashboard-line', href: '/sources' },
		{ id: 'catalog', label: 'Catalog', icon: 'ri:apps-line', href: '/sources/catalog' },
		{ id: 'activity', label: 'Activity', icon: 'ri:history-line', href: '/sources/activity' },
	],
};

export const SIDEBAR_MODES: Record<string, SidebarMode> = {
	settings: SETTINGS_MODE,
	developer: DEVELOPER_MODE,
	wiki: WIKI_MODE,
	sources: SOURCES_MODE,
};
