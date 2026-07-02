/**
 * System Sidebar Sections
 *
 * Single source of truth for the sidebar's system navigation.
 * These are rendered directly from constants — no database backing.
 * Only user-created folders and items live in the DB.
 *
 * The sidebar is a "contents page", not a mode-switching rail: a stable
 * directory of top-level destinations. Deep navigation lives inside each
 * room (and is curated via Pins / Spaces), never in this panel.
 *
 * Destinations split along the two axes of the LifeOS, separated by a blank gap
 * (no text headers):
 *   - Reflect (time / self) — Today (the day) and You (the self-model).
 *   - Work    (space)       — Spaces, Pages, Wiki, Drive, Actions, Chats.
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

const YOU: SystemSection = {
	id: 'sys_you',
	name: 'Narrative',
	icon: 'ri:quill-pen-line',
	type: 'link',
	href: '/narrative-identity',
};

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
	href: '/entities',
};

const SPACES: SystemSection = {
	id: 'sys_spaces',
	name: 'Spaces',
	icon: 'ri:box-3-line',
	type: 'link',
	href: '/spaces',
};

const DRIVE: SystemSection = {
	id: 'sys_drive',
	name: 'Drive',
	icon: 'ri:cloud-line',
	type: 'link',
	href: '/drive',
};

const ACTIONS: SystemSection = {
	id: 'sys_actions',
	name: 'Actions',
	icon: 'ri:flashlight-line',
	type: 'link',
	href: '/actions',
};

export const SECTION_GROUPS: SectionGroup[] = [
	// Reflect (time / self) — then a blank gap — then Work (space / domains).
	{ id: 'grp_reflect', label: null, items: [HOME, TODAY, YOU] },
	{ id: 'grp_work', label: null, items: [CHATS, PAGES, SPACES, WIKI, DRIVE, ACTIONS] },
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
	'sys_projects': 'sys_spaces',
	'sys_things': 'sys_spaces',
};
