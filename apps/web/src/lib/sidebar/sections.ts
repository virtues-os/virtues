/**
 * System Sidebar Sections
 *
 * Single source of truth for the sidebar's system sections.
 * These are rendered directly from constants — no database backing.
 * Only user-created folders and items live in the DB.
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
	sortOrder: number;
	/** Link sections: direct route */
	href?: string;
	/** Visual group break — adds extra spacing above this item */
	groupBreak?: boolean;
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

export const SYSTEM_SECTIONS: SystemSection[] = [
	{
		id: 'sys_home',
		name: 'Today',
		icon: 'ri:sun-line',
		type: 'link',
		sortOrder: 50,
		href: '/day',
	},
	{
		id: 'sys_chat',
		name: 'Chat',
		icon: 'ri:chat-1-line',
		type: 'link',
		sortOrder: 100,
		href: '/chat-history',
		quickAdd: 'chat',
	},
	{
		id: 'sys_pages',
		name: 'Pages',
		icon: 'ri:file-text-line',
		type: 'link',
		sortOrder: 200,
		href: '/page',
		groupBreak: true,
		quickAdd: 'page',
	},
	{
		id: 'sys_wiki',
		name: 'Wiki',
		icon: 'ri:book-open-line',
		type: 'link',
		sortOrder: 300,
		href: '/entities',
	},
	{
		id: 'sys_drive',
		name: 'Drive',
		icon: 'ri:hard-drive-2-line',
		type: 'link',
		sortOrder: 350,
		href: '/drive',
	},
	{
		id: 'sys_spaces',
		name: 'Spaces',
		icon: 'ri:layout-masonry-line',
		type: 'link',
		sortOrder: 360,
		href: '/spaces',
	},
	{
		id: 'sys_actions',
		name: 'Actions',
		icon: 'ri:flashlight-line',
		type: 'link',
		sortOrder: 450,
		href: '/actions',
		groupBreak: true,
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
	'sys_projects': 'sys_spaces',
	'sys_things': 'sys_spaces',
};
