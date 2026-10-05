/**
 * Which tabs keep their view mounted while hidden.
 *
 * Every tab ever opened used to stay mounted behind `display: none`: its DOM,
 * its sockets, its timers, its maps and PDF workers and terminal. A Mac app
 * open for days held all of it, which is how the window reached 2-3 GB. The
 * reasons for keeping a hidden view alive are now held elsewhere: a chat's
 * reply runs on the box and its stream outlives the tab (chatInstances), a
 * page's text is in Yjs and IndexedDB, a terminal reattaches to tmux.
 *
 * So the visible tabs stay mounted, plus the few most recently looked at
 * (switching back and forth between two or three tabs stays instant), plus
 * any tab whose content exists only in its view. Everything else unmounts
 * and remounts from its route when it is next opened. A tab restored at
 * launch but never opened is never mounted at all.
 *
 * What a remount costs: scroll position, undo history in an editor, an
 * unsaved form. Those are the same things a browser's discarded tab loses.
 */

import { isTemporaryRoute } from '$lib/components/chat/state/chatRoute';

/** Hidden tabs kept mounted, besides the visible ones. */
export const KEEP_HIDDEN = 4;

/** How many recent ids to remember; only the first few matter. */
const RECENT_CAP = 32;

/** The recency list after `visible` were shown: those first, then the rest in order. */
export function touch(recent: readonly string[], visible: readonly string[]): string[] {
	const seen = new Set(visible);
	return [...visible, ...recent.filter((id) => !seen.has(id))].slice(0, RECENT_CAP);
}

/**
 * A view that is the only copy of what it shows. A temporary chat's
 * transcript lives in its view's chat instance and nowhere else: unmounting
 * it would delete the conversation.
 */
export function mustStayMounted(tab: { route: string }): boolean {
	return isTemporaryRoute(tab.route);
}

/** The ids of the tabs whose views stay mounted. */
export function mountedTabs(
	tabs: readonly { id: string; route: string }[],
	visible: readonly string[],
	recent: readonly string[],
	keepHidden = KEEP_HIDDEN,
): Set<string> {
	const open = new Set(tabs.map((t) => t.id));
	const keep = new Set(visible.filter((id) => open.has(id)));
	let hidden = 0;
	for (const id of recent) {
		if (hidden >= keepHidden) break;
		if (!open.has(id) || keep.has(id)) continue;
		keep.add(id);
		hidden++;
	}
	for (const t of tabs) if (mustStayMounted(t)) keep.add(t.id);
	return keep;
}
