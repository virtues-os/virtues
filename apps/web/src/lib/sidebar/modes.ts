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

export const SIDEBAR_MODES: Record<string, SidebarMode> = {
	settings: SETTINGS_MODE,
	developer: DEVELOPER_MODE,
};
