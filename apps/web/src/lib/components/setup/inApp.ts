/**
 * Where `/setup` opens: full screen, or as a tab in the app.
 *
 * ONE ADDRESS, TWO PLACES. The first time through, Setup is its own stage:
 * the opening, the letter, pairing, the close. Once someone has been let
 * into the app, coming back to finish a step they skipped shouldn't take
 * them out of it, so `/setup` then opens in the app's pane like any other
 * room (`SetupView`). Two places ask this module: the stage's own load,
 * which sends a fresh load of `/setup` on into the app, and the app, which
 * catches a link to `/setup` before it leaves (routes/(app)/+layout.svelte).
 * Not SvelteKit's `reroute`: it caches its answer per address for the whole
 * session, and this answer changes.
 *
 * The rule, all of which must hold for the pane:
 * - this device has been let into the app on this server (the app's gate
 *   let it through: `markInApp`), and nothing since has sent it back to
 *   Setup or to pairing (`markInApp(false)` at each of those);
 * - it isn't a phone, whose app has no panes;
 * - the step isn't Welcome, whose opening is only ever full screen (Settings
 *   → Introduction replays it there);
 * - the full-screen stage isn't already up (`holdStage`): a replay that
 *   started there stays there to its close.
 */

const IN_APP_KEY = 'virtues-setup-in-app';

/** The full-screen stage is mounted. In memory only: a reload re-decides. */
let stageUp = false;

export function markInApp(on: boolean): void {
	try {
		if (on) localStorage.setItem(IN_APP_KEY, '1');
		else localStorage.removeItem(IN_APP_KEY);
	} catch {
		/* full screen, the safe default */
	}
}

export function holdStage(on: boolean): void {
	stageUp = on;
}

type Shell = { __VIRTUES_MOBILE__?: boolean };

/** Whether `pathname` (a `/setup` address) opens in the app's pane. */
export function setupOpensInApp(pathname: string): boolean {
	if (typeof window === 'undefined' || stageUp) return false;
	if (pathname === '/setup/welcome') return false;
	if ((window as unknown as Shell).__VIRTUES_MOBILE__ || window.innerWidth < 768) return false;
	try {
		// A device that forgot its server this launch has no app to open in.
		if (sessionStorage.getItem('virtues-forgot-server') === '1') return false;
		return localStorage.getItem(IN_APP_KEY) === '1';
	} catch {
		return false;
	}
}

/** Where a fresh load of `/setup` is sent to reach the app: its catch-all
 *  takes it, the shell opens Setup's tab from what follows, and the address
 *  goes back to `/setup/...` as the tab's own. */
export const IN_APP_PREFIX = '/_in-app';
