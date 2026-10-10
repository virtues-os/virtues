/**
 * The insert menu (`/`) of a block page; the input rules that turn a quote
 * opened with `[!tip] ` into a callout and a bulleted item opened with
 * `[ ] ` into a to-do; and the keys that act on a whole block: Backspace and
 * Delete at its edge (`BlockEdges`), and moving, copying and deleting the
 * caret's item or block (`BlockMoves`).
 *
 * One flat list in the order a writer reaches for things, as the CodeMirror
 * editor's (`codemirror/extensions/slash-commands.ts`): headings, lists, the
 * occasional block, media, and Ask AI last, since it opens a prompt rather
 * than inserting a block. Heading 1 is first, as there, so `/` and Enter
 * makes one; Text, which makes a heading a paragraph again and which the
 * CodeMirror editor has no need of, comes after the headings. Headings 4 to
 * 6 appear once their name or keyword is typed; `####` makes one too.
 * `/task` makes a to-do list: page checkboxes are page content.
 *
 * A list item nests under the one above it with Tab, which a phone's
 * keyboard does not have: there Indent and Outdent (`/indent`, `/outdent`)
 * do it, and two spaces typed at the start of an item nest it, as spaces
 * before a `-` nest it in the CodeMirror editor.
 */

import { Extension, InputRule, isiOS, isMacOS, type Editor, type Range } from '@tiptap/core';
import type { Node as PmNode, ResolvedPos } from '@tiptap/pm/model';
import { NodeSelection, Selection, TextSelection, type EditorState, type Transaction } from '@tiptap/pm/state';
import type { MenuCommand } from '$lib/components/menuCommand';
import type { Applet } from '$lib/api/client';
import type { Anchor } from './anchor';
import { kindOfMime, mediaNode } from './media';
import { insertBlockAt } from './place';
import { contract, isRoute } from './schema';
import { cellAt, moveRow } from './tables';

export interface TreeCommand extends MenuCommand {
	/** Apply the command where `range` (the trigger and its query) was, removing it. */
	run(editor: Editor, range: Range): void;
}

/**
 * What a command needs from the editor's host: pickers that open over the
 * page. `pos` is where the trigger was, in the page as it is when the
 * command runs; a picker stays open while others edit the page, so the host
 * holds it by an anchor (`anchor.ts`) for what is picked.
 */
export interface TreeCommandHost {
	/** The file picker, for an image or any file; the files go at `pos`. */
	pickFiles(kind: 'image' | 'file', pos: number): void;
	/** The applets to insert, at `pos`. */
	pickApplet(pos: number): void;
	/** The mention picker at `pos`. */
	mention(pos: number): void;
	/** The inline writer's prompt at `pos`. */
	askAi(pos: number): void;
}

export type Tone = 'note' | 'tip' | 'warning';
const TONES: Tone[] = ['note', 'tip', 'warning'];

/** The tone a query or a GFM alert names, the two GFM spells otherwise folded in. */
export function toneOf(word: string): Tone | null {
	const w = word.toLowerCase();
	if ((TONES as string[]).includes(w)) return w as Tone;
	if (w === 'important') return 'note';
	if (w === 'caution') return 'warning';
	return null;
}

const chain = (editor: Editor, range: Range) => editor.chain().focus().deleteRange(range);

/** The query typed after the trigger. */
function queryOf(editor: Editor, range: Range): string {
	return editor.state.doc.textBetween(Math.min(range.from + 1, range.to), range.to, '', '');
}

/** Put a block widget where the trigger was. */
export function insertBlock(editor: Editor, range: Range, node: PmNode): void {
	const tr = editor.state.tr.delete(range.from, range.to);
	insertBlockAt(tr, range.from, node);
	editor.view.dispatch(tr.scrollIntoView());
	editor.commands.focus();
}

/** The list item the caret is in, by its type: a bulleted or numbered list's, or a to-do's. */
function itemAt(editor: Editor): 'listItem' | 'taskItem' | null {
	const { $from } = editor.state.selection;
	for (let d = $from.depth; d > 0; d--) {
		const name = $from.node(d).type.name;
		if (name === 'listItem' || name === 'taskItem') return name;
	}
	return null;
}

