/**
 * The page's undo is y-undo: one history of the person's own edits, shared
 * with every device's binding of the page.
 */

import type { EditorState, Transaction } from '@tiptap/pm/state';
import type { EditorView } from '@tiptap/pm/view';
import { yUndoPluginKey } from '@tiptap/y-tiptap';
import type * as Y from 'yjs';

/** The page's undo manager, when the editor is bound to a shared document. */
export function undoManagerOf(state: EditorState): Y.UndoManager | null {
	return (yUndoPluginKey.getState(state) as { undoManager?: Y.UndoManager } | undefined)?.undoManager ?? null;
}

/**
 * Dispatch `tr` as an undo step of its own: a paste or files put in the
 * page. Within y-undo's capture window of typing just before it, it would
 * join that typing, and one undo would take both.
 */
export function ownUndoStep(view: EditorView, tr: Transaction): void {
	const manager = undoManagerOf(view.state);
	manager?.stopCapturing();
	view.dispatch(tr);
	manager?.stopCapturing();
}
