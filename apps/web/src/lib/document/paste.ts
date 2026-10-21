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
 *   converted, prose with no block's sign included, and code (another
 *   language's, or code copied out of a markdown file's fence) goes to
 *   Tiptap's code block paste.
 * - Plain text that reads as markdown: converted by the contract's own
 *   converter, compiled into the page (`converter.ts`), so a paste reads
 *   markdown exactly as the server's writers do. Only a block's sign counts
 *   as markdown (a heading, a list, a quote, a fence, a table row): a line
 *   of code holds `==`, `**` and `](` often enough, and the converter would
 *   read `<Tag>` in it as a tag. Text with a line that reads as code is
 *   never markdown (`readsAsCode`): a Python or shell comment starts with
 *   `# `, a heading's sign, and the converter would read the code's
 *   `__init__` as bold. The converter keeps each line a line. The words of
 *   a line are formatted by the editor's own paste rules (bold, italic,
 *   code, strike, highlight, links), which read emphasis as CommonMark
 *   does, so `a == b` and `2 * 3 * 4` stay as they are, and run on plain
 *   text only, never in inline code
 *   (`delimited` in schema.ts). The converter reads a paste as text
 *   (`convertNow(…, 'paste')`): a tag the page would lose the words of (an
 *   unknown tag, a comment, `<script>`), or CriticMarkup, stays in it as
 *   written.
 *
 *   The conversion runs inside the paste event, and what it makes goes into
 *   the page in the same edit that takes out the selection it replaces: the
 *   page changes once, when the person pastes, and nothing written after a
 *   wait can race their typing, their undo or another device's edits. That
 *   edit is an undo step of its own (`ownUndoStep`). If the converter cannot
 *   run, refuses the text, or makes nothing of it, the text is pasted as it
 *   is; more than one paste converts is said, and so, once, is a converter
 *   that cannot run. Clipboard HTML that only wraps the text, with no
 *   formatting of its own (`onlyWrapsText`), as Slack and many note apps
 *   write it, does not stop markdown in the text being read as markdown.
 * - Pasted as plain text (Shift with the paste, as Paste and Match Style
 *   is): the text as written, a paragraph a line, never read as markdown
 *   and its words never formatted by the paste rules, so text misread as
 *   markdown can always go in as it is.
 * - A paste or a drop that would take the page past what it holds
 *   (`MAX_NODES`, every text run counted, or `MAX_DEPTH` levels of nesting)
 *   is refused and said, whatever made it: the server would refuse the
 *   edit, and the page would stop saving.
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
import { Plugin, TextSelection, type Selection, type Transaction } from '@tiptap/pm/state';
import { CellSelection } from '@tiptap/pm/tables';
import { canSplit } from '@tiptap/pm/transform';
import type { EditorView } from '@tiptap/pm/view';
import { toast } from 'svelte-sonner';
import { convertNow, type ConvertResult } from './converter';
import { readsAsCode } from './code-text';
import { ownUndoStep } from './history';
import { contract, ITEM_NUMBER, itemNumbers, joinLists } from './schema';

/** Markdown as the blocks a paste makes, now: the page's converter by default. */
export type Convert = (markdown: string) => ConvertResult;

const convertPaste: Convert = (markdown) => convertNow(markdown, 'paste');

