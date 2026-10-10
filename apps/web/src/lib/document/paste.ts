/**
 * Paste into a block page.
 *
 * - In a code block: the text as it is, never parsed, with Windows' CRLF
 *   line ends made plain ones.
 * - HTML: the contract's parse rules, plus the inline styles Google Docs and
 *   Word use for bold, italic, underline and strike (`PASTE` in schema.ts).
 *   ProseMirror's own paste does this. Word writes a list as paragraphs
 *   marked `mso-list`, each with its bullet or number as text before its
 *   words; those are made a list again first (`wordLists`). A code editor's HTML
 *   is the exception (`isCodeEditorHtml`): VS Code copies its syntax
 *   colours as styled spans, and a token's bold is no writer's bold, so its
 *   text is pasted instead, as markdown when it reads as markdown. Where the
 *   browser hands over VS Code's own data, a markdown file's text is
 *   converted and any other language's goes to Tiptap's code block paste.
 * - Plain text that reads as markdown: converted by the server, the one
 *   converter, so a paste reads markdown exactly as the server's own
 *   writers do. Only a block's sign counts as markdown (a heading, a list, a
 *   quote, a fence, a table row): a line of code holds `==`, `**` and `](`
 *   often enough, and the converter would read `<Tag>` in it as a tag. The
 *   words of a line are formatted by the editor's own paste rules (bold,
 *   italic, code, strike, highlight, links), which read emphasis as
 *   CommonMark does, so `a == b` and `2 * 3 * 4` stay as they are, and run
 *   on plain text only, never in inline code (`delimited` in schema.ts).
 *   The converter takes a paste as text (`convertPastedMarkdown`): a tag
 *   the page cannot hold, or CriticMarkup, stays in it as written. It lands
 *   where the paste was, wherever edits made meanwhile have moved that,
 *   another device's included: the spot is held by an anchor
 *   (`anchor.ts`), as an upload's is. If that text is gone by the time the
 *   server answers, it lands at the caret, and says so. If the server cannot convert it, or converts
 *   it to nothing, the text is pasted as it is. A selection the paste
 *   replaces stays, drawn as going, until the paste lands, and goes in the
 *   same edit: one undo takes the paste back and puts the selection back,
 *   however long the server took. If its words changed meanwhile (someone
 *   typed into them on another device), they stay, the paste lands before
 *   them, and the person is told. That edit is an undo step of its own,
 *   apart from typing done meanwhile. Pastes waiting at one place land
 *   there in the order they were made, whichever the server answers first.
 *   Undo while a paste waits calls it off (`callOffPaste`): its marker
 *   goes, the selection it would have replaced stays, and nothing lands
 *   when the server answers.
 * - Over selected table cells: each cell takes the text, as prosemirror-tables
 *   pastes text into cells; it is not converted.
 * - Dropped text is read as the same text pasted: plain text that reads as
 *   markdown is converted and lands where it was dropped; anything else is
 *   ProseMirror's drop, whose words the paste rules read as a paste's
 *   (`dropping` in schema.ts).
 *
 * What the converter makes lands as Notion and Google Docs land a paste
 * (`placeConverted`): one paragraph is a run of words and joins the text at
 * the caret; anything else arrives as whole blocks, never merged into the
 * paragraph it was pasted into.
 */

import { Extension } from '@tiptap/core';
import { DOMParser as PmDOMParser, Fragment, Slice, type Node as PmNode, type Schema } from '@tiptap/pm/model';
import { Plugin, PluginKey, TextSelection, type EditorState, type Selection, type Transaction } from '@tiptap/pm/state';
import { CellSelection } from '@tiptap/pm/tables';
import { canJoin, canSplit } from '@tiptap/pm/transform';
import { Decoration, DecorationSet, type EditorView } from '@tiptap/pm/view';
import { toast } from 'svelte-sonner';
import { convertPastedMarkdown } from '$lib/api/client';
import {
	anchorAt,
	goingDecorations,
	heldPos,
	heldSelection,
	hold,
	holdRangeThrough,
	holdSelection,
	holdThrough,
	holdsAsTaken,
	landApart,
	nextWait,
	type Anchor,
	type Held,
	type HeldRange,
} from './anchor';
import { contract } from './schema';

