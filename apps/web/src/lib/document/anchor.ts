/**
 * Places in a page that hold while something waits on them: the link panel
 * or a menu open over the page, the applet list, the mention picker, the
 * file dialog, an upload, the inline writer.
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
import type { Fragment, Node as PmNode } from '@tiptap/pm/model';
import { AllSelection, NodeSelection, Plugin, TextSelection, type EditorState, type Selection, type Transaction } from '@tiptap/pm/state';
import { CellSelection } from '@tiptap/pm/tables';
import { Decoration, type EditorView } from '@tiptap/pm/view';
import {
	absolutePositionToRelativePosition,
	relativePositionToAbsolutePosition,
	ySyncPluginKey,
	yUndoPluginKey,
} from '@tiptap/y-tiptap';
import * as Y from 'yjs';

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
 * shows now. In an editor bound to no shared document nobody else writes, and
 * the anchor is the position as it was.
 *
 * In text, `assoc` says which character it holds to: the one before it (-1),
 * so another device's words typed at it land after it, or the one after it
 * (1), so they land before it. The start of a stretch held through a wait
 * holds to its first character: words typed just before the stretch stay
 * outside it, as this device's own do (`holdRangeThrough`).
 */
export function anchorAt(state: EditorState, pos: number, assoc: -1 | 1 = -1): Anchor {
	const ys = bindingOf(state);
	if (!ys) return { resolve: () => pos };
	const mapping = ys.binding.mapping as never;
	const rel = absolutePositionToRelativePosition(pos, ys.type, mapping);
	const right = assoc > 0 ? heldToTheRight(ys.doc, rel) : null;
	if (!right) return { resolve: () => relativePositionToAbsolutePosition(ys.doc, ys.type, rel, mapping) };
	return {
		resolve: () => {
			const at = Y.createAbsolutePositionFromRelativePosition(right, ys.doc);
			if (!at) return null;
			// y-tiptap places positions it made, which hold to the left: the
			// same place, said that way.
			const left = Y.createRelativePositionFromTypeIndex(at.type, at.index, -1);
			return relativePositionToAbsolutePosition(ys.doc, ys.type, left, mapping);
		},
	};
}

/**
 * `rel`, a place in text as y-tiptap makes one (held to the character
 * before it), held to the character after it instead; null where there is
 * none to hold to (not in text, or at the end of a run of it).
 */
