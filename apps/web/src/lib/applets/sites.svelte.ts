/**
 * A site is an applet with a view. This holds the two things a chat needs
 * when the assistant makes or rebuilds one: opening it beside the chat, and
 * telling a frame that is already showing it to load the new view.
 */
import { windowShellStore } from '$lib/stores/window-shell.svelte';
import { mobileLayout } from '$lib/stores/mobileLayout.svelte';
import { homeRoute } from './routes';

class SiteBuilds {
	/** How many times each site's view was rebuilt in this session. A frame
	 *  reads its site's count, so a rebuild reloads it. */
	counts = $state<Record<string, number>>({});

	rebuilt(appletId: string) {
		this.counts[appletId] = (this.counts[appletId] ?? 0) + 1;
	}
}

export const siteBuilds = new SiteBuilds();

/**
 * Open a site in the pane beside the active one and leave the active pane
 * where it was, so a chat that is building the site keeps the keyboard.
 */
export function openSiteBeside(appletId: string, name?: string) {
	const from = windowShellStore.activePaneId;
	windowShellStore.openRouteBeside(homeRoute(appletId), name);
	if (windowShellStore.isSplit) windowShellStore.activePaneId = from;
}

/**
 * What a chat does when the assistant has just made or rebuilt a site: load
 * the new view in any frame showing it, then open it the way the app opens
 * anything the assistant made. That is beside the chat when the window is
 * already split, and otherwise a new tab next to the chat. It never splits
 * the window, and the chat keeps the keyboard. A phone shows one view at a
 * time, and leaving the chat in the middle of a reply would lose the person's
 * place, so there the chat's own row is the way in.
 */
export function showBuiltSite(appletId: string, name?: string) {
	siteBuilds.rebuilt(appletId);
	if (mobileLayout.isMobile) return;
	const from = windowShellStore.activePaneId;
	windowShellStore.openRouteInSplitOrActive(homeRoute(appletId), name);
	if (windowShellStore.isSplit) windowShellStore.activePaneId = from;
}
