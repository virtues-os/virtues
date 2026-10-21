/**
 * A chat edit's animation on the page it changed.
 *
 * A chat `edit_page` applies on the server and reaches an open editor as a
 * remote Yjs change, so nothing in the editor marks it as the assistant's.
 * When the page is open in a block editor (`lib/document/registry.ts`), the
 * blocks the edit wrote flash there, by id (`lib/document/presence.ts`).
 */

import type { Editor } from "@tiptap/core";
import contractJson from "$contract";
import { getTreeEditor } from "$lib/document/registry";

/**
 * How long to wait for the blocks a chat edit wrote to sync into the open
 * editor. Past this the edit flashes what has arrived, or nothing.
 */
const WAIT_MS = 2000;

/** The attribute a block's id is kept in (`contract.json`). */
const ID_ATTR: string = (contractJson as { id: { attr: string } }).id.attr;

/** The ids in `ids` that a block of the editor's page carries. */
function blocksPresent(editor: Editor, ids: string[]): string[] {
	const want = new Set(ids);
	const found: string[] = [];
	editor.state.doc.descendants((node) => {
		const id = node.attrs[ID_ATTR];
		if (typeof id === "string" && want.has(id)) found.push(id);
		return found.length < want.size;
	});
	return found;
}

/**
 * Flash `blockIds` (the edit's `blocks`) in the editor open on `pageId`. If
 * the sync has not brought them all yet, look again each time the editor's
 * document changes, for up to `WAIT_MS`. Nothing happens when the page is
 * not open, or the edit names no blocks (a result from a markdown page).
 */
export function animateChatEdit(pageId: string, blockIds: string[] = []): void {
	if (!pageId || !blockIds.length) return;
	const tree = getTreeEditor(pageId);
	if (!tree) return;
	const flash = (present: string[]): void => {
		if (!present.length) return;
		// Loaded here, not at the top: the chat reaches this module, and
		// the block editor's code is already loaded when one is open.
		void import("$lib/document/presence").then(({ flashBlocks }) => {
			if (!tree.isDestroyed) flashBlocks(tree, present);
		});
	};
	const now = blocksPresent(tree, blockIds);
	if (now.length === blockIds.length) {
		flash(now);
		return;
	}
	let timer: ReturnType<typeof setTimeout> | undefined;
	const stop = (): void => {
		clearTimeout(timer);
		tree.off("update", look);
		tree.off("destroy", stop);
	};
	function look(): void {
		const present = blocksPresent(tree!, blockIds);
		if (present.length < blockIds.length) return;
		stop();
		flash(present);
	}
	timer = setTimeout(() => {
		stop();
		if (!tree.isDestroyed) flash(blocksPresent(tree, blockIds));
	}, WAIT_MS);
	tree.on("update", look);
	tree.on("destroy", stop);
}
