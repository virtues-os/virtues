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
import { shortcuts } from '$lib/shortcuts/registry.svelte';
import { newProject } from '$lib/utils/projectActions';
import { newTaskDialog } from '$lib/stores/newTask.svelte';
import { getLocalDateSlug } from '$lib/utils/dateUtils';

/** Where a new thing opens: its route, and the label its tab starts with. */
export type Opener = (route: string, label: string) => void;

export async function newPage(open: Opener): Promise<void> {
	const { pagesStore } = await import('$lib/stores/pages.svelte');
	const page = await pagesStore.createNewPage();
	open(`/page/${page.id}`, page.title);
}

/** The New task dialog. */
export function newTask(): void {
	newTaskDialog.open();
}

/**
 * A new tab beside the active one in `paneId`. A route already open in a tab
 * (Threads, the wiki) brings that tab to the front instead of doubling it.
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
	threads: '/threads',
	applets: '/applets',
	wiki: '/wiki',
};

/**
 * A day's page, `daysAgo` before today. Dated, as an open day tab's route
 * is, so a day already open in a tab comes to the front (`inNewTab`).
 */
function dayPageRoute(daysAgo: number): string {
	const d = new Date();
	d.setDate(d.getDate() - daysAgo);
	return `/day/day_${getLocalDateSlug(d)}`;
}

/**
 * The + menu. New things first, in the sidebar's order, then today's and
 * yesterday's pages, then the ways to reach something that exists: the
 * in-app browser (Mac only, so the caller says whether it is there) and the
 * sidebar's doors. Search has its own button in the window's corner, and a
 * temporary chat its own shortcut (⌘⇧I), so neither is offered here.
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
			action: () => newTask(),
		},
		{
			id: 'new-thread',
			label: 'New thread',
			icon: 'atlas:projects',
			action: () => newProject(open),
			dividerAfter: true,
		},
		{
			id: 'today',
			label: 'Today',
			icon: 'atlas:day',
			action: () => open(dayPageRoute(0), 'Today'),
		},
		{
			id: 'yesterday',
			label: 'Yesterday',
			icon: 'atlas:calendar',
			action: () => open(dayPageRoute(1), 'Yesterday'),
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
