/**
 * The line a drag shows where it would land, in a page and in the chat
 * composer: ProseMirror's drop cursor, drawn as every drag in the app draws
 * one (agents/build/design.md, "Drag and drop"). Between blocks it is a line
 * across the column; inside text it is a bar the height of the line, like
 * the caret. It fades in when it appears and glides from gap to gap
 * (drop-line.css).
 *
 * The line goes the moment something is dropped. Left to itself the drop
 * cursor keeps it for a few more milliseconds and draws it again from the
 * last dragover as the drop's edits land, at a place worked out on the
 * changed page.
 */

import { Dropcursor } from '@tiptap/extensions';
import { Plugin } from '@tiptap/pm/state';
import type { EditorView } from '@tiptap/pm/view';
import { lengthPx, motionMs, tokenString } from '../utils/motion';
import './drop-line.css';

/** How things a drop moved travel: `--motion-state` on `--ease-premium`. */
export function dropMotion(): { duration: number; easing: string } {
	return {
		duration: motionMs('--motion-state', 150),
		easing: tokenString('--ease-premium', 'cubic-bezier(0.2, 0, 0, 1)'),
	};
}

/**
 * Take the drop line away now. The drop cursor removes it when a drag
 * leaves the editor for somewhere else, which is what this tells it.
 */
export function clearDropLine(view: EditorView): void {
	view.dom.dispatchEvent(new MouseEvent('dragleave', { relatedTarget: null }));
}

const DropLineCursor = Dropcursor.extend({
	addProseMirrorPlugins() {
		return [
			...(this.parent?.() ?? []),
			new Plugin({
				props: {
					handleDOMEvents: {
						// Ahead of the drop's own edits.
						drop(view) {
							clearDropLine(view);
							return false;
						},
					},
				},
			}),
		];
	},
});

/** The drop line, its thickness read from `--drop-line-width` as the editor is made. */
export function dropLine() {
	return DropLineCursor.configure({ color: false, width: lengthPx('--drop-line-width', 2), class: 'doc-dropcursor' });
}