/** Nest the caret's list item under the one above it. */
export function indentItem(editor: Editor): boolean {
	const item = itemAt(editor);
	return item ? editor.commands.sinkListItem(item) : false;
}

/** Take the caret's list item out a level. */
export function outdentItem(editor: Editor): boolean {
	const item = itemAt(editor);
	return item ? editor.commands.liftListItem(item) : false;
}

export function treeCommands(host: TreeCommandHost): TreeCommand[] {
	const heading = (level: 1 | 2 | 3 | 4 | 5 | 6, keywords: string[], searchOnly = false): TreeCommand => ({
		label: `Heading ${level}`,
		keywords,
		icon: `ri:h-${level}`,
		searchOnly,
		run: (editor, range) => chain(editor, range).setNode('heading', { level }).run(),
	});
	return [
		heading(1, ['h1', 'title']),
		heading(2, ['h2', 'subtitle']),
		heading(3, ['h3']),
		heading(4, ['h4'], true),
		heading(5, ['h5'], true),
		heading(6, ['h6'], true),
		{
			label: 'Text',
			keywords: ['paragraph', 'plain', 'p'],
			icon: 'ri:text',
			run: (editor, range) => chain(editor, range).setParagraph().run(),
		},
		{
			label: 'Bulleted list',
			keywords: ['ul', 'unordered', 'bullet'],
			icon: 'ri:list-unordered',
			run: (editor, range) => chain(editor, range).toggleBulletList().run(),
		},
		{
			label: 'Numbered list',
			keywords: ['ol', 'ordered', 'number'],
			icon: 'ri:list-ordered',
			run: (editor, range) => chain(editor, range).toggleOrderedList().run(),
		},
		{
			label: 'To-do list',
			keywords: ['todo', 'task', 'checkbox', 'check'],
			icon: 'ri:checkbox-line',
			run: (editor, range) => chain(editor, range).toggleTaskList().run(),
		},
		{
			label: 'Indent',
			keywords: ['nest', 'tab', 'indent'],
			icon: 'ri:indent-increase',
			searchOnly: true,
			run: (editor, range) => {
				chain(editor, range).run();
				indentItem(editor);
			},
		},
		{
			label: 'Outdent',
			keywords: ['unnest', 'dedent', 'outdent'],
			icon: 'ri:indent-decrease',
			searchOnly: true,
			run: (editor, range) => {
				chain(editor, range).run();
				outdentItem(editor);
			},
		},
		{
			label: 'Quote',
			keywords: ['blockquote'],
			icon: 'ri:double-quotes-l',
			run: (editor, range) => chain(editor, range).toggleBlockquote().run(),
		},
		{
			label: 'Callout',
			keywords: ['callout', 'note', 'tip', 'warning'],
			icon: 'ri:lightbulb-line',
			run: (editor, range) => {
				const tone = toneOf(queryOf(editor, range)) ?? 'note';
				chain(editor, range).wrapIn('callout', { tone }).run();
			},
		},
		{
			label: 'Code block',
			keywords: ['fence', 'pre', 'code'],
			icon: 'ri:code-box-line',
			run: (editor, range) => chain(editor, range).setCodeBlock().run(),
		},
		{
			label: 'Divider',
			keywords: ['hr', 'rule', 'separator', 'horizontal'],
			icon: 'ri:separator',
			run: (editor, range) => chain(editor, range).setHorizontalRule().run(),
		},
		{
			label: 'Table',
			keywords: ['grid'],
			icon: 'ri:table-line',
			run: (editor, range) => chain(editor, range).insertTable({ rows: 3, cols: 3, withHeaderRow: true }).run(),
		},
		{
			label: 'Image',
			keywords: ['photo', 'picture'],
			icon: 'ri:image-line',
			run: (editor, range) => {
				chain(editor, range).run();
				host.pickFiles('image', range.from);
			},
		},
		{
			label: 'File',
			keywords: ['upload', 'attachment', 'pdf', 'document', 'audio', 'video'],
			icon: 'ri:attachment-line',
			run: (editor, range) => {
				chain(editor, range).run();
				host.pickFiles('file', range.from);
			},
		},
		{
			label: 'Applet',
			keywords: ['widget', 'chart', 'embed'],
			icon: 'ri:apps-2-line',
			run: (editor, range) => {
				chain(editor, range).run();
				host.pickApplet(range.from);
			},
		},
		{
			label: 'Mention',
			keywords: ['@', 'person', 'place', 'page', 'link', 'ref'],
			icon: 'ri:at-line',
			run: (editor, range) => {
				chain(editor, range).run();
				host.mention(range.from);
			},
		},
		{
			label: 'Ask AI',
			keywords: ['ai', 'write', 'continue', 'generate', 'virtues'],
			icon: 'ri:sparkling-2-line',
			run: (editor, range) => {
				chain(editor, range).run();
				host.askAi(range.from);
			},
		},
	];
}

