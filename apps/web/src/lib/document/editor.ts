/**
 * The block editor for a page: Tiptap on the page's Yjs tree, its schema from
 * the contract (`schema.ts`), its widgets as Svelte node views (`views.ts`).
 *
 * Opening a page writes nothing to it. Every extension here either draws
 * (decorations, carets, node views) or acts on the person's input; none
 * rewrites what it is given. That is why `TrailingNode` is absent: its
 * `appendTransaction` adds a paragraph after a page ending in a table, an
 * image or an applet, once per client that opens it, a write nobody made. A
 * click below the last block adds that paragraph as the person's own edit,
 * and the gap cursor lets typing land after a final table.
 */

import { Editor, Extension, type AnyExtension } from '@tiptap/core';
import Collaboration, { isChangeOrigin } from '@tiptap/extension-collaboration';
import CollaborationCaret from '@tiptap/extension-collaboration-caret';
import UniqueID from '@tiptap/extension-unique-id';
import { ListKeymap } from '@tiptap/extension-list';
import { Dropcursor, Gapcursor, Placeholder } from '@tiptap/extensions';
import type { Node as PmNode, Slice } from '@tiptap/pm/model';
import { Plugin, TextSelection, type EditorState, type NodeSelection } from '@tiptap/pm/state';
import type { EditorView } from '@tiptap/pm/view';
import type * as Y from 'yjs';
import type { Awareness } from 'y-protocols/awareness';
import type { AiIntent } from '$lib/ai/inlineComplete';
import { contract, contractExtensions, idTypes, newId, type Extras } from './schema';
import { BlockEdges, BlockMoves, HeadingBackspace, TodoKeys, pageInputRules } from './commands';
import { TableKeys } from './tables';
import { localUser, renderCaret, renderSelection } from './presence';
import { pageNodeViews } from './views';
import { clipboardText } from './markdown';
import { ProposalsPasted, ProposalsRead, TypingOutsideDeletions } from './suggestions';
import { callOffPaste, waitingPastes } from './paste';
import { callOffUpload, waitingUploads } from './media';
import { KeptThroughOthersEdits } from './anchor';

/** What the editor binds to: a `TreeDocument`, or anything with its document and caret channel. */
export interface EditorBinding {
	ydoc: Y.Doc;
	/** The awareness channel other devices' carets come through; null for no carets. */
	provider: { awareness: Awareness } | null;
}

export interface PageEditorOptions {
	element: HTMLElement;
	doc: EditorBinding;
	editable: boolean;
	/** What assistive technology calls the editor: the page's title. */
	label?: string;
	/** How this device appears in others' carets; `localUser` by default. */
	user?: { name: string; color?: string; tint?: string };
	placeholder: string;
	/** Further extensions: the host's menus, find, uploads, … */
	plugins?: AnyExtension[];
	/** The node views; the page's own by default. */
	extras?: Extras;
	spellcheck?: boolean;
}

/**
 * The inline writer, as the editor drives it: ⌘J, `/ai` and Ask AI start it,
 * Escape and typing stop it. The page that mounts the editor supplies it;
 * without one, the editor offers no Ask AI.
 */
export interface TreeAiDriver {
	start(o: { editor: Editor; intent: AiIntent; instruction: string; pageTitle?: string }): void;
	abortIn(editor: Editor): void;
	isActive(): boolean;
}

/**
 * Where a click below the last block puts the caret: at the end of the
 * page's last text, followed down through the containers that end the page
 * (a list, a quote, a callout) to the deepest last block. When that is a
 * widget (an image, an applet, a rule) or a table, its text is no end to
 * type at, and a new paragraph goes after it, in the container it ends, or
 * at the top level if that container holds none there. Never a selection of
 * the widget, which the next key would replace.
 */
export function endOfPage(doc: PmNode): { text: number } | { paragraphAt: number } {
	const top = doc.content.size;
	let node = doc;
	let start = 0;
	for (;;) {
		const last = node.lastChild;
		if (!last) return { paragraphAt: top };
		const at = start + node.content.size - last.nodeSize;
		if (last.isTextblock) return { text: at + 1 + last.content.size };
		if (last.isAtom || last.isLeaf || last.type.name === 'table') {
			const paragraph = doc.type.schema.nodes.paragraph;
			const fits = node.canReplaceWith(node.childCount, node.childCount, paragraph);
			return { paragraphAt: fits ? at + last.nodeSize : top };
		}
		node = last;
		start = at + 1;
	}
}