export type Convert = (markdown: string) => Promise<{ html: string }>;

/** A block's markdown sign at the start of a line: what makes pasted text markdown. */
const MARKDOWN_SIGNS = [/^#{1,6} /m, /^ {0,3}[-*+] /m, /^ {0,3}\d+\. /m, /^ {0,3}> /m, /^ {0,3}(```|~~~)/m, /^\|.*\|\s*$/m];

/** Signs of markdown inside a line: bold, a link, a highlight. */
const INLINE_SIGNS = [/\*\*/, /\]\(/, /==/];

/**
 * Whether text reads as markdown rather than plain words: a line starts
 * with a block's sign. With `inline`, for text a model wrote as markdown
 * (the inline writer's output), a sign inside a line counts too.
 */
export function looksLikeMarkdown(text: string, o: { inline?: boolean } = {}): boolean {
	return MARKDOWN_SIGNS.some((re) => re.test(text)) || (!!o.inline && INLINE_SIGNS.some((re) => re.test(text)));
}

/**
 * Whether clipboard HTML is a code editor's rendering of plain text: one
 * `white-space: pre` block of divs and styled spans, as VS Code and Cursor
 * copy their syntax colours. Read in an inert document: nothing in a
 * paste's HTML loads or runs.
 */
export function isCodeEditorHtml(html: string): boolean {
	const body = new DOMParser().parseFromString(html, 'text/html').body;
	const roots = [...body.children].filter((el) => el.tagName !== 'META' && el.tagName !== 'STYLE');
	if (roots.length !== 1) return false;
	const root = roots[0];
	if (root.tagName !== 'DIV' || !/(^|;)\s*white-space\s*:\s*pre/i.test(root.getAttribute('style') ?? '')) return false;
	return [...root.querySelectorAll('*')].every((el) => el.tagName === 'DIV' || el.tagName === 'SPAN' || el.tagName === 'BR');
}

/** Word's list paragraph: the list it belongs to and its level (`mso-list:l0 level2 lfo1`). */
const MSO_LIST = /mso-list:\s*(l\d+)\s+level(\d+)/i;
/** A marker that numbers (`1.`, `a)`, `iv.`, `(2)`); any other (`·`, `o`, `§`) is a bullet. */
const NUMBERED = /^\(?(\d+|[a-z]|[ivxlcdm]+)[.)]$/i;

/**
 * Word's lists as lists. Word writes each item as a paragraph styled
 * `mso-list`, its level in the style and its bullet or number as text in a
 * span styled `mso-list:Ignore`, between conditional comments that every
 * HTML parser keeps the text of. Read as they come, the items were
 * paragraphs starting with `·` or `1.` and a run of no-break spaces, which
 * the page's markdown, search and the chat read too. Each run of them is
 * made a `ul` or an `ol` (a number for a marker, a bullet for anything
 * else), nested by level, its marker gone; an `ol` starts at its first
 * number. Other HTML is returned as it came. Read in an inert document:
 * nothing in a paste's HTML loads or runs.
 */
export function wordLists(html: string): string {
	if (!/mso-list/i.test(html)) return html;
	const doc = new DOMParser().parseFromString(html, 'text/html');
	const items = [...doc.body.querySelectorAll<HTMLElement>('p')].filter((el) => MSO_LIST.test(el.getAttribute('style') ?? ''));
	if (!items.length) return html;
	// An item continues the lists open before it when it directly follows the last one.
	const follows = items.map((el, i) => i > 0 && el.previousElementSibling === items[i - 1]);
	let open: { level: number; id: string; ordered: boolean; list: HTMLElement }[] = [];
	items.forEach((el, i) => {
		const [, id, depth] = MSO_LIST.exec(el.getAttribute('style') ?? '')!;
		const level = Number(depth);
		const marker = [...el.querySelectorAll('span')].find((span) => /mso-list:\s*ignore/i.test(span.getAttribute('style') ?? ''));
		const mark = (marker?.textContent ?? '').replace(/[\s\u00a0]+/g, '');
		marker?.remove();
		const ordered = NUMBERED.test(mark);
		if (!follows[i]) open = [];
		while (open.length && open[open.length - 1].level > level) open.pop();
		let top = open.at(-1);
		if (top && top.level === level && (top.ordered !== ordered || top.id !== id)) {
			open.pop();
			top = open.at(-1);
		}
		if (!top || top.level < level) {
			const list = doc.createElement(ordered ? 'ol' : 'ul');
			const start = /^\(?(\d+)/.exec(mark);
			if (ordered && start && Number(start[1]) !== 1) list.setAttribute('start', start[1]);
			if (top) top.list.lastElementChild!.append(list);
			else el.before(list);
			top = { level, id, ordered, list };
			open.push(top);
		}
		const item = doc.createElement('li');
		const words = doc.createElement('p');
		words.append(...el.childNodes);
		item.append(words);
		top.list.append(item);
		el.remove();
	});
	// The pasted styles stay: ProseMirror reads their rules into the elements they style.
	const styles = [...doc.head.querySelectorAll('style')].map((style) => style.outerHTML).join('');
	return styles + doc.body.innerHTML;
}

/** The language VS Code says it copied from, when the browser hands its data over. */
function vscodeMode(data: DataTransfer): string | null {
	const raw = data.getData('vscode-editor-data');
	if (!raw) return null;
	try {
		const mode = (JSON.parse(raw) as { mode?: unknown }).mode;
		return typeof mode === 'string' ? mode : '';
	} catch {
		return '';
	}
}

/** Plain text as paragraphs, one per line. */
export function plainSlice(schema: Schema, text: string): Slice {
	const lines = text.replace(/\r\n?/g, '\n').split('\n');
	if (lines.length === 1) return new Slice(lines[0] ? Fragment.from(schema.text(lines[0])) : Fragment.empty, 0, 0);
	const paragraphs: PmNode[] = lines.map((line) =>
		schema.nodes.paragraph.create(null, line ? schema.text(line) : null),
	);
	return new Slice(Fragment.from(paragraphs), 1, 1);
}

/** HTML as the slice a paste of it would insert. */
export function htmlSlice(schema: Schema, html: string): Slice {
	const div = document.createElement('div');
	div.innerHTML = html;
	return PmDOMParser.fromSchema(schema).parseSlice(div);
}

function isList(node: PmNode): boolean {
	return node.type.isInGroup('list');
}

/** Join the lists either side of `pos` when they are the same kind. */
function joinLists(tr: Transaction, pos: number): void {
	const $pos = tr.doc.resolve(pos);
	const before = $pos.nodeBefore;
	const after = $pos.nodeAfter;
	if (before && after && before.type === after.type && isList(before) && canJoin(tr.doc, pos)) tr.join(pos);
}

/**
 * Put what the converter made at `at`.
 *
 * One paragraph is a run of words: it joins the text at `at`. Anything else
 * is whole blocks. They land beside the textblock `at` is in, which is split
 * at `at` unless `at` is at its start or end, and outside the lists around
 * it, which are split the same way, so a heading never lands in a list item
 * the caret was at the top level of. An empty line there gives way to them,
 * and a pasted list joins a list of its kind it meets. The caret, if it was
 * at `at`, ends after them. Returns where they end.
 */
export function placeConverted(tr: Transaction, at: number, content: Fragment): number {
	const caretHere = tr.selection.empty && tr.selection.from === at;
	const first = content.firstChild;
	if (!first) return at;
	const $at = tr.doc.resolve(at);
	const steps = tr.steps.length;
	if (first.isInline || (content.childCount === 1 && first.type.name === 'paragraph')) {
		tr.replaceRange(at, at, first.isInline ? new Slice(content, 0, 0) : new Slice(content, 1, 1));
		return tr.mapping.slice(steps).map(at, 1);
	}
	if (!$at.parent.isTextblock) {
		tr.replaceRange(at, at, new Slice(content, 0, 0));
		return tr.mapping.slice(steps).map(at, 1);
	}
	// The node the blocks land in: the textblock's parent, or, when that is
	// a list item, whatever holds the list, as many lists out as there are.
	let depth = $at.depth - 1;
	while (depth > 0 && isList($at.node(depth - 1))) depth -= 2;
	// An empty line, with the items and lists that hold nothing else.
	let line: { from: number; to: number } | null = null;
	if ($at.parent.content.size === 0) {
		let d = $at.depth;
		while (d - 1 > depth && $at.node(d - 1).childCount === 1) d--;
		line = { from: $at.before(d), to: $at.after(d) };
	}
	// Out to that node, splitting each node on the way where `pos` is inside
	// it rather than at an edge. The second half of a split is a new block,
	// so it gets a new id, and a numbered list's goes on counting from the
	// items before it.
	let pos = at;
	let split = false;
	for (let d = $at.depth; d > depth; d--) {
		const $pos = tr.doc.resolve(pos);
		const node = $pos.parent;
		const atStart = node.isTextblock ? $pos.parentOffset === 0 : $pos.index() === 0;
		const atEnd = node.isTextblock ? $pos.parentOffset === node.content.size : $pos.index() === node.childCount;
		const attrs: Record<string, unknown> = { ...node.attrs };
		if (contract.id.attr in attrs) attrs[contract.id.attr] = null;
		if ('start' in attrs && node.type.name === 'orderedList') attrs.start = (Number(node.attrs.start) || 1) + $pos.index();
		const after = [{ type: node.type, attrs }];
		if (atEnd) {
			pos = $pos.after();
		} else if (atStart) {
			pos = $pos.before();
		} else if (canSplit(tr.doc, pos, 1, after)) {
			tr.split(pos, 1, after);
			pos += 1;
			split = true;
		} else if (!split) {
			// A list item whose paragraph is followed by a list cannot split
			// there: the caret was at the end of its line, so past it whole,
			// its own items with it.
			pos = $pos.after();
		} else {
			// The caret was in an item nested in this one, now split around
			// it: the blocks stay in this item, among its items.
			break;
		}
	}
	tr.insert(pos, content);
	// Where the blocks start and end, mapped through what follows.
	const placed = tr.steps.length;
	const edge = (p: number) => tr.mapping.slice(placed).map(p, -1);
	const [start, end] = [pos, pos + content.size];
	if (line) {
		const from = tr.mapping.slice(steps).map(line.from, 1);
		const to = tr.mapping.slice(steps).map(line.to, -1);
		const $from = tr.doc.resolve(from);
		if ($from.parent.canReplace($from.index(), $from.index() + 1)) tr.delete(from, to);
	}
	joinLists(tr, edge(end));
	joinLists(tr, edge(start));
	if (caretHere) tr.setSelection(TextSelection.near(tr.doc.resolve(edge(end)), -1));
	return edge(end);
}

interface PendingPaste {
	id: string;
	/** When it started waiting, among pastes and uploads (`nextWait`). */
	order: number;
	/** Where the paste lands. */
	at: Held;
	/** The selection the paste replaces, taken out when it lands. */
	replaces?: HeldRange;
}

type PasteMeta =
	| { add: PendingPaste }
	/** A paste landed; `follow` are the pastes made after it at the same place, which go after it, at `pos`. */
	| { remove: string; follow?: { ids: string[]; pos: number } }
	/** The place taken again as an anchor, once the document it is in is the page's. */
	| { anchor: { id: string; at: Anchor } };

interface Pastes {
	/** The markers, and the selections going, drawn where they are now. */
	set: DecorationSet;
	pending: PendingPaste[];
}

const pasteKey = new PluginKey<Pastes>('markdownPaste');
let nextPaste = 0;

function marker(p: PendingPaste): HTMLElement {
	const el = document.createElement('span');
	el.className = 'doc-paste-pending';
	el.dataset.pasteId = p.id;
	return el;
}

function decorations(doc: PmNode, p: PendingPaste): Decoration[] {
	const out: Decoration[] = [];
	if (p.at.pos !== null) {
		out.push(Decoration.widget(Math.min(p.at.pos, doc.content.size), () => marker(p), { pasteId: p.id, side: -1 }));
	}
	out.push(...goingDecorations(doc, p.replaces));
	return out;
}

function pendingPastes(view: EditorView): PendingPaste[] {
	return pasteKey.getState(view.state)?.pending ?? [];
}

/** The pastes waiting on the server, as Undo weighs them against uploads. */
export function waitingPastes(state: EditorState): { id: string; order: number }[] {
	return (pasteKey.getState(state)?.pending ?? []).map(({ id, order }) => ({ id, order }));
}

/**
 * Call off a waiting paste, as Undo does while it waits: its marker goes,
 * the selection it would have replaced stays, and when the server answers
 * nothing lands. A paste waiting is no edit yet, so y-undo would pass over
 * it to the person's own writing, and its landing, a new edit, would then
 * clear the redo that could bring that writing back.
 */
export function callOffPaste(view: EditorView, id: string): void {
	view.dispatch(view.state.tr.setMeta(pasteKey, { remove: id }));
	toast('Undo stopped the paste');
}

/**
 * Convert `text` and put it at `pos`, wherever that has moved by the time
 * the server answers. With `replacing`, that selection goes in the same
 * edit, wherever it has moved. Pastes waiting at one place land there in
 * the order they were made, whichever the server answers first. The
 * landing is an undo step of its own.
 */
export async function pasteMarkdown(
	view: EditorView,
	text: string,
	pos: number,
	convert: Convert,
	replacing?: Selection,
): Promise<void> {
	const id = `paste-${++nextPaste}`;
	const replaces = replacing && holdSelection(view.state, replacing);
	view.dispatch(view.state.tr.setMeta(pasteKey, { add: { id, order: nextWait(), at: hold(view.state, pos), replaces } }));
	let content: Fragment | null = null;
	try {
		const { html } = await convert(text);
		if (view.isDestroyed) return;
		content = htmlSlice(view.state.schema, html).content;
	} catch {
		if (view.isDestroyed) return;
	}
	const pending = pendingPastes(view);
	const index = pending.findIndex((p) => p.id === id);
	// Undo called it off while it waited.
	if (index < 0) return;
	const self = pending[index];
	const origin = heldPos(view.state, self.at);
	let at = origin;
	const replaced = self.replaces ? heldSelection(view.state, self.replaces) : null;
	const tr = view.state.tr;
	if (self.replaces && replaced && holdsAsTaken(self.replaces, replaced)) {
		const first = Math.min(...replaced.ranges.map((r) => r.$from.pos));
		replaced.replace(tr);
		at = tr.mapping.map(first, -1);
	} else if (replaced) {
		toast('Pasted beside your selection', {
			description: 'Someone changed the words you selected while your server converted the paste, so they stay.',
		});
	}
	if (at === null) {
		at = view.state.selection.from;
		toast('Pasted at the cursor', {
			description: 'Someone deleted the text you pasted into while your server converted the paste.',
		});
	}
	// Nothing converted is the text pasted as it is, not lost.
	let end: number;
	if (content?.size) {
		end = placeConverted(tr, at, content);
	} else {
		const steps = tr.steps.length;
		tr.replaceRange(at, at, plainSlice(view.state.schema, text));
		end = tr.mapping.slice(steps).map(at, 1);
	}
	// A paste made after this one at the same place goes after it.
	const later = pending.slice(index + 1);
	const follow = origin === null ? [] : later.filter((p) => heldPos(view.state, p.at) === origin).map((p) => p.id);
	tr.setMeta(pasteKey, { remove: id, follow: { ids: follow, pos: end } });
	landApart(view, tr);
	for (const other of follow) {
		const now = pendingPastes(view).find((p) => p.id === other)?.at.pos;
		if (now != null) view.dispatch(view.state.tr.setMeta(pasteKey, { anchor: { id: other, at: anchorAt(view.state, now) } }));
	}
}

export function pasteHandling(convert: Convert = convertPastedMarkdown): Extension {
	return Extension.create({
		name: 'pagePaste',
		addProseMirrorPlugins() {
			return [
				new Plugin<Pastes>({
					key: pasteKey,
					state: {
						init: () => ({ set: DecorationSet.empty, pending: [] }),
						apply(tr, prev) {
							const meta = tr.getMeta(pasteKey) as PasteMeta | undefined;
							if (!meta && !prev.pending.length) return prev;
							// Each held through this edit: a marker stays before text typed at it.
							let pending: PendingPaste[] = prev.pending.map((p) => ({
								...p,
								at: holdThrough(tr, p.at, -1),
								replaces: p.replaces && holdRangeThrough(tr, p.replaces),
							}));
							if (meta && 'add' in meta) pending = [...pending, meta.add];
							if (meta && 'remove' in meta) {
								const follow = meta.follow;
								pending = pending
									.filter((p) => p.id !== meta.remove)
									.map((p) => (follow?.ids.includes(p.id) ? { ...p, at: { at: p.at.at, pos: follow.pos } } : p));
							}
							if (meta && 'anchor' in meta) {
								pending = pending.map((p) => (p.id === meta.anchor.id ? { ...p, at: { ...p.at, at: meta.anchor.at } } : p));
							}
							return { set: DecorationSet.create(tr.doc, pending.flatMap((p) => decorations(tr.doc, p))), pending };
						},
					},
					props: {
						decorations: (state) => pasteKey.getState(state)?.set,
						transformPastedHTML: wordLists,
						handlePaste(view, event) {
							const data = event.clipboardData;
							if (!data || data.files?.length) return false;
							const text = data.getData('text/plain');
							const { $from } = view.state.selection;
							if ($from.parent.type.spec.code) {
								if (!text) return false;
								view.dispatch(view.state.tr.insertText(text.replace(/\r\n?/g, '\n')).scrollIntoView());
								return true;
							}
							if (!text) return false;
							// Selected cells take a paste as cells, each the text pasted
							// (prosemirror-tables), never one of them the converted blocks.
							if (view.state.selection instanceof CellSelection) return false;
							const markdown = looksLikeMarkdown(text);
							const html = data.getData('text/html');
							if (html) {
								const mode = vscodeMode(data);
								if (mode !== null) {
									// Tiptap's code block paste takes any other language.
									if (mode !== 'markdown' || !markdown) return false;
								} else if (!isCodeEditorHtml(html)) {
									return false;
								} else if (!markdown) {
									view.dispatch(view.state.tr.replaceSelection(plainSlice(view.state.schema, text)).scrollIntoView());
									return true;
								}
							} else if (!markdown) {
								return false;
							}
							// The selection stays until the paste lands, and goes with it.
							const { selection } = view.state;
							void pasteMarkdown(view, text, selection.from, convert, selection);
							return true;
						},
						handleDrop(view, event, _slice, moved) {
							const data = event.dataTransfer;
							if (moved || !data || data.files?.length || data.getData('text/html')) return false;
							const text = data.getData('text/plain');
							if (!text || !looksLikeMarkdown(text)) return false;
							const at = view.posAtCoords({ left: event.clientX, top: event.clientY })?.pos;
							if (at === undefined || view.state.doc.resolve(at).parent.type.spec.code) return false;
							event.preventDefault();
							void pasteMarkdown(view, text, at, convert);
							return true;
						},
					},
				}),
			];
		},
	});
}