/** What the mention picker (`RefPicker`) hands back. */
export interface PickedEntity {
	name: string;
	url: string;
	entity_type: string;
	mime_type?: string;
}

/**
 * Put what the mention picker chose where `range` (the typed `@`, or nothing)
 * was: a Drive file as its media block, chosen by its type; a record as a
 * mention, then a space; an address elsewhere as linked text.
 */
export function insertEntity(editor: Editor, range: Range, entity: PickedEntity): void {
	const { schema } = editor.state;
	if (entity.entity_type === 'file') {
		const node = mediaNode(schema, kindOfMime(entity.mime_type), entity.url, entity.name);
		if (node) {
			insertBlock(editor, range, node);
			return;
		}
	}
	const label =
		isRoute(entity.url) && schema.nodes.mention
			? schema.nodes.mention.create({ to: entity.url, label: entity.name })
			: schema.text(entity.name, [schema.marks.link.create({ href: entity.url })]);
	const tr = editor.state.tr.replaceWith(range.from, range.to, [label, schema.text(' ')]);
	editor.view.dispatch(tr.scrollIntoView());
	editor.commands.focus();
}

/**
 * The applets with a face, as commands that insert one where `at` is when
 * one is chosen: where the Applet command ran, wherever edits made while
 * the list was open have moved it. If the text it was in is gone, at the
 * caret.
 */
export function appletCommands(applets: Applet[], at: Anchor): TreeCommand[] {
	return applets
		.filter((a) => a.has_face)
		.map((a) => ({
			label: a.name,
			keywords: [a.id, ...(a.description ? [a.description.toLowerCase()] : [])],
			icon: 'ri:apps-2-line',
			run: (editor: Editor) => {
				const type = editor.schema.nodes.applet;
				if (!type) return;
				const pos = at.resolve() ?? editor.state.selection.from;
				insertBlock(editor, { from: pos, to: pos }, type.create({ ref: a.id }));
			},
		}));
}

/** `[!tip] ` typed at the start of a quote makes the quote a callout of that tone. */
const ALERT = /^\[!(note|tip|warning|important|caution)\]\s$/i;

/** Two spaces typed at the start of a list item's text. */
const NEST = /^ {2}$/;

/** `[ ] ` or `[x] ` typed at the start of a bulleted item's text. */
const TASK = /^\[([ xX]?)\]\s$/;

/** A GFM table's delimiter row, closed with its `|`: `| --- | :-: | ---: |`. */
const TABLE_RULE = /^ {0,3}\|(?:\s*:?-+:?\s*\|)+$/;

/**
 * Where each cell of a GFM table row is in `row`, a paragraph's text with
 * one character per inline widget: `[from, to)` of its words, the pipes
 * around the row and the spaces around each cell left out. A pipe after a
 * backslash is the cell's own. None for a line with no pipe.
 */
function rowCells(row: string): [number, number][] | null {
	if (!row.includes('|')) return null;
	const bars = [...row.matchAll(/(?<!\\)\|/g)].map((m) => m.index);
	const edges = [-1, ...bars, row.length];
	const cells: [number, number][] = [];
	for (let i = 0; i + 1 < edges.length; i++) {
		let from = edges[i] + 1;
		let to = edges[i + 1];
		while (from < to && /\s/.test(row[from])) from++;
		while (to > from && /\s/.test(row[to - 1])) to--;
		cells.push([from, to]);
	}
	// The pipes that open and close the row bound no cell.
	if (/^\s*\|/.test(row)) cells.shift();
	if (/\|\s*$/.test(row)) cells.pop();
	return cells;
}

/** The alignment a delimiter cell's colons give: `:--` left, `--:` right, `:-:` center. */
function alignOf(cell: string): 'left' | 'center' | 'right' | null {
	const left = cell.startsWith(':');
	const right = cell.endsWith(':');
	return left && right ? 'center' : right ? 'right' : left ? 'left' : null;
}

