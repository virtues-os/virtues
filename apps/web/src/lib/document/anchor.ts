/**
 * Places in a page that hold while a person is choosing what goes there or
 * a writer streams: the link panel or a menu open over the page, the applet
 * list, the mention picker, the file dialog, the inline writer. Nothing the
 * page waits on a server for is held: a paste converts in the page, and a
 * file's block goes in when it is given (`paste.ts`, `media.ts`).
 *
 * Another device's edit, or the assistant's, can land meanwhile, and Tiptap's
 * Yjs binding hands it to the editor as one step that rewrites the whole
 * document. A position mapped through that step lands at the document's
 * edge, and one kept as a number points at whatever text moved under it. An
 * anchor is a Yjs relative position instead: it stays with the text it was
 * taken in, through this device's edits and everyone else's. Anything drawn
 * at a position is placed again from its anchor after such a step (Tiptap's
 * `isChangeOrigin` names them), and so is a selection the binding cannot
 * place itself (`KeptThroughOthersEdits`).
 */

import { Extension } from '@tiptap/core';
import { isChangeOrigin } from '@tiptap/extension-collaboration';
import { GapCursor } from '@tiptap/pm/gapcursor';
import type { Node as PmNode } from '@tiptap/pm/model';
import { Plugin, type EditorState, type Selection, type Transaction } from '@tiptap/pm/state';
import { CellSelection } from '@tiptap/pm/tables';
import type { EditorView } from '@tiptap/pm/view';
import { absolutePositionToRelativePosition, relativePositionToAbsolutePosition, ySyncPluginKey } from '@tiptap/y-tiptap';
import type * as Y from 'yjs';

/** A place in the page that holds through every edit. */
export interface Anchor {
	/** Where it is now; null once the text it was in is gone. */
	resolve(): number | null;
}

/** Two anchors around a stretch of the page. */
export interface AnchoredRange {
	from: Anchor;
	to: Anchor;
}

interface YBinding {
	doc: Y.Doc;
	type: Y.XmlFragment;
	binding: { mapping: Map<unknown, unknown> };
}

/** The editor's Yjs binding, which places an anchor; null for an editor bound to no shared document. */
function bindingOf(state: EditorState): YBinding | null {
	const ys = ySyncPluginKey.getState(state) as YBinding | undefined;
	return ys?.binding ? ys : null;
}

/** Whether the editor is bound to a shared document, where anchors follow other devices' edits. */
export function isBound(state: EditorState): boolean {
	return bindingOf(state) !== null;
}

/**
 * An anchor at `pos` in `state`'s document, which must be the one the editor
 * shows now. It holds to the character before it, so another device's words
 * typed at it land after it. In an editor bound to no shared document nobody
 * else writes, and the anchor is the position as it was.
 */
export function anchorAt(state: EditorState, pos: number): Anchor {
	const ys = bindingOf(state);
	if (!ys) return { resolve: () => pos };
	const mapping = ys.binding.mapping as never;
	const rel = absolutePositionToRelativePosition(pos, ys.type, mapping);
	return { resolve: () => relativePositionToAbsolutePosition(ys.doc, ys.type, rel, mapping) };
}

/** Anchors around `from`..`to`. */
export function anchorRange(state: EditorState, from: number, to: number): AnchoredRange {
	return { from: anchorAt(state, from), to: anchorAt(state, to) };
}

/** Where an anchored range is now; null once either end is gone or they cross. */
export function resolveRange(range: AnchoredRange): { from: number; to: number } | null {
	const from = range.from.resolve();
	const to = range.to.resolve();
	return from === null || to === null || to < from ? null : { from, to };
}

/**
 * A menu's place over the page, held to the text it opened at: the anchor,
 * and where the menu sat from it. A menu over the page is placed by screen
 * coordinates once; held so, it is placed again where that text is now when
 * the page scrolls under it.
 */
export interface Pinned {
	at: Anchor;
	dx: number;
	dy: number;
}

type Coords = { x: number; y: number };

/** Where a position is on screen: the bottom left of the caret there, as menus open under it; null with no layout (a hidden pane). */
function caretCoords(view: EditorView, pos: number): Coords | null {
	try {
		const c = view.coordsAtPos(Math.min(pos, view.state.doc.content.size));
		return { x: c.left, y: c.bottom };
	} catch {
		return null;
	}
}

/** A menu shown at `coords`, opened at `pos` in the page as the editor shows it now. */
export function pinAt(view: EditorView, pos: number, coords: Coords): Pinned {
	const here = caretCoords(view, pos) ?? coords;
	return { at: anchorAt(view.state, pos), dx: coords.x - here.x, dy: coords.y - here.y };
}

