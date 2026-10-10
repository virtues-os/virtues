/**
 * Proposals: a rewrite the inline writer suggests, held in the page as two
 * marks sharing an id (`proposal`): `proposedDeletion` over the words it
 * would take out and `proposedInsertion` over the words it would put in.
 * Accepting one deletes what it would take out and keeps what it puts in;
 * rejecting does the reverse. Each is one transaction, so one undo step and
 * one update to the shared document. With the caret in one, ⌘⌥Enter accepts
 * it and ⌘⌥Backspace rejects it (`proposalKeys`), as the SuggestionBar's
 * buttons do.
 *
 * The eye tells the halves apart by colour and strikethrough; a screen
 * reader by role (`ProposalsRead`), and it is told when the caret comes into
 * one, and the keys that settle it (`proposalSaid`).
 */

import { Extension, type Editor } from '@tiptap/core';
import { isChangeOrigin } from '@tiptap/extension-collaboration';
import { DOMSerializer, Fragment, Slice, type Mark as PmMark, type MarkType, type Node as PmNode } from '@tiptap/pm/model';
import { EditorState, Plugin, type Transaction } from '@tiptap/pm/state';
import type { EditorView } from '@tiptap/pm/view';
import { ySyncPluginKey } from '@tiptap/y-tiptap';
import { newId } from './schema';

export const DELETION = 'proposedDeletion';
export const INSERTION = 'proposedInsertion';
/** The attribute that ties a proposal's two halves together. */
export const PROPOSAL_ATTR = 'proposal';

export interface Proposal {
	id: string;
	/** Whether it takes words out, puts words in, or both. */
	deletes: boolean;
	inserts: boolean;
}

interface Range {
	from: number;
	to: number;
}

/** The proposal marks this schema has; none until the contract defines them. */
function markTypes(state: EditorState): { del: MarkType; ins: MarkType } | null {
	const del = state.schema.marks[DELETION];
	const ins = state.schema.marks[INSERTION];
	return del && ins ? { del, ins } : null;
}

/** The proposal at `pos`: the one a mark on the text before or after the position belongs to. */
export function proposalAt(state: EditorState, pos: number): Proposal | null {
	const types = markTypes(state);
	if (!types) return null;
	const $pos = state.doc.resolve(pos);
	const marks = [...($pos.nodeBefore?.marks ?? []), ...($pos.nodeAfter?.marks ?? [])];
	const mark = marks.find((m) => m.type === types.del || m.type === types.ins);
	if (!mark) return null;
	const id = String(mark.attrs[PROPOSAL_ATTR]);
	return describe(state.doc, types, id);
}

function describe(doc: PmNode, types: { del: MarkType; ins: MarkType }, id: string): Proposal {
	return {
		id,
		deletes: rangesOf(doc, types.del, id).length > 0,
		inserts: rangesOf(doc, types.ins, id).length > 0,
	};
}

/** Every proposal id in the page, in document order. */
export function proposalIds(state: EditorState): string[] {
	const types = markTypes(state);
	if (!types) return [];
	const ids: string[] = [];
	state.doc.descendants((node) => {
		for (const m of node.marks) {
			if ((m.type === types.del || m.type === types.ins) && !ids.includes(String(m.attrs[PROPOSAL_ATTR]))) {
				ids.push(String(m.attrs[PROPOSAL_ATTR]));
			}
		}
		return true;
	});
	return ids;
}

/** The text ranges carrying `type` with proposal `id`, adjacent runs joined. */
function rangesOf(doc: PmNode, type: MarkType, id: string): Range[] {
	const out: Range[] = [];
	doc.descendants((node, pos) => {
		if (!node.isInline) return true;
		const has = node.marks.some((m) => m.type === type && String(m.attrs[PROPOSAL_ATTR]) === id);
		if (!has) return false;
		const last = out[out.length - 1];
		if (last && last.to === pos) last.to = pos + node.nodeSize;
		else out.push({ from: pos, to: pos + node.nodeSize });
		return false;
	});
	return out;
}

/**
 * Textblocks every piece of whose content lies in `ranges`: deleting the
 * ranges would leave them empty, so they go whole, as the writer meant.
 */
function emptiedBlocks(doc: PmNode, ranges: Range[]): Range[] {
	const out: Range[] = [];
	doc.descendants((node, pos) => {
		if (!node.isTextblock) return true;
		if (node.content.size === 0) return false;
		const start = pos + 1;
		const end = pos + 1 + node.content.size;
		let covered = 0;
		for (const r of ranges) covered += Math.max(0, Math.min(r.to, end) - Math.max(r.from, start));
		if (covered >= node.content.size) out.push({ from: pos, to: pos + node.nodeSize });
		return false;
	});
	return out;
}