/**
 * A row of cells typed, then the delimiter row under it: the two lines made
 * a table, as GFM reads them and as a markdown page draws them. The first
 * line is the header, its cells keeping their words and marks, each column
 * aligned as its delimiter's colons say; the caret goes into the first cell
 * of a new row under it. Not inside a table, which markdown cannot nest.
 */
function makeTable(tr: Transaction, $from: ResolvedPos, delimiter: string): boolean {
	const { schema } = tr.doc.type;
	const { table, tableRow, tableHeader, tableCell, paragraph } = schema.nodes;
	if (!table || !tableRow || !tableHeader || !tableCell || $from.parent.type !== paragraph || $from.depth < 1) return false;
	for (let d = $from.depth; d > 0; d--) if ($from.node(d).type.spec.tableRole) return false;
	const holder = $from.node(-1);
	const index = $from.index(-1);
	const header = index > 0 ? holder.child(index - 1) : null;
	if (!header || header.type !== paragraph) return false;
	const text = header.textBetween(0, header.content.size, undefined, '\ufffc');
	const cells = rowCells(text);
	const rules = rowCells(delimiter)?.map(([a, b]) => delimiter.slice(a, b));
	if (!cells?.length || !rules || rules.length !== cells.length) return false;
	if (!holder.canReplaceWith(index - 1, index + 1, table)) return false;
	const aligns = rules.map(alignOf);
	const head = tableRow.create(
		null,
		cells.map(([a, b], i) => tableHeader.create({ align: aligns[i] }, paragraph.create(null, header.content.cut(a, b)))),
	);
	const body = tableRow.create(
		null,
		aligns.map((align) => tableCell.create({ align }, paragraph.create())),
	);
	const start = $from.before() - header.nodeSize;
	tr.replaceWith(start, $from.after(), table.create(null, [head, body]));
	// Into the table, past the header row, into the first cell's paragraph.
	tr.setSelection(TextSelection.near(tr.doc.resolve(start + 1 + head.nodeSize + 3)));
	return true;
}

/**
 * Ids cleared through `node`: a copy is a new block, and the editor gives
 * it ids of its own.
 */
function withoutIds(node: PmNode): PmNode {
	if (node.isText) return node;
	const attrs = contract.id.attr in node.attrs ? { ...node.attrs, [contract.id.attr]: null } : node.attrs;
	const children: PmNode[] = [];
	node.content.forEach((child) => children.push(withoutIds(child)));
	return node.type.create(attrs, children, node.marks);
}

/**
 * The bulleted item `$from` starts the text of, made a to-do: `- [ ] `
 * typed is a to-do, as the CodeMirror editor draws it. A list holds one
 * kind of item, so the item becomes a to-do list of its own, between the
 * items before and after it. It keeps its id: it is the same item.
 */
function makeTask(tr: Transaction, $from: ResolvedPos, checked: boolean): boolean {
	const { schema } = tr.doc.type;
	const taskList = schema.nodes.taskList;
	const taskItem = schema.nodes.taskItem;
	if (!taskList || !taskItem || $from.depth < 3) return false;
	const item = $from.node(-1);
	const list = $from.node(-2);
	if (item.type.name !== 'listItem' || list.type.name !== 'bulletList' || $from.index(-1) !== 0) return false;
	const index = $from.index(-2);
	const idAttr = contract.id.attr;
	const items = (from: number, to: number) => {
		const out: PmNode[] = [];
		for (let i = from; i < to; i++) out.push(list.child(i));
		return out;
	};
	const before = index > 0 ? list.type.create(list.attrs, items(0, index)) : null;
	const after =
		index < list.childCount - 1 ? list.type.create({ ...list.attrs, [idAttr]: null }, items(index + 1, list.childCount)) : null;
	const task = taskList.create(
		before ? { [idAttr]: null } : { [idAttr]: list.attrs[idAttr] ?? null },
		taskItem.create({ checked, [idAttr]: item.attrs[idAttr] ?? null }, item.content),
	);
	const start = $from.before(-2);
	tr.replaceWith(start, $from.after(-2), [before, task, after].filter((n): n is PmNode => n !== null));
	tr.setSelection(TextSelection.create(tr.doc, start + (before?.nodeSize ?? 0) + 3));
	return true;
}

