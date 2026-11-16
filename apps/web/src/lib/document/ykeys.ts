/**
 * The Yjs binding's plugin keys, by the string ProseMirror stores them under.
 *
 * The chat composer shares the page's schema, paste and trigger code, and that
 * code only asks whether a transaction came from the binding. Importing
 * `@tiptap/y-tiptap` to ask put yjs, lib0 and y-tiptap (about 110 KB) into the
 * chat home's first load. `tr.getMeta` and `state[key]` take the key's string
 * just as well, so this module names the strings and nothing imports yjs until
 * a page editor opens.
 *
 * `editor.ts` checks these against y-tiptap's own keys when it loads (see
 * `assertYKeys`), so a y-tiptap upgrade that renames a key fails loudly instead
 * of silently treating every remote edit as local.
 */

import type { EditorState, Transaction } from '@tiptap/pm/state';

/** `ySyncPluginKey.key`: `new PluginKey('y-sync')`, the first of that name. */
export const Y_SYNC_KEY = 'y-sync$';
/** `yUndoPluginKey.key`: `new PluginKey('y-undo')`. */
export const Y_UNDO_KEY = 'y-undo$';

/** True when the Yjs binding made `tr` (another device's edit, or undo/redo). */
export function isChangeOrigin(tr: Transaction): boolean {
	return !!tr.getMeta(Y_SYNC_KEY);
}

/** The y-undo plugin's state, when the editor is bound to a shared document. */
export function yUndoState(state: EditorState): unknown {
	return (state as unknown as Record<string, unknown>)[Y_UNDO_KEY];
}

/** Throws when y-tiptap's keys are not the strings above. */
export function assertYKeys(keys: { sync: string; undo: string }): void {
	if (keys.sync !== Y_SYNC_KEY || keys.undo !== Y_UNDO_KEY) {
		throw new Error(
			`ykeys: y-tiptap's plugin keys are ${keys.sync} and ${keys.undo}, expected ${Y_SYNC_KEY} and ${Y_UNDO_KEY}. Update ykeys.ts.`
		);
	}
}
