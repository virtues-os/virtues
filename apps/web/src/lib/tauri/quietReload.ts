/**
 * Whether the app may reload itself while hidden, to take up a UI the shell
 * has staged (`checkForNewUi` in the app layout): only when a reload costs
 * nothing that isn't saved.
 */

import { uploadsAtStake } from '$lib/document/uploads';

/** What the page holds that a reload would lose, as the app layout reads it from its stores. */
export interface AtStake {
	/** A temporary chat is open: its transcript lives only in this page. */
	temporaryChat: boolean;
	/** Setup shows in a pane: a step can hold a connection or an interview mid-flight. */
	setupShowing: boolean;
	/** A reply is still streaming. */
	replying: boolean;
	/** The focused element, whose text a reload would lose (a half-entered key or rename). */
	focused: Element | null;
}

/**
 * Would a reload now cost anything that isn't saved? What `at` holds, and a
 * page's file not yet stored (`uploadsAtStake`): on its way, waiting to be
 * tried again, or stopped short of a refusal and waiting on Try again. Its
 * block is already in the page, and the file is only in this page's memory,
 * so a reload would leave that block, on every device, a file that never
 * finished uploading. Composer drafts are saved as they are typed, so they
 * don't count.
 */
export function nothingToLose(at: AtStake): boolean {
	if (uploadsAtStake() > 0) return false;
	if (at.temporaryChat || at.setupShowing || at.replying) return false;
	const focused = at.focused as HTMLInputElement | HTMLTextAreaElement | null;
	return !(focused && typeof focused.value === 'string' && focused.value.length > 0);
}
