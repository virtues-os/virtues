/**
 * Sidebar sub-navigation modes.
 *
 * A mode swaps the sidebar's contents wholesale: the normal destinations slide
 * out to the left, the mode's own rows slide in, and you leave by an explicit
 * exit row at the top. Settings and Developer are the two consumers.
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
	/** Shown next to the back arrow in the exit row. */
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
		{ id: 'sources', label: 'Sources', icon: 'ri:database-2-line', href: '/virtues/sources' },
		{ id: 'billing', label: 'Billing', icon: 'ri:bank-card-line', href: '/virtues/billing' },
		{ id: 'box', label: 'Box', icon: 'ri:server-line', href: '/virtues/box' },
		{ id: 'devices', label: 'Devices', icon: 'ri:device-line', href: '/virtues/devices' },
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
		{
			id: 'telemetry',
			label: 'Telemetry',
			icon: 'ri:pulse-line',
			href: '/virtues/developer/telemetry',
		},
		{
			id: 'activity',
			label: 'Activity',
			icon: 'ri:history-line',
			href: '/virtues/developer/activity',
		},
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

export const SIDEBAR_MODES: Record<string, SidebarMode> = {
	settings: SETTINGS_MODE,
	developer: DEVELOPER_MODE,
	wiki: WIKI_MODE,
};
