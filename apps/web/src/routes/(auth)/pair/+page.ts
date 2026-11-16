import type { PageLoad } from './$types';

/**
 * THE MOBILE SHELL NEVER SEES THIS PAGE.
 *
 * `/pair` is the browser's landing for an unpaired device. On a phone it is
 * the "old path" — a fourth-generation copy of the connect screen, reached
 * through the SPA, with no access to the key the app actually holds and so no
 * way to pair anything. It has appeared on a phone twice (2026-08-11), both
 * times because something upstream opened the SPA on a session the box would
 * not accept.
 *
 * It used to be kept away by making the airlock pre-flight a full `reach_status`
 * probe before opening the app — up to 15s of "Opening your server…" on every
 * launch to rule out a case that is otherwise vanishingly rare (the box was
 * reset, or this device was revoked). Closing it HERE instead is what lets the
 * airlock open the app optimistically: `connect.html#reset` is the airlock's
 * "your server doesn't recognize this phone" screen, which can actually fix it
 * by pairing again.
 *
 * `__VIRTUES_MOBILE__` is injected by the Tauri mobile shell (src-tauri's
 * `lib.rs`), so a real browser pointed at a box still gets the page below.
 *
 * The load never resolves on that path: the navigation away is the outcome, and
 * returning would paint the page we are leaving for a frame first.
 */
export const load: PageLoad = async () => {
	const shell = typeof window !== 'undefined' ? (window as unknown as Record<string, unknown>) : null;
	// The app's own copy (the phone, and the Mac since 2026-09-29): the box
	// refused this device, and `/reconnect` says so and can pair it again.
	// It used to go through `connect.html#reset`, which now forwards there.
	if (shell && (shell.__VIRTUES_BACKEND_ORIGIN__ || shell.__VIRTUES_MOBILE__)) {
		window.location.replace('/reconnect');
		await new Promise(() => {});
	}
	// Windows and Linux show the server's own copy through the loopback, so
	// there is no app page here to send them to. Relaunching runs the
	// launch check, which opens the connect page's "your server doesn't
	// recognize this device" screen (`connect.html#reset`), where they pair
	// again.
	//
	// At most once a minute: if the launch check and the server disagree,
	// the app would otherwise relaunch into this page forever. Past that,
	// the page below explains, and quitting and reopening tries again.
	if (shell && shell.__VIRTUES_BOX_URL__ && !relaunchedRecently()) {
		const { restartApp } = await import('$lib/tauri/bridge');
		await restartApp();
		await new Promise(() => {});
	}
	return {};
};

const RELAUNCHED_AT = 'virtues.pair.relaunchedAt';

/** True when this page relaunched the app in the last minute; records a
 *  relaunch otherwise. Storage that throws counts as "recently", so the page
 *  never loops when it cannot remember. */
function relaunchedRecently(): boolean {
	try {
		const last = Number(localStorage.getItem(RELAUNCHED_AT) ?? 0);
		if (Date.now() - last < 60_000) return true;
		localStorage.setItem(RELAUNCHED_AT, String(Date.now()));
		return false;
	} catch {
		return true;
	}
}
