/**
 * The block editors open now, by page: how code outside an editor (the chat's
 * edit animation, `ai/aiPresence.ts`) reaches the editor showing a page.
 * `DocumentEditor` registers on mount and unregisters on destroy.
 */

import type { Editor } from '@tiptap/core';

const editors = new Map<string, Editor>();

export function registerTreeEditor(pageId: string, editor: Editor): void {
	editors.set(pageId, editor);
}

/** Forget `editor` for `pageId`, unless a newer editor has taken its place. */
export function unregisterTreeEditor(pageId: string, editor: Editor): void {
	if (editors.get(pageId) === editor) editors.delete(pageId);
}

/** The editor showing `pageId`, if one is open and alive. */
export function getTreeEditor(pageId: string): Editor | null {
	const editor = editors.get(pageId);
	if (!editor || editor.isDestroyed) return null;
	return editor;
}