/** The page's own input rules: nesting a list item, a bulleted item made a to-do, a quote made a callout, and two typed lines made a table. */
export const pageInputRules = Extension.create({
	name: 'pageInputRules',
	addInputRules() {
		return [
			// `| --- | --- |` typed under `| a | b |` is a table.
			new InputRule({
				find: TABLE_RULE,
				handler: ({ state, range, match }) => {
					if (!makeTable(state.tr, state.doc.resolve(range.from), `${match[0]}`)) return null;
				},
			}),
			// Two spaces at the start of a list item nest it under the one
			// above, where there is one; otherwise they stay as typed.
			new InputRule({
				find: NEST,
				handler: ({ state, range, chain }) => {
					const $from = state.doc.resolve(range.from);
					const item = $from.depth >= 2 ? $from.node(-1) : null;
					if (!item || !['listItem', 'taskItem'].includes(item.type.name) || $from.index(-1) !== 0) return null;
					if ($from.index(-2) === 0) return null;
					chain().deleteRange(range).sinkListItem(item.type.name).run();
				},
			}),
			// `[ ] ` after `- ` is a to-do. Tiptap's own to-do rule wraps a
			// paragraph in a to-do list, which an item's first paragraph
			// cannot be: it would leave the brackets as text.
			new InputRule({
				find: TASK,
				handler: ({ state, range, match }) => {
					const tr = state.tr.delete(range.from, range.to);
					if (!makeTask(tr, tr.doc.resolve(range.from), match[1].toLowerCase() === 'x')) return null;
				},
			}),
			new InputRule({
				find: ALERT,
				handler: ({ state, range, match }) => {
					const callout = state.schema.nodes.callout;
					const tone = toneOf(match[1]);
					const $from = state.doc.resolve(range.from);
					if (!callout || !tone || $from.depth < 2) return null;
					const quote = $from.node(-1);
					if (quote.type.name !== 'blockquote' || $from.index(-1) !== 0) return null;
					const quoteStart = $from.before(-1);
					const tr = state.tr.delete(range.from, range.to);
					// The block keeps its id: it is the same block, now a callout.
					tr.setNodeMarkup(tr.mapping.map(quoteStart), callout, { ...quote.attrs, tone });
				},
			}),
		];
	},
});

/**
 * Backspace at the start of a heading makes it a paragraph, as removing
 * its `#` does in the CodeMirror editor; one more Backspace joins it to the
 * block above. The block keeps its id: it is the same block, now text. An
 * input rule that just made the heading is undone first, giving back what
 * was typed.
 */
export const HeadingBackspace = Extension.create({
	name: 'headingBackspace',
	// Ahead of the editor's own Backspace, which would join the heading to
	// the block above.
	priority: 1000,
	addKeyboardShortcuts() {
		return {
			Backspace: ({ editor }) => {
				const { state } = editor;
				const { $from, empty } = state.selection;
				if (!empty || $from.parentOffset !== 0 || $from.parent.type.name !== 'heading') return false;
				if (editor.commands.undoInputRule()) return true;
				const paragraph = state.schema.nodes.paragraph;
				if (!$from.node(-1).canReplaceWith($from.index(-1), $from.index(-1) + 1, paragraph)) return false;
				const heading = $from.parent;
				const attrs = Object.fromEntries(
					Object.keys(paragraph.spec.attrs ?? {})
						.filter((k) => k in heading.attrs)
						.map((k) => [k, heading.attrs[k]]),
				);
				editor.view.dispatch(state.tr.setNodeMarkup($from.before(), paragraph, attrs));
				return true;
			},
		};
	},
});

/**
 * Backspace and Delete at a block's edge never take a widget or a code
 * block with them in one key. Beside a widget (an image, an applet, a file,
 * a rule) they select it, drawn as selected, and the next key deletes it,
 * as the CodeMirror editor's table is selected before it goes. Beside a
 * code block they move the caret into it, and from a code block's edge
 * into the block beside it: joining would flatten the code into a line of
 * text and lose its language. An empty line beside either goes, as the key
 * takes an empty line anywhere, and the widget is then selected or the
 * caret is in the code: an empty line between two of them is no line the
 * keys cannot remove. An empty code block is Tiptap's to clear.
 */