/** In `tr`: delete the `remove` half of proposal `id` and unmark the `keep` half. */
function settle(tr: Transaction, remove: MarkType, keep: MarkType, id: string): void {
	const doc = tr.doc;
	for (const r of rangesOf(doc, keep, id)) {
		tr.removeMark(r.from, r.to, keep.create({ [PROPOSAL_ATTR]: id }));
	}
	const removed = rangesOf(tr.doc, remove, id);
	const blocks = emptiedBlocks(tr.doc, removed);
	const inBlock = (r: Range) => blocks.some((b) => r.from >= b.from && r.to <= b.to);
	const cuts = [...blocks.map((b) => ({ ...b, block: true })), ...removed.filter((r) => !inBlock(r)).map((r) => ({ ...r, block: false }))];
	cuts.sort((a, b) => b.from - a.from);
	for (const cut of cuts) {
		if (cut.block) tr.deleteRange(cut.from, cut.to);
		else tr.delete(cut.from, cut.to);
	}
}

/**
 * `doc` as rejecting proposal `id` would leave it. A writer checks a change
 * against it: what a reject cannot take back (a node the marks cannot hold)
 * must not be part of a proposal.
 */
export function withProposalRejected(doc: PmNode, id: string): PmNode {
	const state = EditorState.create({ doc });
	const types = markTypes(state);
	if (!types) return doc;
	const tr = state.tr;
	settle(tr, types.ins, types.del, id);
	return tr.doc;
}

function resolve(editor: Editor, ids: string[], accept: boolean): boolean {
	const types = markTypes(editor.state);
	if (!types || !ids.length) return false;
	const tr = editor.state.tr;
	for (const id of ids) {
		if (accept) settle(tr, types.del, types.ins, id);
		else settle(tr, types.ins, types.del, id);
	}
	if (!tr.docChanged) return false;
	editor.view.dispatch(tr);
	return true;
}

/** Take proposal `id`: what it takes out goes, what it puts in stays. */
export function acceptProposal(editor: Editor, id: string): boolean {
	return resolve(editor, [id], true);
}

/** Refuse proposal `id`: what it puts in goes, what it takes out stays. */
export function rejectProposal(editor: Editor, id: string): boolean {
	return resolve(editor, [id], false);
}

export function acceptAllProposals(editor: Editor): boolean {
	return resolve(editor, proposalIds(editor.state), true);
}

export function rejectAllProposals(editor: Editor): boolean {
	return resolve(editor, proposalIds(editor.state), false);
}

/** Accept or reject the proposal at the caret from the keyboard: ⌘⌥Enter and ⌘⌥Backspace. */
export function proposalKeys(): Extension {
	const at = (editor: Editor) =>
		editor.isEditable && editor.state.selection.empty ? proposalAt(editor.state, editor.state.selection.head) : null;
	return Extension.create({
		name: 'proposalKeys',
		addKeyboardShortcuts() {
			return {
				'Mod-Alt-Enter': ({ editor }) => {
					const p = at(editor);
					return p ? acceptProposal(editor, p.id) : false;
				},
				'Mod-Alt-Backspace': ({ editor }) => {
					const p = at(editor);
					return p ? rejectProposal(editor, p.id) : false;
				},
			};
		},
	});
}

/** What a screen reader is told when the caret comes into proposal `p`: what it would do, and the keys that settle it. */
export function proposalSaid(p: Proposal, keys: { accept: string; reject: string }): string {
	const what = p.deletes && p.inserts ? 'a rewrite' : p.deletes ? 'a deletion' : 'an insertion';
	return `Suggested edit, ${what}. ${keys.accept} accepts it, ${keys.reject} rejects it.`;
}

const READ_AS: Record<string, { role: string; 'aria-roledescription': string }> = {
	[DELETION]: { role: 'deletion', 'aria-roledescription': 'suggested deletion' },
	[INSERTION]: { role: 'insertion', 'aria-roledescription': 'suggested insertion' },
};

/** A proposal half's element as the schema draws it, with the role a screen reader reads. */
function readAs(mark: PmMark, _view: EditorView, inline: boolean) {
	const spec = mark.type.spec.toDOM?.(mark, inline);
	if (!spec) throw new Error(`contract: ${mark.type.name} draws nothing`);
	const { dom, contentDOM } = DOMSerializer.renderSpec(document, spec);
	for (const [name, value] of Object.entries(READ_AS[mark.type.name])) (dom as HTMLElement).setAttribute(name, value);
	return { dom, contentDOM };
}

