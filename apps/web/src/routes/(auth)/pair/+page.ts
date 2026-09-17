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
	if (typeof window !== 'undefined' && (window as unknown as Record<string, unknown>).__VIRTUES_MOBILE__) {
		window.location.replace('/connect.html#reset');
		await new Promise(() => {});
	}
	return {};
};