export const BlockEdges = Extension.create({
	name: 'blockEdges',
	// Ahead of the editor's own Backspace and Delete, which join.
	priority: 1000,
	addKeyboardShortcuts() {
		const edge = (editor: Editor, dir: -1 | 1): boolean => {
			const { state } = editor;
			const { $from, empty } = state.selection;
			if (!empty || !$from.parent.isTextblock || $from.depth < 1) return false;
			const atEdge = dir < 0 ? $from.parentOffset === 0 : $from.parentOffset === $from.parent.content.size;
			if (!atEdge) return false;
			const code = !!$from.parent.type.spec.code;
			if (code && $from.parent.content.size === 0) return false;
			const container = $from.node(-1);
			const index = $from.index(-1) + dir;
			if (index < 0 || index >= container.childCount) return false;
			const beside = container.child(index);
			const widget = beside.isAtom && beside.isBlock;
			if (!widget && !beside.type.spec.code && !code) return false;
			const tr = state.tr;
			let at = dir < 0 ? $from.before() - beside.nodeSize : $from.after();
			let edge = dir < 0 ? $from.before() : $from.after();
			const line = $from.index(-1);
			if (!code && $from.parent.content.size === 0 && container.canReplace(line, line + 1)) {
				tr.delete($from.before(), $from.after());
				edge = $from.before();
				if (dir > 0) at = edge;
			}
			const selection: Selection = widget ? NodeSelection.create(tr.doc, at) : Selection.near(tr.doc.resolve(edge), dir);
			editor.view.dispatch(tr.setSelection(selection).scrollIntoView());
			return true;
		};
		const back = ({ editor }: { editor: Editor }) => edge(editor, -1);
		const forward = ({ editor }: { editor: Editor }) => edge(editor, 1);
		// Every key Tiptap's own keymap joins blocks with, on the platforms it
		// binds them on: deleting a word back to a line's start (⌥⌫, Ctrl-
		// Backspace) reaches the edge as Backspace does.
		return {
			...Object.fromEntries(backwardKeys().map((key) => [key, back])),
			...Object.fromEntries(forwardKeys().map((key) => [key, forward])),
		};
	},
});

/** The keys Tiptap's keymap sends to its Backspace handler, which joins a line to the block before it. */
function backwardKeys(): string[] {
	return ['Backspace', 'Mod-Backspace', 'Shift-Backspace', ...(isMacOS() || isiOS() ? ['Alt-Backspace', 'Ctrl-h'] : [])];
}

/** The keys Tiptap's keymap sends to its Delete handler, which joins the block after a line to it. */
function forwardKeys(): string[] {
	return ['Delete', 'Mod-Delete', ...(isMacOS() || isiOS() ? ['Ctrl-d', 'Ctrl-Alt-Backspace', 'Alt-Delete', 'Alt-d'] : [])];
}

/** Tick the to-do the caret is in, or untick it; false outside one, and in code, where ⌘Enter leaves the code. */
export function tickAtCaret(editor: Editor): boolean {
	const { $from } = editor.state.selection;
	if ($from.parent.type.spec.code || !editor.isEditable) return false;
	for (let d = $from.depth; d > 0; d--) {
		const node = $from.node(d);
		if (node.type.name !== 'taskItem') continue;
		editor.view.dispatch(editor.state.tr.setNodeMarkup($from.before(d), undefined, { ...node.attrs, checked: !node.attrs.checked }));
		return true;
	}
	return false;
}

/**
 * ⌘Enter ticks the to-do the caret is in, as the box does, so the keyboard
 * ticks it from its text. Ahead of the hard break Tiptap binds the key to.
 */
export const TodoKeys = Extension.create({
	name: 'todoKeys',
	priority: 1000,
	addKeyboardShortcuts() {
		return { 'Mod-Enter': ({ editor }) => tickAtCaret(editor) };
	},
});

/** The items a key that moves a line moves inside a list, as it moves a line. */
const ITEMS = ['listItem', 'taskItem'];

/**
 * What a key that moves a line moves: every list item the selection
 * touches, else every block at the top of the page it touches, or the
 * block selected; siblings, from `start` to `end`. A selection reaching
 * from one list item into a block outside the list moves the blocks around
 * both. A selection that ends at the very start of a block leaves that
 * block out, as the CodeMirror editor leaves out a line the selection only
 * reaches the start of. Null where there is nothing to move.
 */
