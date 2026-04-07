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
	type: 'smart' | 'static';
	sortOrder: number;
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
		id: 'sys_chats',
		name: 'Chats',
		icon: 'ri:chat-1-line',
		type: 'smart',
		sortOrder: 100,
		namespace: 'chat',
		limit: 8,
		quickAdd: 'chat',
		moreRoute: '/chat-history',
	},
	{
		id: 'sys_pages',
		name: 'Pages',
		icon: 'ri:file-text-line',
		type: 'smart',
		sortOrder: 200,
		namespace: 'page',
		limit: 8,
		quickAdd: 'page',
		moreRoute: '/page',
	},
	{
		id: 'sys_wiki',
		name: 'Wiki',
		icon: 'ri:book-open-line',
		type: 'static',
		sortOrder: 300,
		items: [
			{ id: 'wiki-day', label: 'Today', icon: 'ri:calendar-todo-line', href: '/day' },
			{ id: 'wiki-entities', label: 'Entities', icon: 'ri:group-line', href: '/entities' },
			{ id: 'wiki-narrative', label: 'Narrative Identity', icon: 'ri:quill-pen-line', href: '/narrative-identity' },
		],
	},
	{
		id: 'sys_files',
		name: 'Files',
		icon: 'ri:folder-line',
		type: 'static',
		sortOrder: 350,
		items: [
			{ id: 'files-drive', label: 'Drive', icon: 'ri:hard-drive-2-line', href: '/drive' },
		],
	},
	{
		id: 'sys_apps',
		name: 'Apps',
		icon: 'ri:apps-line',
		type: 'static',
		sortOrder: 375,
		items: [
			{ id: 'apps-actions', label: 'Actions', icon: 'ri:flashlight-line', href: '/actions' },
		],
	},
];

/** Map old DB view IDs → new constant IDs (for localStorage migration) */
export const LEGACY_ID_MAP: Record<string, string> = {
	'view_sys_sec_chats': 'sys_chats',
	'view_sys_sec_pages': 'sys_pages',
	'view_sys_sec_wiki': 'sys_wiki',
	'view_sys_sec_files': 'sys_files',
	'view_sys_sec_data': 'sys_connections',
	'view_sys_sec_developer': '', // deleted, no mapping
};
