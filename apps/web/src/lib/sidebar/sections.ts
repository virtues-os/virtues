/**
 * The Library — the sidebar's system shelf.
 *
 * The rail is two zones with two contracts, named from inside the metaphor
 * the whole shell uses:
 *
 *   - Desk    — what you've taken off the shelf to work on. Today that means
 *               notebooks; the species list grows (a page, a person, a day).
 *               Serif spines, bookcloth dots, the user's own order, uncapped.
 *               Rendered by DeskSection, not from these constants.
 *   - Library — the system's fixed shelf. Stable forever, so muscle memory
 *               can live here. Sans labels, Atlas icons.
 *
 * No Home row: the masthead is a path — `∴ Virtues / …` — and its root is the
 * way home. The earlier wordmark-as-home failed because a bare mark behaving
 * like a button is read nowhere as navigation; a breadcrumb root is read
 * everywhere as navigation. The path is the difference.
 *
 * No inline recents: the desk fills the column with things the user made, and
 * a shelf that reorders itself is how a stable list stops being a place.
 */

export interface SystemSectionItem {
	id: string;
	label: string;
	icon: string;
	href: string;
}

export interface SystemSection {
	id: string;
	name: string;
	icon: string;
	type: 'smart' | 'static' | 'link';
	/** Link sections: direct route */
	href?: string;
	/** Smart sections: namespace for fetching recent items */
	namespace?: string;
	/** Smart sections: max items to show */
	limit?: number;
	/**
	 * The `+` on the row: what this collection creates.
	 */
	quickAdd?: 'chat' | 'page' | 'notebook';
	/** Open on first render. */
	defaultExpanded?: boolean;
	/** Smart sections: "View All" route */
	moreRoute?: string;
	/** Static sections: fixed child items */
	items?: SystemSectionItem[];
}

/** A named cluster of destinations. `label: null` renders no header. */
export interface SectionGroup {
	id: string;
	label: string | null;
	items: SystemSection[];
}

export const HOME_ROUTE = '/home';

const CHATS: SystemSection = {
	id: 'sys_chat',
	name: 'Chats',
	icon: 'atlas:chats',
	type: 'link',
	href: '/chat-history',
	quickAdd: 'chat',
};

const PAGES: SystemSection = {
	id: 'sys_pages',
	name: 'Pages',
	icon: 'atlas:pages',
	type: 'link',
	href: '/page',
	quickAdd: 'page',
};

// Calendar is coming, and was briefly rendered here as an inert "soon" row.
// Removed: the shelf is a set of places you can go, and a row that cannot be
// gone to is furniture. It comes back when its room exists — which is what
// Bookmarks just did.
// Notebooks is a destination like any other. It briefly vanished from the
// shelf because an early Desk fetched notebooks directly — but the Desk holds
// whatever you pinned, which may be no notebooks at all, so the room still
// needs its door.
const NOTEBOOKS: SystemSection = {
	id: 'sys_notebooks',
	name: 'Notebooks',
	icon: 'atlas:notebooks',
	type: 'link',
	href: '/notebooks',
	quickAdd: 'notebook',
};

const BOOKMARKS: SystemSection = {
	id: 'sys_bookmarks',
	name: 'Bookmarks',
	icon: 'atlas:bookmarks',
	type: 'link',
	href: '/bookmarks',
};

const WIKI: SystemSection = {
	id: 'sys_wiki',
	name: 'Wiki',
	icon: 'atlas:wiki',
	type: 'link',
	href: '/wiki',
};

const DRIVE: SystemSection = {
	id: 'sys_drive',
	name: 'Drive',
	icon: 'atlas:drive',
	type: 'link',
	href: '/storage',
};

const ACTIONS: SystemSection = {
	id: 'sys_actions',
	name: 'Applets',
	icon: 'atlas:applets',
	type: 'link',
	href: '/applets',
};

export const SECTION_GROUPS: SectionGroup[] = [
	{
		id: 'grp_library',
		label: 'Library',
		items: [NOTEBOOKS, CHATS, PAGES, BOOKMARKS, WIKI, DRIVE, ACTIONS],
	},
];

/** Map old DB view IDs → new constant IDs (for localStorage migration) */
export const LEGACY_ID_MAP: Record<string, string> = {
	'view_sys_sec_chats': 'sys_chat',
	'view_sys_sec_pages': 'sys_pages',
	'view_sys_sec_wiki': 'sys_wiki',
	'view_sys_sec_files': 'sys_drive',
	'view_sys_sec_data': '',
	'view_sys_sec_developer': '',
	'sys_chats': 'sys_chat',
	'sys_files': 'sys_drive',
	'sys_projects': 'sys_notebooks',
	'sys_things': 'sys_notebooks',
	'sys_spaces': 'sys_notebooks',
	'sys_you': '',
};