/**
 * Proposals as a screen reader reads them: the words a proposal would take
 * out are a deletion and the words it would put in an insertion, as `<del>`
 * and `<ins>` are, each said to be suggested. Mark views, so the editor
 * draws them and neither the page's HTML nor the shared document holds them.
 */
export const ProposalsRead = Extension.create({
	name: 'proposalsRead',
	addProseMirrorPlugins() {
		return [
			new Plugin({
				props: {
					markViews: { [DELETION]: readAs, [INSERTION]: readAs },
				},
			}),
		];
	},
});

/** `slice` with the marks of every node in it as `change` makes them. */
function remarked(slice: Slice, change: (marks: readonly PmMark[]) => readonly PmMark[]): Slice {
	const walk = (fragment: Fragment): Fragment => {
		const out: PmNode[] = [];
		fragment.forEach((node) => {
			const marks = change(node.marks);
			out.push(node.isText ? node.mark(marks) : node.copy(walk(node.content)).mark(marks));
		});
		return Fragment.from(out);
	};
	return new Slice(walk(slice.content), slice.openStart, slice.openEnd);
}

/**
 * `slice` with each proposal in it under a new id, its two halves still
 * sharing one. A proposal copied and pasted is a second proposal: under the
 * id of the one it was copied from, accepting or rejecting either would
 * settle both, rewriting words the person never acted on.
 */
export function freshProposals(slice: Slice, del: MarkType, ins: MarkType): Slice {
	const ids = new Map<string, string>();
	const fresh = (id: unknown) => {
		const key = String(id);
		if (!ids.has(key)) ids.set(key, newId());
		return ids.get(key)!;
	};
	return remarked(slice, (marks) =>
		marks.map((m) =>
			m.type === del || m.type === ins ? m.type.create({ ...m.attrs, [PROPOSAL_ATTR]: fresh(m.attrs[PROPOSAL_ATTR]) }) : m,
		),
	);
}

/** Where the drop being put in lands, while its own event runs: plain text dropped takes the marks there. */
let dropAt: number | null = null;

/**
 * What a paste or a drop brings into the page holds its proposals under new
 * ids (`freshProposals`), and takes no deletion from where it lands.
 * ProseMirror gives plain text the marks at the caret: pasted in the middle
 * of words a proposal would delete, the person's words would be a deletion
 * of their own, which Accept all would delete.
 */
export const ProposalsPasted = Extension.create({
	name: 'proposalsPasted',
	addProseMirrorPlugins() {
		return [
			new Plugin({
				props: {
					handleDOMEvents: {
						drop(view, event) {
							try {
								dropAt = view.posAtCoords({ left: event.clientX, top: event.clientY })?.pos ?? null;
							} catch {
								dropAt = null;
							}
							queueMicrotask(() => (dropAt = null));
							return false;
						},
					},
					transformPasted(slice, view) {
						const types = markTypes(view.state);
						if (!types) return slice;
						const at = Math.min(dropAt ?? view.state.selection.from, view.state.doc.content.size);
						const there = view.state.doc.resolve(at).marks().filter((m) => m.type === types.del);
						const own = there.length ? remarked(slice, (marks) => marks.filter((m) => !there.some((t) => t.eq(m)))) : slice;
						return freshProposals(own, types.del, types.ins);
					},
				},
			}),
		];
	},
});

/**
 * What the person types in words a proposal would delete is their own
 * words, never part of the deletion, which Accept would take them with. A
 * proposal's marks are not inclusive (`views.ts`), which keeps them off
 * text typed at its edge; inside it, typing takes the marks of the text
 * around the caret. So wherever the caret or a selection is in a deletion,
 * the marks typing takes (the stored marks) are those without it. Words
 * typed in an insertion edit what it would put in, and stay part of it.
 */
export const TypingOutsideDeletions = Extension.create({
	name: 'typingOutsideDeletions',
	addProseMirrorPlugins() {
		return [
			new Plugin({
				appendTransaction(transactions, _old, state) {
					const types = markTypes(state);
					if (!types) return null;
					const { selection } = state;
					const marks =
						state.storedMarks ?? (selection.empty ? selection.$from.marks() : selection.$from.marksAcross(selection.$to)) ?? [];
					const own = marks.filter((m) => m.type !== types.del);
					if (own.length === marks.length) return null;
					const tr = state.tr.setStoredMarks(own);
					// After another device's edit, part of it, as the binding's own placing is.
					const change = transactions.find((t) => isChangeOrigin(t));
					return change ? tr.setMeta(ySyncPluginKey, change.getMeta(ySyncPluginKey)).setMeta('addToHistory', false) : tr;
				},
			}),
		];
	},
});
