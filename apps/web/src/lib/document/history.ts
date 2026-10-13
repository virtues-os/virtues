/**
 * The page's undo is y-undo: one history of the person's own edits, shared
 * with every device's binding of the page.
 */

import type { Transaction } from '@tiptap/pm/state';
import type { EditorView } from '@tiptap/pm/view';
import { yUndoPluginKey } from '@tiptap/y-tiptap';
import type * as Y from 'yjs';

/**
 * Dispatch `tr` as an undo step of its own: a paste or files put in the
 * page. Within y-undo's capture window of typing just before it, it would
 * join that typing, and one undo would take both.
 */
export function ownUndoStep(view: EditorView, tr: Transaction): void {
	const manager = (yUndoPluginKey.getState(view.state) as { undoManager?: Y.UndoManager } | undefined)?.undoManager;
	manager?.stopCapturing();
	view.dispatch(tr);
	manager?.stopCapturing();
}
