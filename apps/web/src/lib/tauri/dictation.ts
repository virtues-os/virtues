/**
 * ⌘D and the composer's mic share one dictation: a second ⌘D stops what the
 * first started, and sending the message stops it too.
 *
 * The Mac does not tell the page when dictation ends, so `listening` is what
 * this page last asked for. If you end dictation yourself with Esc, the next
 * ⌘D sends a stop that does nothing, and the one after starts it again. A
 * shell too old to stop dictation never sets it, so there every ⌘D starts.
 */

import { DICTATION_STOP_SURFACE, shellSupports, startDictation, stopDictation } from '$lib/tauri/bridge';

let listening = false;
let canStop: Promise<boolean> | null = null;

export function beginDictation(): void {
	canStop ??= shellSupports(DICTATION_STOP_SURFACE);
	void canStop.then((yes) => (listening = yes));
	void startDictation();
}

export function endDictation(): void {
	if (!listening) return;
	listening = false;
	void stopDictation();
}

export function dictationListening(): boolean {
	return listening;
}
