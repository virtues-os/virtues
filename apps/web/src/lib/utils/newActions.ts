/**
 * Making a new thing, and the tab strip's + menu that offers all of them.
 *
 * Each maker takes where its result opens. The sidebar's doors and shortcuts
 * open it in the tab you are on, the phone's drawer opens it and closes
 * itself, and the + menu opens it in a new tab beside the one you are on.
 */
import type { ContextMenuItem } from '$lib/stores/contextMenu.svelte';
import { windowShellStore } from '$lib/stores/window-shell.svelte';
import { sidebarDoors, type DoorId } from '$lib/stores/sidebarDoors.svelte';
import { search } from '$lib/stores/search.svelte';
import { shortcuts } from '$lib/shortcuts/registry.svelte';
import { newProject } from '$lib/utils/projectActions';

/** Where a new thing opens: its route, and the label its tab starts with. */
export type Opener = (route: string, label: string) => void;

export async function newPage(open: Opener): Promise<void> {
	const { pagesStore } = await import('$lib/stores/pages.svelte');
	const page = await pagesStore.createNewPage();
	open(`/page/${page.id}`, page.title);
}

/** The Tasks list, with its add row open. */
export async function newTask(open: Opener): Promise<void> {
	const { tasksStore } = await import('$lib/stores/tasks.svelte');
	tasksStore.requestAdd();
	open('/tasks', 'Tasks');
}

/**
 * A new tab beside the active one in `paneId`. A route already open in a tab
 * (Tasks, the wiki) brings that tab to the front instead of doubling it.
 */
export function inNewTab(paneId?: 'left' | 'right'): Opener {
	return (route, label) => {
		const open = windowShellStore.findTab((t) => t.route === route);
		if (open) {
			windowShellStore.setActiveTabInPane(open.tab.id, open.paneId as 'left' | 'right');
			return;
		}
		windowShellStore.openTabFromRoute(route, { label, paneId, forceNew: true, adjacent: true });
	};
}

/** Each door's list, for the menu's Go to. New chat is the menu's first row. */
const DOOR_ROUTES: Partial<Record<DoorId, string>> = {
	pages: '/page',
	tasks: '/tasks',
	projects: '/projects',
	applets: '/applets',
	wiki: '/wiki',
};

/**
 * The + menu. New things first, in the sidebar's order, then the ways to
 * reach something that exists: the in-app browser (Mac only, so the caller
 * says whether it is there), search, and the sidebar's doors.
 */
export function newTabMenuItems(paneId: 'left' | 'right' | undefined, opts: { browser: boolean }): ContextMenuItem[] {
	const open = inNewTab(paneId);
	const items: ContextMenuItem[] = [
		{
			id: 'new-chat',
			label: 'New chat',
			icon: 'atlas:new-chat',
			shortcut: shortcuts.format('mod+n'),
			action: () => void windowShellStore.newChat(paneId),
		},
		{
			id: 'new-temporary-chat',
			label: 'New temporary chat',
			icon: 'mynaui:incognito',
			shortcut: shortcuts.format('mod+shift+i'),
			action: () => open('/?temporary=1', 'Temporary Chat'),
		},
		{
			id: 'new-page',
			label: 'New page',
			icon: 'atlas:pages',
			shortcut: shortcuts.format('mod+shift+n'),
			action: () => newPage(open),
		},
		{
			id: 'new-task',
			label: 'New task',
			icon: 'atlas:tasks',
			shortcut: shortcuts.format('mod+shift+e'),
			action: () => newTask(open),
		},
		{
			id: 'new-project',
			label: 'New project',
			icon: 'atlas:projects',
			action: () => newProject(open),
			dividerAfter: true,
		},
	];

	if (opts.browser) {
		items.push({
			id: 'browser',
			label: 'Browser',
			icon: 'mynaui:globe',
			// One browser, shared with the assistant: an open one comes to the front.
			action: () => {
				const browser = windowShellStore.findTab((t) => t.type === 'browser');
				if (browser) windowShellStore.setActiveTabInPane(browser.tab.id, browser.paneId as 'left' | 'right');
				else open('/browser', 'Browser');
			},
		});
	}

	items.push(
		{
			id: 'search',
			label: 'Search',
			icon: 'atlas:search',
			shortcut: shortcuts.format('mod+k'),
			action: () => search.show(),
		},
		{
			id: 'go-to',
			label: 'Go to',
			icon: 'mynaui:compass',
			submenu: sidebarDoors.all.flatMap((door) => {
				const route = DOOR_ROUTES[door.id];
				return route
					? [{ id: `go-${door.id}`, label: door.label, icon: `atlas:${door.glyph}`, action: () => open(route, door.label) }]
					: [];
			}),
		},
	);

	return items;
}