/**
 * A click below the last block puts the caret at the end of the page
 * (`endOfPage`): in its last text, else in a new paragraph after the widget
 * or table that ends it. The paragraph is the person's own edit, so it is
 * theirs to undo.
 */
const ClickBelowEnd = Extension.create({
	name: 'clickBelowEnd',
	addProseMirrorPlugins() {
		return [
			new Plugin({
				props: {
					handleDOMEvents: {
						mousedown(view, event) {
							if (!view.editable || event.button !== 0) return false;
							const last = view.dom.lastElementChild;
							if (!last || event.clientY <= last.getBoundingClientRect().bottom) return false;
							const end = endOfPage(view.state.doc);
							const tr = view.state.tr;
							if ('text' in end) {
								tr.setSelection(TextSelection.create(tr.doc, end.text));
							} else {
								tr.insert(end.paragraphAt, view.state.schema.nodes.paragraph.create());
								tr.setSelection(TextSelection.create(tr.doc, end.paragraphAt + 1));
							}
							view.dispatch(tr.scrollIntoView());
							view.focus();
							event.preventDefault();
							return true;
						},
					},
				},
			}),
		];
	},
});

/**
 * The editor's own attributes. Tiptap names it a `textbox`; a page is many
 * lines, called by its title, and read only when it cannot be edited, which
 * assistive technology says only when told.
 */
export function editorAttributes(o: { spellcheck: boolean; editable: boolean; label?: string }): Record<string, string> {
	return {
		class: 'doc-prose',
		spellcheck: String(o.spellcheck),
		'aria-multiline': 'true',
		'aria-label': o.label || 'Page',
		...(o.editable ? {} : { 'aria-readonly': 'true' }),
	};
}

/**
 * Make an editor editable or read only and its spelling checked or not, and
 * name it, its attributes following.
 */
export function setPageEditorState(editor: Editor, o: { editable: boolean; spellcheck: boolean; label?: string }): void {
	editor.setOptions({ editorProps: { ...editor.options.editorProps, attributes: editorAttributes(o) } });
	if (editor.isEditable !== o.editable) editor.setEditable(o.editable);
}

/**
 * Where a selection is, for the inline writer: in one code block, in text,
 * or across the edge of a code block. A code block holds no marks, so a
 * rewrite there replaces the selection; across the edge it would replace
 * prose with nothing to accept or reject, so the writer is not offered.
 */
export function codeAt(state: EditorState): 'code' | 'text' | 'across' {
	const { $from, $to } = state.selection;
	const from = !!$from.parent.type.spec.code;
	const to = !!$to.parent.type.spec.code;
	if (from && to && $from.sameParent($to)) return 'code';
	return from || to ? 'across' : 'text';
}

/**
 * Undo while a paste or an upload waits on the server calls off the newest
 * of them (`callOffPaste`, `callOffUpload`), and says so; only with nothing
 * waiting does it undo an edit. What waits is no edit yet: y-undo would pass
 * over it to the person's own writing, and its landing, a new edit, would
 * clear the redo that could bring that writing back, while the paste or the
 * file the person meant to undo landed anyway. Returns whether one waited.
 */
export function callOffNewestWaiting(view: EditorView): boolean {
	const waiting = [
		...waitingPastes(view.state).map((p) => ({ order: p.order, callOff: () => callOffPaste(view, p.id) })),
		...waitingUploads(view.state).map((u) => ({ order: u.order, callOff: () => callOffUpload(view, u.id) })),
	];
	if (!waiting.length) return false;
	waiting.reduce((newest, w) => (w.order > newest.order ? w : newest)).callOff();
	return true;
}

/** ⌘Z calls off what waits first (`callOffNewestWaiting`). */
const UndoCallsOffWaiting = Extension.create({
	name: 'undoCallsOffWaiting',
	// Ahead of Collaboration's ⌘Z (priority 1000), which goes to y-undo.
	priority: 1001,
	addKeyboardShortcuts() {
		return { 'Mod-z': ({ editor }) => callOffNewestWaiting(editor.view) };
	},
});

/**
 * The platform's own undo: shake to undo, three fingers, the iPad's undo
 * key, Edit > Undo. They reach the page as `historyUndo` and `historyRedo`
 * input, not as ⌘Z; left alone, the browser would undo the DOM under
 * ProseMirror. They go to the editor's own undo, as the CodeMirror editor's
 * binding sends them to its, and an undo calls off what waits first.
 */
