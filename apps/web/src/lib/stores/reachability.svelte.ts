/**
 * Can this client reach its server right now?
 *
 * Chat-first made this question load-bearing on the phone: the failure state
 * IS the home screen, so the shell needs one honest signal to show for it
 * (see MobileShell's banner) instead of a spinner that never resolves.
 *
 * No new heartbeat. Two signals the app already produces are combined:
 *  - `navigator.onLine` — the radio's own verdict, instant on airplane mode;
 *  - `chatSessions.error` — the boot-time and drawer-open loads of the chat
 *    list, which is the request this shell depends on anyway.
 *
 * While unreachable, the list load is retried on an interval (visible pages
 * only) so the banner heals itself the moment the box comes back — the same
 * request that raised the flag is the one that lowers it. A 500 also raises
 * it: "your server is in trouble" and "your server is gone" earn the same
 * calm banner, and the distinction belongs to This device, its door.
 */

import { chatSessions } from './chatSessions.svelte';

const RETRY_MS = 12_000;

let online = $state(typeof navigator === 'undefined' ? true : navigator.onLine);

if (typeof window !== 'undefined') {
	window.addEventListener('online', () => {
		online = true;
		// The radio came back — reprobe now rather than waiting out the interval.
		void chatSessions.load();
	});
	window.addEventListener('offline', () => (online = false));

	// Every layout since 2026-09-29: the Mac keeps its own copy of the app up
	// when the server goes away, so the desktop needs the banner to heal
	// itself too (ServerUnreachableBar). Only while there is something to heal
	// and someone to see it, so a healthy app never polls.
	setInterval(() => {
		if (document.hidden) return;
		if (!online || chatSessions.error !== null) void chatSessions.load();
	}, RETRY_MS);
}

export const reachability = {
	/** True when the server cannot be reached (or is erroring) right now. */
	get unreachable(): boolean {
		return !online || chatSessions.error !== null;
	},
};
