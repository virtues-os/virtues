/**
 * System Sidebar Sections
 *
 * Single source of truth for the sidebar's system navigation.
 * These are rendered directly from constants — no database backing.
 * Only user-created folders and items live in the DB.
 *
 * The sidebar is a "contents page", not a mode-switching rail: a stable
 * directory of top-level destinations. Deep navigation lives inside each
 * room (and is curated via Pins / Notebooks), never in this panel.
 *
 * Destinations split into three gap-separated clusters (no text headers):
 *   - Rhythm    — Home, Today.
 *   - Create    — Chats, Pages, Notebooks (where you work).
 *   - Substrate — Wiki (the life-graph: entities + time + narrative), Drive,
 *                 Actions (the layers underneath).
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
	/** Smart sections: quick-add action type */
	quickAdd?: 'chat' | 'page';
	/** Smart sections: "View All" route */
	moreRoute?: string;
	/** Static sections: fixed child items */
	items?: SystemSectionItem[];
}

/** A named cluster of destinations. `label: null` renders no header (the "now" zone). */
export interface SectionGroup {
	id: string;
	label: string | null;
	items: SystemSection[];
}

const HOME: SystemSection = {
	id: 'sys_landing',
	name: 'Home',
	icon: 'ri:home-5-line',
	type: 'link',
	href: '/home',
};

const TODAY: SystemSection = {
	id: 'sys_home',
	name: 'Today',
	icon: 'ri:sun-line',
	type: 'link',
	href: '/day',
};

// Narrative folded into Wiki (the life-graph's throughline). Still reachable at
// /narrative-identity via Wiki + search; no longer a top-level sidebar entry.

const CHATS: SystemSection = {
	id: 'sys_chat',
	name: 'Chats',
	icon: 'ri:chat-1-line',
	type: 'link',
	href: '/chat-history',
};

const PAGES: SystemSection = {
	id: 'sys_pages',
	name: 'Pages',
	icon: 'ri:file-text-line',
	type: 'link',
	href: '/page',
	quickAdd: 'page',
};

const WIKI: SystemSection = {
	id: 'sys_wiki',
	name: 'Wiki',
	icon: 'ri:book-open-line',
	type: 'link',
	href: '/wiki',
};

const NOTEBOOKS: SystemSection = {
	id: 'sys_notebooks',
	name: 'Notebooks',
	icon: 'ri:booklet-line',
	type: 'link',
	href: '/notebooks',
};

// STORIES — deferred. `wiki_stories` and its magnet are built (0038/0039) but
// the rendering isn't, so there is no sidebar link yet (the /stories route
// still resolves for deep links). A notebook is a CONTAINER; a story is a
// CLAIM ("I got fat", "I act differently on rainy days") that gathers evidence
// across years — peers, not variants. It returns to the Create cluster once the
// layer beneath it (events, entities, days) is verified.

const DRIVE: SystemSection = {
	id: 'sys_drive',
	name: 'Drive',
	icon: 'ri:cloud-line',
	type: 'link',
	href: '/storage',
};

const ACTIONS: SystemSection = {
	id: 'sys_actions',
	name: 'Applets',
	icon: 'ri:flashlight-line',
	type: 'link',
	href: '/actions',
};

export const SECTION_GROUPS: SectionGroup[] = [
	// Rhythm — Create — Substrate, each separated by a blank gap (no headers).
	{ id: 'grp_rhythm', label: null, items: [HOME, TODAY] },
	{ id: 'grp_create', label: null, items: [CHATS, PAGES, NOTEBOOKS] },
	{ id: 'grp_substrate', label: null, items: [WIKI, DRIVE, ACTIONS] },
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