/** Where a pinned menu goes now; null once the text it opened at is gone, or with no layout. */
export function placeOf(view: EditorView, pinned: Pinned | null): Coords | null {
	const pos = pinned?.at.resolve();
	if (!pinned || pos == null) return null;
	const here = caretCoords(view, pos);
	return here && { x: here.x + pinned.dx, y: here.y + pinned.dy };
}

const isCell = (node: PmNode | null | undefined) =>
	node?.type.spec.tableRole === 'cell' || node?.type.spec.tableRole === 'header_cell';

/** The cells from the one before `anchor` to the one before `head`; null unless both are cells of one table. */
function cellsAt(doc: PmNode, anchor: number | null, head: number | null): CellSelection | null {
	if (anchor === null || head === null || anchor > doc.content.size || head > doc.content.size) return null;
	const $anchor = doc.resolve(anchor);
	const $head = doc.resolve(head);
	if (!isCell($anchor.nodeAfter) || !isCell($head.nodeAfter) || $anchor.depth < 1) return null;
	return $anchor.node($anchor.depth - 1) === $head.node($head.depth - 1) ? CellSelection.create(doc, anchor, head) : null;
}

/**
 * A selection Tiptap's Yjs binding cannot place again after another
 * device's edit, held by anchors: table cells, or a gap cursor (the place
 * between blocks that typing after a final table lands in). The binding
 * places a selection again as text or as a node only, so selected cells
 * became the text of one cell, and the next key emptied that cell alone,
 * and a gap cursor went into the last cell before it.
 */
type KeptSelection = { cells: { anchor: Anchor; head: Anchor } } | { gap: Anchor };

/** `selection` kept by anchors, when it is one the binding would lose; else null. */
function keepSelection(state: EditorState, selection: Selection): KeptSelection | null {
	if (selection instanceof CellSelection) {
		return { cells: { anchor: anchorAt(state, selection.$anchorCell.pos), head: anchorAt(state, selection.$headCell.pos) } };
	}
	return selection instanceof GapCursor ? { gap: anchorAt(state, selection.head) } : null;
}

/** Where a kept selection is in `doc`; null once it cannot be there. */
function keptSelection(doc: PmNode, kept: KeptSelection): Selection | null {
	if ('cells' in kept) return cellsAt(doc, kept.cells.anchor.resolve(), kept.cells.head.resolve());
	const at = kept.gap.resolve();
	if (at === null || at > doc.content.size) return null;
	// A gap cursor's bookmark resolves to one only where one can be.
	const gap = new GapCursor(doc.resolve(at)).getBookmark().resolve(doc);
	return gap instanceof GapCursor ? gap : null;
}

/**
 * What another device's edit, or the assistant's, leaves as the person had
 * it: selected table cells and a gap cursor, placed again from anchors
 * taken before the edit (`keepSelection`), and the marks waiting to be
 * typed (⌘B with nothing selected), which the binding's one step over the
 * whole document clears, put back while the caret is still a caret. An
 * undo or a redo places the selection it puts back itself.
 */
export const KeptThroughOthersEdits = Extension.create({
	name: 'keptThroughOthersEdits',
	addProseMirrorPlugins() {
		// Taken as the binding takes its own selection, at the start of a Yjs
		// transaction, while the page is as this device last wrote it: one
		// that brings another device's edit (not local), as this device's own
		// writes place nothing again.
		let kept: KeptSelection | null | undefined;
		return [
			new Plugin({
				view(view) {
					const ys = bindingOf(view.state);
					if (!ys) return {};
					const before = (transaction: Y.Transaction) => {
						if (kept !== undefined || transaction.local) return;
						try {
							kept = keepSelection(view.state, view.state.selection);
						} catch {
							// A place the binding cannot anchor: left to the binding.
							kept = null;
						}
					};
					const after = () => {
						kept = undefined;
					};
					ys.doc.on('beforeTransaction', before);
					ys.doc.on('afterAllTransactions', after);
					return {
						destroy() {
							ys.doc.off('beforeTransaction', before);
							ys.doc.off('afterAllTransactions', after);
						},
					};
				},
				appendTransaction(transactions, oldState, newState) {
					const change = transactions.find((tr) => isChangeOrigin(tr));
					const meta = change?.getMeta(ySyncPluginKey) as { isUndoRedoOperation?: boolean } | undefined;
					if (!change || meta?.isUndoRedoOperation) return null;
					let tr: Transaction | null = null;
					const selection = kept ? keptSelection(newState.doc, kept) : null;
					if (selection && !selection.eq(newState.selection)) tr = newState.tr.setSelection(selection);
					const marks = oldState.storedMarks;
					if (marks && !newState.storedMarks && oldState.selection.empty && (tr ?? newState).selection.empty) {
						tr = (tr ?? newState.tr).setStoredMarks(marks);
					}
					// Part of the change it follows, as the binding's own placing is.
					return tr && tr.setMeta(ySyncPluginKey, meta).setMeta('addToHistory', false);
				},
			}),
		];
	},
});