/** A block's markdown sign at the start of a line: what makes pasted text markdown. */
const MARKDOWN_SIGNS = [/^#{1,6} /m, /^ {0,3}[-*+] /m, /^ {0,3}\d+\. /m, /^ {0,3}> /m, /^ {0,3}(```|~~~)/m, /^\|.*\|\s*$/m];

/** Signs of markdown inside a line: bold, a link, a highlight. */
const INLINE_SIGNS = [/\*\*/, /\]\(/, /==/];

/** An article's own signs inside a line: a footnote's marker, a veil. */
const ARTICLE_SIGNS = [/\[\^[A-Za-z0-9]/, /⟦/];

/**
 * Whether text reads as markdown rather than plain words: a line starts
 * with a block's sign, and no line reads as code (`readsAsCode`). With
 * `inline`, for text a model wrote as markdown (the inline writer's
 * output), a sign inside a line counts too, and code in it is the model's
 * to fence; with `article` as well, an article's footnote marker or veil.
 */
export function looksLikeMarkdown(text: string, o: { inline?: boolean; article?: boolean } = {}): boolean {
	if (o.inline) {
		const signs = o.article ? [...INLINE_SIGNS, ...ARTICLE_SIGNS] : INLINE_SIGNS;
		return MARKDOWN_SIGNS.some((re) => re.test(text)) || signs.some((re) => re.test(text));
	}
	return MARKDOWN_SIGNS.some((re) => re.test(text)) && !readsAsCode(text);
}

/**
 * The most nodes a page holds, every text run counted: the crate's
 * `MAX_NODES` (`model.rs`). The server refuses an edit that takes a page
 * past it, and the page stops saving.
 */
export const MAX_NODES = 100_000;

/** How many nodes `doc` holds as the server counts them: every node in the page, each text run one. */
export function nodeCount(doc: PmNode): number {
	let n = 0;
	doc.descendants(() => {
		n++;
		return true;
	});
	return n;
}

/**
 * The most levels a page's nodes nest, its own blocks one and text none:
 * the crate's `MAX_DEPTH` (`model.rs`). The server refuses an edit that
 * nests a page deeper, wherever it comes from, and the page stops saving.
 */
export const MAX_DEPTH = 100;

/** How many levels of nodes `doc` nests as the server counts them: a page of paragraphs is 1, one holding a mention 2. */
export function nodeDepth(doc: PmNode): number {
	let deepest = 0;
	const walk = (node: PmNode, level: number): void => {
		node.forEach((child) => {
			if (child.isText) return;
			deepest = Math.max(deepest, level);
			walk(child, level + 1);
		});
	};
	walk(doc, 1);
	return deepest;
}

/** What a page holds that an edit can take it past: its nodes, or how deep they nest. */
export type PageLimit = 'nodes' | 'depth';

/**
 * What `tr` takes the page past, or null when it leaves the page within
 * what it holds. A page already past a limit may stay there, no further.
 */
export function pastPageLimit(before: PmNode, tr: Transaction): PageLimit | null {
	if (!tr.docChanged) return null;
	const nodes = nodeCount(tr.doc);
	if (nodes > MAX_NODES && nodes > nodeCount(before)) return 'nodes';
	const depth = nodeDepth(tr.doc);
	if (depth > MAX_DEPTH && depth > nodeDepth(before)) return 'depth';
	return null;
}

/**
 * What the person is told when what they gave the page would take it past
 * what it holds. Files go in as blocks beside the line they are given at,
 * never deeper, so only their number can.
 */
export function tellPastLimit(limit: PageLimit, what: 'pasted' | 'files' = 'pasted'): void {
	if (what === 'files') {
		toast('Not added', {
			description: 'That would make this page larger than it can hold. Add the files to a new page instead.',
		});
	} else if (limit === 'nodes') {
		toast('Not pasted', {
			description: 'That would make this page larger than it can hold. Paste less at a time, or start a new page for it.',
		});
	} else {
		toast('Not pasted', {
			description: 'That would nest lists, quotes or tables deeper than a page can hold. Hold Shift as you paste to paste it as plain text.',
		});
	}
}

/**
 * Whether the paste under way is one of plain text: Shift held with it, as
 * ProseMirror reads it (`view.input`), but not Shift-Insert, which is an
 * ordinary paste on Windows and Linux.
 */
function pastingPlain(view: EditorView): boolean {
	const input = (view as unknown as { input?: { shiftKey?: boolean; lastKeyCode?: number | null } }).input;
	return !!input?.shiftKey && input.lastKeyCode !== 45;
}

/**
 * Whether clipboard HTML is a code editor's rendering of plain text: one
 * `white-space: pre` block of divs and styled spans, as VS Code and Cursor
 * copy their syntax colours. Read in an inert document: nothing in a
 * paste's HTML loads or runs.
 */
export function isCodeEditorHtml(html: string): boolean {
	const body = inertHtml(html);
	const roots = [...body.children].filter((el) => el.tagName !== 'META' && el.tagName !== 'STYLE');
	if (roots.length !== 1) return false;
	const root = roots[0];
	if (root.tagName !== 'DIV' || !/(^|;)\s*white-space\s*:\s*pre/i.test(root.getAttribute('style') ?? '')) return false;
	return [...root.querySelectorAll('*')].every((el) => el.tagName === 'DIV' || el.tagName === 'SPAN' || el.tagName === 'BR');
}

/** Tags that hold text without formatting it. */
const PLAIN_TAGS = new Set(['DIV', 'P', 'SPAN', 'BR', 'META', 'STYLE']);
/** A style that formats words: bold, italic, underline, strike, a font of its own. */
const FORMATTING_STYLE = /font-weight\s*:\s*(bold|[6-9]00)|font-style\s*:\s*italic|text-decoration[^;]*:\s*[^;]*(underline|line-through)|font-family\s*:[^;]*mono/i;

/**
 * Whether clipboard HTML only wraps `text`: no tag or style that formats
 * it, and the same text. Slack, terminals and many note apps put such a
 * wrapper beside the plain text, and markdown written in them is markdown.
 * HTML with formatting of its own (a heading, a list, a link, bold) is a
 * rich copy, and the page reads that instead. Read in an inert document.
 */
export function onlyWrapsText(html: string, text: string): boolean {
	const body = inertHtml(html);
	for (const el of body.querySelectorAll('*')) {
		if (!PLAIN_TAGS.has(el.tagName)) return false;
		if (el.tagName !== 'STYLE' && FORMATTING_STYLE.test(el.getAttribute('style') ?? '')) return false;
	}
	for (const el of body.querySelectorAll('style')) el.remove();
	// Line breaks and block edges hold no text in `textContent`: compared without any white space.
	const bare = (s: string) => s.replace(/[\s\u00a0]+/g, '');
	return bare(body.textContent ?? '') === bare(text);
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

/**
 * HTML read in a document of its own, which loads nothing: an image the
 * converter's output names is not fetched before it is in the page.
 */
export function inertHtml(html: string): HTMLElement {
	return new DOMParser().parseFromString(html, 'text/html').body;
}

/** HTML as the slice a paste of it would insert. */
export function htmlSlice(schema: Schema, html: string): Slice {
	return PmDOMParser.fromSchema(schema).parseSlice(inertHtml(html));
}

function isList(node: PmNode): boolean {
	return node.type.isInGroup('list');
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
	// so it gets a new id; an item's counts on from the first half, and a
	// numbered list's starts at its first item's number, so every item keeps
	// the one it showed.
	let pos = at;
	let split = false;
	for (let d = $at.depth; d > depth; d--) {
		const $pos = tr.doc.resolve(pos);
		const node = $pos.parent;
		const atStart = node.isTextblock ? $pos.parentOffset === 0 : $pos.index() === 0;
		const atEnd = node.isTextblock ? $pos.parentOffset === node.content.size : $pos.index() === node.childCount;
		const attrs: Record<string, unknown> = { ...node.attrs };
		if (contract.id.attr in attrs) attrs[contract.id.attr] = null;
		if (node.type.name === 'listItem') attrs[ITEM_NUMBER] = null;
		const number = node.type.name === 'orderedList' ? itemNumbers(node)[$pos.index()] : undefined;
		if (number !== undefined) attrs.start = number;
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

/** Said once a session: a converter that cannot run here stays so until the app changes. */
let toldUnavailable = false;

/** What the person is told when their markdown went in as its text. */
function tell(result: ConvertResult): void {
	if (result.kind === 'too-large') {
		toast('Pasted as text', {
			description: "That's more markdown than one paste converts. Paste it in parts to keep its formatting.",
		});
	} else if (result.kind === 'unavailable' && !toldUnavailable) {
		toldUnavailable = true;
		toast('Pasted as text', {
			description: "This app couldn't start its markdown reader, so headings, lists and tables came in as plain text. Try updating the app.",
		});
	}
}

/**
 * Convert `text` and put what it makes at `at`, now, in one edit with taking
 * out `replacing` when there is a selection to replace: a paste's or a
 * drop's whole effect on the page, an undo step of its own. What the
 * converter cannot make blocks of goes in as the text it is.
 */
export function pasteMarkdown(
	view: EditorView,
	text: string,
	at: number,
	o: { replacing?: Selection; convert?: Convert } = {},
): void {
	const { schema } = view.state;
	const tr = view.state.tr;
	if (o.replacing && !o.replacing.empty) {
		const first = Math.min(...o.replacing.ranges.map((r) => r.$from.pos));
		o.replacing.replace(tr);
		at = tr.mapping.map(first, -1);
	}
	const result = (o.convert ?? convertPaste)(text);
	const content = result.kind === 'converted' ? htmlSlice(schema, result.html).content : null;
	const converted = !!content?.size;
	if (content && converted) placeConverted(tr, at, content);
	else tr.replaceRange(at, at, plainSlice(schema, text));
	// As text, a paste the converter found too large is a paragraph a line.
	const past = pastPageLimit(view.state.doc, tr);
	if (past) {
		tellPastLimit(past);
		return;
	}
	if (!converted) tell(result);
	// No `uiEvent`: Tiptap's paste rules run on a transaction marked as a
	// paste or a drop, and would format the converter's text again, or the
	// text pasted as it is.
	ownUndoStep(view, tr.scrollIntoView());
}

/** Put a paste of text as it is into the page, as its own undo step, unless it would take the page past what it holds. */
function place(view: EditorView, tr: Transaction): void {
	const past = pastPageLimit(view.state.doc, tr);
	if (past) tellPastLimit(past);
	else ownUndoStep(view, tr.scrollIntoView());
}

export function pasteHandling(convert: Convert = convertPaste): Extension {
	return Extension.create({
		name: 'pagePaste',
		addProseMirrorPlugins() {
			return [
				new Plugin({
					// ProseMirror's own paste and drop, of HTML or of text.
					filterTransaction(tr, state) {
						const ui = tr.getMeta('uiEvent');
						if (ui !== 'paste' && ui !== 'drop') return true;
						const past = pastPageLimit(state.doc, tr);
						if (!past) return true;
						tellPastLimit(past);
						return false;
					},
					props: {
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
							if (pastingPlain(view)) {
								// No `uiEvent`: the paste rules would format its words.
								place(view, view.state.tr.replaceSelection(plainSlice(view.state.schema, text)));
								return true;
							}
							const markdown = looksLikeMarkdown(text);
							const html = data.getData('text/html');
							if (html) {
								const mode = vscodeMode(data);
								if (mode !== null) {
									// Tiptap's code block paste takes any other language, and
									// code from a markdown file: VS Code names the file's
									// language, not that of the fence the code was copied
									// from. A markdown file's other text is converted, a line
									// of its prose included, which has no block's sign.
									if (mode !== 'markdown' || readsAsCode(text)) return false;
								} else if (!isCodeEditorHtml(html)) {
									if (!markdown || !onlyWrapsText(html, text)) return false;
								} else if (!markdown) {
									place(view, view.state.tr.replaceSelection(plainSlice(view.state.schema, text)));
									return true;
								}
							} else if (!markdown) {
								return false;
							}
							const { selection } = view.state;
							pasteMarkdown(view, text, selection.from, { replacing: selection, convert });
							return true;
						},
						handleDrop(view, event, _slice, moved) {
							const data = event.dataTransfer;
							if (moved || !data || data.files?.length) return false;
							const text = data.getData('text/plain');
							if (!text || !looksLikeMarkdown(text)) return false;
							const html = data.getData('text/html');
							if (html && !onlyWrapsText(html, text)) return false;
							const at = view.posAtCoords({ left: event.clientX, top: event.clientY })?.pos;
							if (at === undefined || view.state.doc.resolve(at).parent.type.spec.code) return false;
							event.preventDefault();
							pasteMarkdown(view, text, at, { convert });
							return true;
						},
					},
				}),
			];
		},
	});
}