function unitAt(state: EditorState): { start: number; end: number; nodes: PmNode[]; parent: PmNode; index: number } | null {
	const { selection } = state;
	if (selection instanceof NodeSelection && selection.node.isBlock) {
		const { $from } = selection;
		return { start: $from.pos, end: selection.to, nodes: [selection.node], parent: $from.parent, index: $from.index() };
	}
	const { $from, $to } = selection;
	// The depths a unit can be at, innermost first: each item around the
	// selection's start, then the block at the top of the page.
	const depths: number[] = [];
	for (let d = $from.depth; d > 1; d--) if (ITEMS.includes($from.node(d).type.name)) depths.push(d);
	if ($from.depth >= 1) depths.push(1);
	for (const depth of depths) {
		// The selection's end is in a sibling of the start's unit.
		if ($to.depth < depth || $to.start(depth - 1) !== $from.start(depth - 1)) continue;
		const parent = $from.node(depth - 1);
		const index = $from.index(depth - 1);
		let last = $to.index(depth - 1);
		if (last > index && !selection.empty && Selection.findFrom(state.doc.resolve($to.start(depth)), 1, true)?.from === $to.pos) last--;
		const nodes: PmNode[] = [];
		for (let i = index; i <= last; i++) nodes.push(parent.child(i));
		const start = $from.before(depth);
		return { start, end: start + nodes.reduce((size, n) => size + n.nodeSize, 0), nodes, parent, index };
	}
	return null;
}

/**
 * The selection moved by `shift` into `doc`, a node selection staying one.
 * An end at the start of a block left out of what moved stays inside `doc`.
 */
function shifted(state: EditorState, doc: PmNode, shift: number): Selection {
	const { anchor, head } = state.selection;
	if (state.selection instanceof NodeSelection) return NodeSelection.create(doc, anchor + shift);
	const at = (pos: number) => doc.resolve(Math.max(0, Math.min(pos + shift, doc.content.size)));
	return TextSelection.between(at(anchor), at(head));
}

/**
 * Move the caret's list item or block, or every one the selection touches,
 * past the one above (`-1`) or below (`1`), as ⌥↑ and ⌥↓ move the lines a
 * selection touches in the CodeMirror editor. In a table it is the row that
 * moves, a table row being that editor's line. Each keeps its id: it is the
 * same block, elsewhere.
 *
 * They are taken out and put back in two edits, one undo step between them.
 * The binding writes the shared document from the page as it stands after
 * each edit, and given the blocks swapped in one it rewrites each one's
 * element with the other's content, which leaves an edit another device
 * made meanwhile in the block it was not made in. Taken out and put back,
 * only the moved blocks' elements are new: the neighbour's is untouched, and
 * an edit made to a moved block meanwhile is lost rather than misplaced.
 */
export function moveBlock(editor: Editor, by: -1 | 1): boolean {
	const { state } = editor;
	if (cellAt(state)) return moveRow(editor, by);
	const unit = unitAt(state);
	if (!unit) return false;
	const { start, end, nodes, parent, index } = unit;
	const other = by < 0 ? index - 1 : index + nodes.length;
	if (other < 0 || other >= parent.childCount) return false;
	const neighbour = parent.child(other);
	editor.view.dispatch(state.tr.delete(start, end));
	const tr = editor.state.tr.insert(by < 0 ? start - neighbour.nodeSize : start + neighbour.nodeSize, nodes);
	tr.setSelection(shifted(state, tr.doc, by < 0 ? -neighbour.nodeSize : neighbour.nodeSize));
	editor.view.dispatch(tr.scrollIntoView());
	return true;
}

/**
 * Copy the caret's list item or block, or every one the selection touches,
 * above (`-1`) or below (`1`) them, the selection going into the copy, as
 * ⇧⌥↑ and ⇧⌥↓ copy lines in the CodeMirror editor. The copies are new
 * blocks, with ids of their own.
 */
