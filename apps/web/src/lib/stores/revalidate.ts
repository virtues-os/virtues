/**
 * Reload the shared lists when the app comes back into view.
 *
 * The chat, page, project and pin lists load once at boot. Nothing pushes
 * changes, so a chat started on the phone, a page the assistant renamed or a
 * project filed from another device stayed invisible here until a full
 * reload. The Mac app in particular stays open for days.
 *
 * Coming back is the moment someone looks: the window regains focus, the
 * page becomes visible again, or the network returns. At most once per
 * `MIN_GAP_MS`, so switching windows back and forth costs nothing. The
 * stores keep their last good list when a reload fails.
 */

import { chatSessions } from './chatSessions.svelte';
import { pagesStore } from './pages.svelte';
import { pinsStore } from './pins.svelte';
import { projectStore } from './project.svelte';

const MIN_GAP_MS = 30_000;

let last = Date.now();

function revalidate() {
	if (typeof document !== 'undefined' && document.hidden) return;
	const now = Date.now();
	if (now - last < MIN_GAP_MS) return;
	last = now;
	void chatSessions.load();
	void pinsStore.load();
	void projectStore.load();
	// Pages load on demand (the sidebar asks when it first draws them), so
	// only a list that has been loaded, or tried to, is reloaded here.
	if (pagesStore.pages.length > 0 || pagesStore.pagesError) void pagesStore.loadPages();
}

let installed = false;

/** Start listening. Safe to call more than once. */
export function installRevalidate(): void {
	if (installed || typeof window === 'undefined') return;
	installed = true;
	window.addEventListener('focus', revalidate);
	window.addEventListener('online', revalidate);
	document.addEventListener('visibilitychange', revalidate);
}