function heldToTheRight(doc: Y.Doc, rel: Y.RelativePosition): Y.RelativePosition | null {
	const at = Y.createAbsolutePositionFromRelativePosition(rel, doc);
	if (!at || !(at.type instanceof Y.XmlText) || at.index >= at.type.length) return null;
	return Y.createRelativePositionFromTypeIndex(at.type, at.index, 0);
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
 * A place something waits at (an upload's placeholder, a paste's marker), as
 * a plugin holds it: its anchor, and where it is in the document the editor
 * shows now. The position is mapped through this device's edits and placed
 * again from the anchor after another device's. A decoration is no such
 * holder: mapping drops a widget at the start of a block an edit re-marks,
 * as UniqueID re-marks the second half of a split paragraph to give it its
 * own id, and the place would go with it.
 */
export interface Held {
	at: Anchor;
	/** Where it is now; null once the text it was in is gone. */
	pos: number | null;
}

/** A place held at `pos` in `state`'s document, which must be the one the editor shows now; `assoc` as `anchorAt` takes it. */
export function hold(state: EditorState, pos: number, assoc: -1 | 1 = -1): Held {
	return { at: anchorAt(state, pos, assoc), pos };
}

/**
 * `held` after `tr`. This device's edit maps it, `assoc` saying which side
 * of text typed at it it stays on; once the text it was in is deleted
 * around it, its position is gone. Another device's edit rewrites the whole
 * document, so it is placed again from its anchor.
 */
export function holdThrough(tr: Transaction, held: Held, assoc: -1 | 1): Held {
	if (isChangeOrigin(tr)) {
		const at = held.at.resolve();
		return { at: held.at, pos: at === null ? null : Math.min(at, tr.doc.content.size) };
	}
	if (held.pos === null || !tr.docChanged) return held;
	const mapped = tr.mapping.mapResult(held.pos, assoc);
	return { at: held.at, pos: mapped.deletedAcross ? null : mapped.pos };
}

/**
 * Where a held place is in the page now: its position; else, in a shared
 * document, its anchor, which stays where the deleted text was; null when
 * it has neither.
 */
export function heldPos(state: EditorState, held: Held): number | null {
	if (held.pos !== null) return held.pos;
	if (!isBound(state)) return null;
	const at = held.at.resolve();
	return at === null ? null : Math.min(at, state.doc.content.size);
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

/** A held selection: the words, the block or the table cells something waiting will replace. */
export interface HeldRange {
	from: Held;
	to: Held;
	/** A block was selected. */
	node: boolean;
	/**
	 * Table cells were selected: the places before the cell the selection
	 * started in and the one it ends in. Every cell between goes, not only
	 * the words from `from` to `to`.
	 */
	cells?: { anchor: Held; head: Held };
	/**
	 * What it holds: as it was taken, and as this device's own edits since
	 * left it. It goes only while it holds that still (`holdsAsTaken`).
	 */
	held?: Fragment;
}

/**
 * The selection `selection` as a held range; none when it is empty. Its
 * start holds to the first character in it, so words typed just before it,
 * here or on another device, stay outside it.
 */
export function holdSelection(state: EditorState, selection: Selection): HeldRange | undefined {
	if (selection.empty) return undefined;
	const range: HeldRange =
		selection instanceof CellSelection
			? {
					from: hold(state, Math.min(...selection.ranges.map((r) => r.$from.pos)), 1),
					to: hold(state, Math.max(...selection.ranges.map((r) => r.$to.pos))),
					node: false,
					cells: { anchor: hold(state, selection.$anchorCell.pos), head: hold(state, selection.$headCell.pos) },
				}
			: { from: hold(state, selection.from, 1), to: hold(state, selection.to), node: selection instanceof NodeSelection };
	range.held = heldSelection(state, range)?.content().content;
	return range;
}

/**
 * Whether `selection`, a held range where it is now (`heldSelection`),
 * holds what it held (`HeldRange.held`). Another device's words typed into
 * it, or another device's change to what it holds, leave it standing: what
 * was waiting to replace it would take words nobody here chose to replace,
 * and their writer would never know.
 */
export function holdsAsTaken(range: HeldRange, selection: Selection): boolean {
	return !range.held || selection.content().content.eq(range.held);
}

/** `range` after `tr`, each end kept inside it, what this device's own edit wrote in it part of what it holds. */
export function holdRangeThrough(tr: Transaction, range: HeldRange): HeldRange {
	const held: HeldRange = { ...range, from: holdThrough(tr, range.from, 1), to: holdThrough(tr, range.to, -1) };
	if (range.cells) {
		held.cells = { anchor: holdThrough(tr, range.cells.anchor, 1), head: holdThrough(tr, range.cells.head, 1) };
	}
	if (range.held && tr.docChanged && !isChangeOrigin(tr)) {
		held.held = selectionIn(tr.doc, held, (h) => h.pos)?.content().content ?? range.held;
	}
	return held;
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
 * The selection a held range is in `state`'s document as it stands when
 * what waited lands; null once it holds nothing. Cells are the cells they
 * were, every one of them, while they are still cells of one table.
 */
export function heldSelection(state: EditorState, range: HeldRange): Selection | null {
	return selectionIn(state.doc, range, (held) => heldPos(state, held));
}

/** The selection `range` is in `doc`, each of its places where `at` says; null once it holds nothing. */
function selectionIn(doc: PmNode, range: HeldRange, at: (held: Held) => number | null): Selection | null {
	if (range.cells) return cellsAt(doc, at(range.cells.anchor), at(range.cells.head));
	const from = at(range.from);
	const to = at(range.to);
	if (from === null || to === null || to <= from || to > doc.content.size) return null;
	if (range.node) {
		const node = doc.nodeAt(from);
		if (node && from + node.nodeSize === to) return NodeSelection.create(doc, from);
	}
	if (from === 0 && to === doc.content.size) return new AllSelection(doc);
	return TextSelection.between(doc.resolve(from), doc.resolve(to));
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

/** What a held range draws while it waits: its words, or each of its cells' words, as going. */
export function goingDecorations(doc: PmNode, range: HeldRange | undefined): Decoration[] {
	if (!range) return [];
	const spans = range.cells
		? (cellsAt(doc, range.cells.anchor.pos, range.cells.head.pos)?.ranges ?? []).map((r) => ({ from: r.$from.pos, to: r.$to.pos }))
		: [{ from: range.from.pos, to: range.to.pos }];
	return spans
		.filter((s): s is { from: number; to: number } => s.from !== null && s.to !== null && s.to > s.from && s.to <= doc.content.size)
		.map((s) => Decoration.inline(s.from, s.to, { class: 'doc-paste-replaced' }));
}

let waits = 0;

/**
 * The order things start waiting at held places in, pastes and uploads
 * alike: Undo calls off the newest of them first.
 */
export function nextWait(): number {
	return ++waits;
}

/**
 * Dispatch what lands at a held place as an undo step of its own. It lands
 * when a server answers, not when the person acted, so within y-undo's
 * capture window of their typing elsewhere it would join that typing, and
 * undoing the typing would take it too.
 */
export function landApart(view: EditorView, tr: Transaction): void {
	const manager = (yUndoPluginKey.getState(view.state) as { undoManager?: Y.UndoManager } | undefined)?.undoManager;
	manager?.stopCapturing();
	view.dispatch(tr);
	manager?.stopCapturing();
}