export function copyBlock(editor: Editor, by: -1 | 1): boolean {
	const { state } = editor;
	const unit = unitAt(state);
	if (!unit) return false;
	const tr = state.tr.insert(by < 0 ? unit.start : unit.end, unit.nodes.map(withoutIds));
	if (by > 0) tr.setSelection(shifted(state, tr.doc, unit.end - unit.start));
	editor.view.dispatch(tr.scrollIntoView());
	return true;
}

/**
 * Delete the caret's list item or block, or every one the selection
 * touches, as ⌘⇧K deletes lines in the CodeMirror editor. Every item of a
 * list takes the list with it; the whole page leaves an empty line.
 */
export function deleteBlock(editor: Editor): boolean {
	const { state } = editor;
	const unit = unitAt(state);
	if (!unit) return false;
	let { start, end } = unit;
	let $start = state.doc.resolve(start);
	while ($start.depth > 0 && $start.parent.type.isInGroup('list') && start === $start.start() && end === $start.end()) {
		start = $start.before();
		end = $start.after();
		$start = state.doc.resolve(start);
	}
	const tr = state.tr;
	if ($start.depth === 0 && start === 0 && end === state.doc.content.size) {
		tr.replaceWith(0, state.doc.content.size, state.schema.nodes.paragraph.create());
	} else {
		tr.delete(start, end);
	}
	tr.setSelection(Selection.near(tr.doc.resolve(Math.min(start, tr.doc.content.size)), 1));
	editor.view.dispatch(tr.scrollIntoView());
	return true;
}

/** The two spaces a code line is indented by, as the CodeMirror editor's indent unit. */
const CODE_INDENT = '  ';

/**
 * Indent (`1`) or outdent (`-1`) every line of a code block the selection
 * touches, as ⌘] and ⌘[ do in the CodeMirror editor: two spaces added at
 * each line's start, or up to two spaces, or a tab, taken away. A line the
 * selection only reaches the start of is left, as that editor leaves it.
 * One edit. False outside a code block, or across its edge.
 */
export function indentCode(editor: Editor, by: -1 | 1): boolean {
	const { state } = editor;
	const { $from, $to, empty } = state.selection;
	if (!editor.isEditable || !$from.parent.type.spec.code || !$from.sameParent($to)) return false;
	const text = $from.parent.textContent;
	const base = $from.start();
	const starts: number[] = [];
	for (let at = text.lastIndexOf('\n', $from.parentOffset - 1) + 1; ; ) {
		starts.push(at);
		const next = text.indexOf('\n', at);
		if (next < 0 || next + 1 > $to.parentOffset || (next + 1 === $to.parentOffset && !empty)) break;
		at = next + 1;
	}
	const tr = state.tr;
	for (const at of starts.reverse()) {
		if (by > 0) {
			tr.insertText(CODE_INDENT, base + at);
			continue;
		}
		const lead = /^(\t| {1,2})/.exec(text.slice(at))?.[0].length ?? 0;
		if (lead) tr.delete(base + at, base + at + lead);
	}
	if (tr.docChanged) editor.view.dispatch(tr.scrollIntoView());
	return true;
}

/**
 * ⌥↑ ⌥↓ move, ⇧⌥↑ ⇧⌥↓ copy and ⌘⇧K delete the caret's list item or block,
 * or every one a selection touches. ⌘] and ⌘[ indent and outdent the lines
 * of a code block, and elsewhere nest a list item and take it out a level,
 * as they indent and outdent a line in the CodeMirror editor. Those two are
 * taken on an editable page where there is nothing to move too, as that
 * editor takes them on any line: ⌘[ is the browser's Back otherwise.
 */
export const BlockMoves = Extension.create({
	name: 'blockMoves',
	addKeyboardShortcuts() {
		return {
			'Alt-ArrowUp': ({ editor }) => moveBlock(editor, -1),
			'Alt-ArrowDown': ({ editor }) => moveBlock(editor, 1),
			'Shift-Alt-ArrowUp': ({ editor }) => copyBlock(editor, -1),
			'Shift-Alt-ArrowDown': ({ editor }) => copyBlock(editor, 1),
			'Mod-Shift-k': ({ editor }) => deleteBlock(editor),
			'Mod-]': ({ editor }) => indentCode(editor, 1) || indentItem(editor) || editor.isEditable,
			'Mod-[': ({ editor }) => indentCode(editor, -1) || outdentItem(editor) || editor.isEditable,
		};
	},
});