const PlatformUndo = Extension.create({
	name: 'platformUndo',
	addProseMirrorPlugins() {
		const editor = this.editor;
		return [
			new Plugin({
				props: {
					handleDOMEvents: {
						beforeinput(view, event) {
							const type = (event as InputEvent).inputType;
							if (type !== 'historyUndo' && type !== 'historyRedo') return false;
							event.preventDefault();
							if (type === 'historyRedo') editor.commands.redo();
							else if (!callOffNewestWaiting(view)) editor.commands.undo();
							return true;
						},
					},
				},
			}),
		];
	},
});

/** The page editor a drag started in, while it lasts. */
let dragSource: EditorView | null = null;

/**
 * What a drag from `source` carries, as a range of it to take out once it
 * lands elsewhere: the node dragged, or the selection dragged; null when
 * the page has changed so that it no longer holds it there.
 */
function draggedRange(source: EditorView): { from: number; to: number } | null {
	const dragging = source.dragging as { slice: Slice; move: boolean; node?: NodeSelection } | null;
	if (!dragging?.move) return null;
	if (dragging.node) {
		const { from, to, node } = dragging.node;
		return source.state.doc.nodeAt(from)?.eq(node) ? { from, to } : null;
	}
	const sel = source.state.selection;
	return !sel.empty && sel.content().eq(dragging.slice) ? { from: sel.from, to: sel.to } : null;
}

/**
 * A drag from one page's editor dropped on another's (two pages side by
 * side) moves what was dragged: ProseMirror puts it in the page it lands
 * on, and this takes it out of the page it came from, once, and only it,
 * as the person's own edit there. A drag with the copy modifier, or a drop
 * that put nothing in, takes nothing out. Tiptap's own version, which
 * deleted the other page's selection once per paste rule, is patched out
 * (`patches/@tiptap__core@3.31.4.patch`).
 */
const DragBetweenPages = Extension.create({
	name: 'dragBetweenPages',
	addProseMirrorPlugins() {
		return [
			new Plugin({
				props: {
					handleDOMEvents: {
						dragstart(view) {
							dragSource = view;
							return false;
						},
						dragend(view) {
							if (dragSource === view) dragSource = null;
							return false;
						},
						drop(view) {
							const source = dragSource;
							if (!source || source === view || source.isDestroyed || !source.editable) return false;
							const range = draggedRange(source);
							if (!range) return false;
							const before = view.state.doc;
							// After ProseMirror's own drop has put it in this page.
							queueMicrotask(() => {
								if (view.isDestroyed || source.isDestroyed || view.state.doc === before) return;
								const now = draggedRange(source);
								if (!now || now.from !== range.from || now.to !== range.to) return;
								source.dispatch(source.state.tr.delete(range.from, range.to).setMeta('uiEvent', 'drop'));
							});
							return false;
						},
					},
				},
			}),
		];
	},
});

export function createPageEditor(o: PageEditorOptions): Editor {
	const user = o.user ?? localUser(o.doc.ydoc.clientID);
	const extensions: AnyExtension[] = [
		...contractExtensions(o.extras ?? pageNodeViews()),
		UniqueID.configure({
			attributeName: contract.id.attr,
			types: idTypes,
			generateID: () => newId(),
			// Ids arrive with remote changes; assigning them again would fight.
			filterTransaction: (tr) => !isChangeOrigin(tr),
		}),
		// Its y-undo is the undo: one history of the person's own edits.
		Collaboration.configure({ document: o.doc.ydoc, field: contract.fragment }),
		KeptThroughOthersEdits,
		...(o.doc.provider
			? [
					CollaborationCaret.configure({
						provider: o.doc.provider,
						user,
						render: renderCaret,
						selectionRender: renderSelection,
					}),
				]
			: []),
		Dropcursor,
		Gapcursor,
		ListKeymap,
		Placeholder.configure({ placeholder: o.placeholder }),
		pageInputRules,
		HeadingBackspace,
		BlockEdges,
		BlockMoves,
		TodoKeys,
		TableKeys,
		ClickBelowEnd,
		UndoCallsOffWaiting,
		PlatformUndo,
		DragBetweenPages,
		ProposalsPasted,
		ProposalsRead,
		TypingOutsideDeletions,
		...(o.plugins ?? []),
	];
	return new Editor({
		element: o.element,
		editable: o.editable,
		extensions,
		editorProps: {
			attributes: editorAttributes({ spellcheck: o.spellcheck ?? true, editable: o.editable, label: o.label }),
			// Text copied out goes where markdown is read (the chat composer,
			// a markdown page): its links, headings and lists as markdown.
			clipboardTextSerializer: clipboardText,
		},
	});
}
