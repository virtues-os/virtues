/**
 * Every reload of the app goes through here. A reload ends every file a
 * page has not finished uploading (`uploadsAtStake` in
 * `$lib/document/uploads`): each is only in this page's memory, and its
 * block is already on every device. A browser asks first (`beforeunload`),
 * but the Mac and iOS apps never do: their web views show no such prompt
 * and let the page go. So a reload or a relaunch the person asks for waits
 * while one is on its way, and says why, and one the app makes on its own
 * waits until none is.
 */

import { toast } from 'svelte-sonner';
import { uploadsAtStake, watchUploadsAtStake } from '$lib/document/uploads';

/**
 * Whether a reload or a relaunch the person asked for waits for a file still
 * on its way, said when it does, with `anyway` offered: a file the server
 * cannot store yet (no room, an error) is tried again every few minutes
 * until it can, so waiting is not always enough, and the person may choose
 * to add it again later instead.
 */
export function uploadsHold(what: 'reload' | 'relaunch', anyway: () => void): boolean {
	const files = uploadsAtStake();
	if (!files) return false;
	const one = files === 1;
	const them = one ? "A file you added to a page hasn't" : `${files} files you added to pages haven't`;
	const again = what === 'reload' ? 'Reload' : 'Relaunch';
	toast(what === 'reload' ? 'Not reloaded' : 'Not relaunched', {
		description: `${them} reached your server yet. ${again} after ${one ? 'it uploads' : 'they upload'}, or ${again.toLowerCase()} now and add ${one ? 'it' : 'them'} again.`,
		action: { label: `${again} anyway`, onClick: anyway },
	});
	return true;
}

/**
 * Reload the app as the person asked, unless a file is still on its way
 * (`uploadsHold`). Whether it reloads.
 */
export function reloadApp(): boolean {
	if (uploadsHold('reload', () => window.location.reload())) return false;
	window.location.reload();
	return true;
}

/**
 * Reload the app on its own, once no file is on its way and `when` holds:
 * now, or the first time both do, asked again whenever the files at stake
 * or the page's visibility change. Returns a stop.
 */
export function reloadOnceQuiet(when: () => boolean): () => void {
	let stopped = false;
	let unsubscribe: (() => void) | null = null;
	const stop = () => {
		stopped = true;
		unsubscribe?.();
		document.removeEventListener('visibilitychange', attempt);
	};
	function attempt() {
		if (stopped || !when() || uploadsAtStake() > 0) return;
		stop();
		window.location.reload();
	}
	document.addEventListener('visibilitychange', attempt);
	unsubscribe = watchUploadsAtStake().subscribe(attempt);
	if (stopped) unsubscribe();
	return stop;
}
