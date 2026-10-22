/**
 * Where you were scrolled in a tab, for Back and Forward.
 *
 * A view saves its scroll as it leaves a place in a tab's history, and
 * reads it back when the tab returns to that same place. The key carries
 * the history position, so reaching the same chat by a new click (a new
 * history entry) opens it fresh, at its latest message, as a browser does.
 * In memory only: a reload starts every view at its usual place.
 */

const CAP = 200;
const memory = new Map<string, number>();

const keyOf = (tabId: string, historyIndex: number, route: string) => `${tabId}|${historyIndex}|${route}`;

export function saveScroll(tabId: string, historyIndex: number, route: string, top: number): void {
	const key = keyOf(tabId, historyIndex, route);
	memory.delete(key);
	memory.set(key, top);
	if (memory.size > CAP) memory.delete(memory.keys().next().value!);
}

/** The saved scroll for this place, if the tab has been here before. */
export function savedScroll(tabId: string, historyIndex: number, route: string): number | undefined {
	return memory.get(keyOf(tabId, historyIndex, route));
}
