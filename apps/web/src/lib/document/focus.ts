/**
 * Focus and typewriter mode, one toggle (`pageDisplay.focusMode`):
 *  - focus: every top-level block but the caret's is dimmed (`doc-dim`);
 *  - typewriter: the caret is kept at 40% of the scroller's height while
 *    the person types or moves the caret with the keyboard. A click, a
 *    drag or another device's edit never scrolls the page.
 * Decorations and scrolling only; nothing is written.
 */

import { Extension } from '@tiptap/core';
import { Plugin, PluginKey, type EditorState, type Transaction } from '@tiptap/pm/state';
import { Decoration, DecorationSet, type EditorView } from '@tiptap/pm/view';
import { isChangeOrigin } from '@tiptap/extension-collaboration';
import { scrollerOf } from './outline';

/** Where typewriter mode keeps the caret, as a share of the scroller's height. */
const CARET_AT = 0.4;

/**
 * How long after the pointer is let go a selection still counts as the
 * pointer's: the browser reports a click's or double-click's selection
 * just after the button comes up.
 */
const POINTER_GRACE_MS = 300;

/**
 * What the last transaction did to the person's own caret: `typed` when it
 * changed the text, `caret` when it only moved the selection.
 */
type Moved = 'typed' | 'caret' | null;

interface FocusState {
	on: boolean;
	moved: Moved;
	/** Where the caret's top-level block starts, or -1 when the caret is between blocks. */
	current: number;
	/** The dimmed blocks, kept until the caret's block or the document changes. */
	dim: DecorationSet | null;
}

const focusKey = new PluginKey<FocusState>('focusMode');

function currentBlock(state: EditorState): number {
	const head = state.selection.$head;
	return head.depth > 0 ? head.before(1) : -1;
}

function dimmed(doc: EditorState['doc'], current: number): DecorationSet {
	const decos: Decoration[] = [];
	doc.forEach((node, offset) => {
		if (offset !== current) decos.push(Decoration.node(offset, offset + node.nodeSize, { class: 'doc-dim' }));
	});
	return DecorationSet.create(doc, decos);
}

/**
 * The dimmed set for the new state. A selection that stays in its block
 * keeps the old set, and a change inside the caret's block maps it. A new,
 * removed or rewrapped block, or a caret in another block, builds it again.
 */
function nextDim(tr: Transaction, prev: FocusState, current: number): DecorationSet {
	if (!prev.dim) return dimmed(tr.doc, current);
	const sameBlock = current === (prev.current < 0 ? -1 : tr.mapping.map(prev.current));
	if (!sameBlock) return dimmed(tr.doc, current);
	if (!tr.docChanged) return prev.dim;
	const mapped = prev.dim.map(tr.mapping, tr.doc);
	return coversOthers(mapped, tr.doc, current) ? mapped : dimmed(tr.doc, current);
}

/** Whether `set` dims exactly the top-level blocks other than the one at `current`. */
function coversOthers(set: DecorationSet, doc: EditorState['doc'], current: number): boolean {
	const found = set.find();
	if (found.length !== doc.childCount - (current < 0 ? 0 : 1)) return false;
	let i = 0;
	let ok = true;
	doc.forEach((node, offset) => {
		if (!ok || offset === current) return;
		const d = found[i++];
		ok = d.from === offset && d.to === offset + node.nodeSize;
	});
	return ok;
}

function movedBy(tr: Transaction): Moved {
	if (isChangeOrigin(tr) || tr.getMeta('pointer') || tr.getMeta('uiEvent') === 'drop') return null;
	if (tr.docChanged) return 'typed';
	return tr.selectionSet ? 'caret' : null;
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
			// Whether a pointer is down on the page, and until when a selection
			// it made may still arrive. A node view's own `select()` and a
			// link's click handler carry no `pointer` meta, so the meta alone
			// does not tell a click from a key.
			let pointerDown = false;
			let pointerUntil = 0;
			const pointerBusy = () => pointerDown || Date.now() < pointerUntil;
			const pointerUp = () => {
				if (!pointerDown) return;
				pointerDown = false;
				pointerUntil = Date.now() + POINTER_GRACE_MS;
			};
			const pointerStart = () => {
				pointerDown = true;
				return false;
			};
			return [
				new Plugin<FocusState>({
					key: focusKey,
					state: {
						init: (_, state) => {
							const current = currentBlock(state);
							return { on: initial, moved: null, current, dim: initial ? dimmed(state.doc, current) : null };
						},
						apply(tr: Transaction, prev, _old, state) {
							const on = (tr.getMeta(focusKey) as boolean | undefined) ?? prev.on;
							const moved = movedBy(tr);
							const current = currentBlock(state);
							const dim = on ? nextDim(tr, prev, current) : null;
							const same = on === prev.on && moved === prev.moved && current === prev.current && dim === prev.dim;
							return same ? prev : { on, moved, current, dim };
						},
					},
					props: {
						decorations: (state) => focusKey.getState(state)?.dim ?? null,
						handleDOMEvents: {
							pointerdown: pointerStart,
							mousedown: pointerStart,
							touchstart: pointerStart,
						},
					},
					view: () => {
						window.addEventListener('pointerup', pointerUp, true);
						window.addEventListener('mouseup', pointerUp, true);
						window.addEventListener('touchend', pointerUp, true);
						window.addEventListener('pointercancel', pointerUp, true);
						return {
							update(view) {
								const s = focusKey.getState(view.state);
								if (!s?.on || !s.moved || !view.hasFocus()) return;
								if (s.moved === 'caret' && pointerBusy()) return;
								keepCaretInPlace(view);
							},
							destroy() {
								window.removeEventListener('pointerup', pointerUp, true);
								window.removeEventListener('mouseup', pointerUp, true);
								window.removeEventListener('touchend', pointerUp, true);
								window.removeEventListener('pointercancel', pointerUp, true);
							},
						};
					},
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
