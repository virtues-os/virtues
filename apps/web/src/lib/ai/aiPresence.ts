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

/** Wait before looking again for what a chat edit wrote, which may not have synced yet. */
const RETRY_MS = 150;

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
 * the sync has not brought them yet, look once more shortly after. Nothing
 * happens when the page is not open, or the edit names no blocks (a result
 * from a markdown page).
 */
export function animateChatEdit(pageId: string, blockIds: string[] = []): void {
	if (!pageId || !blockIds.length) return;
	const tree = getTreeEditor(pageId);
	if (!tree) return;
	const flash = (attempt: number): void => {
		if (tree.isDestroyed) return;
		const present = blocksPresent(tree, blockIds);
		if (present.length < blockIds.length && attempt === 0) {
			setTimeout(() => flash(1), RETRY_MS);
			return;
		}
		// Loaded here, not at the top: the chat reaches this module, and
		// the block editor's code is already loaded when one is open.
		void import("$lib/document/presence").then(({ flashBlocks }) => {
			if (!tree.isDestroyed) flashBlocks(tree, present);
		});
	};
	flash(0);
}
