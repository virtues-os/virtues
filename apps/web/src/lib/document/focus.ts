/**
 * Focus and typewriter mode, one toggle (`pageDisplay.focusMode`):
 *  - focus: every top-level block but the caret's is dimmed (`doc-dim`);
 *  - typewriter: the caret is kept at 40% of the scroller's height while
 *    the person writes. Another device's edit never scrolls the page.
 * Decorations and scrolling only; nothing is written.
 */

import { Extension } from '@tiptap/core';
import { Plugin, PluginKey, type EditorState, type Transaction } from '@tiptap/pm/state';
import { Decoration, DecorationSet, type EditorView } from '@tiptap/pm/view';
import { isChangeOrigin } from '@tiptap/extension-collaboration';
import { scrollerOf } from './outline';

/** Where typewriter mode keeps the caret, as a share of the scroller's height. */
const CARET_AT = 0.4;

interface FocusState {
	on: boolean;
	/** The last transaction moved the person's own caret or text. */
	moved: boolean;
}

const focusKey = new PluginKey<FocusState>('focusMode');

function dimmed(state: EditorState): DecorationSet {
	const head = state.selection.$head;
	const current = head.depth > 0 ? head.before(1) : -1;
	const decos: Decoration[] = [];
	state.doc.forEach((node, offset) => {
		if (offset !== current) decos.push(Decoration.node(offset, offset + node.nodeSize, { class: 'doc-dim' }));
	});
	return DecorationSet.create(state.doc, decos);
}

function keepCaretInPlace(view: EditorView): void {
	requestAnimationFrame(() => {
		if (view.isDestroyed) return;
		const scroller = scrollerOf(view.dom);
		const caret = view.coordsAtPos(view.state.selection.head);
		const box =
			scroller === document.scrollingElement
				? { top: 0, height: window.innerHeight }
				: scroller.getBoundingClientRect();
		const delta = caret.top - (box.top + box.height * CARET_AT);
		if (Math.abs(delta) > 2) scroller.scrollBy({ top: delta });
	});
}

/** Focus mode, starting as `initial`; `setFocusMode` changes it. */
export function focusMode(initial: boolean): Extension {
	return Extension.create({
		name: 'pageFocusMode',
		addProseMirrorPlugins() {
			return [
				new Plugin<FocusState>({
					key: focusKey,
					state: {
						init: () => ({ on: initial, moved: false }),
						apply(tr: Transaction, prev) {
							const on = (tr.getMeta(focusKey) as boolean | undefined) ?? prev.on;
							const moved = (tr.docChanged || tr.selectionSet) && !isChangeOrigin(tr);
							return on === prev.on && moved === prev.moved ? prev : { on, moved };
						},
					},
					props: {
						decorations: (state) => (focusKey.getState(state)?.on ? dimmed(state) : null),
					},
					view: () => ({
						update(view) {
							const s = focusKey.getState(view.state);
							if (s?.on && s.moved && view.hasFocus()) keepCaretInPlace(view);
						},
					}),
				}),
			];
		},
	});
}

/** Turn focus mode on or off in an open editor. */
export function setFocusMode(view: EditorView, on: boolean): void {
	if (focusKey.getState(view.state)?.on === on) return;
	view.dispatch(view.state.tr.setMeta(focusKey, on));
}
