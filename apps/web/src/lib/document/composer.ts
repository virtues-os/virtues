/**
 * The chat composer: the block editor's kernel (Tiptap on ProseMirror, the
 * page contract's schema) holding one message, with no Yjs. Its history is
 * plain prosemirror-history.
 *
 * What it holds is part of the contract (`MESSAGE` in `message.ts`); what it
 * sends is that document as markdown (`messageText`), and a draft or a
 * prefill is read back through the page's converter (`readMessage`).
 *
 * Keys, as a messaging app's: Enter sends on a desktop and breaks the line
 * on a phone, whose send button sends. Shift+Enter breaks the line
 * everywhere; in a list it starts the next item and on an empty item ends
 * the list, and on an empty line in a quote it ends the quote. Cmd+Enter
 * (Ctrl+Enter) sends from anywhere. In a code block Enter and Shift+Enter
 * are new lines, Tab indents and Shift+Tab takes the indent back; Enter on a
 * line of three backticks opens one. Elsewhere Tab nests a list item and
 * otherwise moves focus, and Shift+Tab cycles the chat's mode (outdenting a
 * nested item first). `@` opens the ref picker, and `/` at the very start
 * of the message opens the skills menu.
 */

import { Editor, Extension } from '@tiptap/core';
import { ListKeymap } from '@tiptap/extension-list';
import { Placeholder, UndoRedo } from '@tiptap/extensions';
import { Fragment, Slice, type Node as PmNode, type Schema } from '@tiptap/pm/model';
import { Plugin, Selection, TextSelection, type EditorState } from '@tiptap/pm/state';
import { findWrapping } from '@tiptap/pm/transform';
import MentionNode from '$lib/components/pages/nodes/MentionNode.svelte';
import { convertNow, loadConverter } from './converter';
import { readsAsCode } from './code-text';
import { messageMarkdown } from './markdown';
import { holdsOnlyMessage, MESSAGE, messageText, readMessage, textBlocks, type Reading } from './message';
import { svelteNodeView } from './nodeviews.svelte';
import { htmlSlice, looksLikeMarkdown, placeConverted } from './paste';
import { contractExtensions, isRoute } from './schema';
import { closeTrigger, pickedRange, triggerState, triggers } from './triggers';
import { drawnCaret } from './caret';
import { dropLine } from './drop-line';

/** Pasted text longer than this becomes an attachment rather than a wall in the composer. */
export const PASTE_AS_FILE_THRESHOLD = 1500;

/** A line that opens a code block when Enter is pressed at its end: three backticks and a language. */
const FENCE_OPENER = /^```([^`\s]*)$/;

/** The indent Tab adds in a code block. */
const INDENT = '  ';

export interface ComposerOptions {
	element: HTMLElement;
	/** The message to open with, as markdown. */
	markdown?: string;
	placeholder?: string;
	/** The field's accessible name. A chat message by default. */
	label?: string;
	disabled?: boolean;
	/**
	 * Read at keystroke time, not creation time: the layout store can flip on
	 * resize. On a phone Return breaks the line and the send button sends.
	 */
	isMobile: () => boolean;
	onSubmit: () => void;
	/** The message's markdown, after every change. */
	onChange: (markdown: string) => void;
	onFocusChange: (focused: boolean) => void;
	/** The content's height in px, whenever it may have changed. */
	onHeight: (px: number) => void;
	/** Files pasted or dropped, and long pasted text. */
	onAttach?: (files: File[]) => void;
	/** Escape with nothing else to close. Return true if handled. */
	onEscape?: () => boolean;
	/** Shift+Tab outside a code block and a nested list item: cycle the chat's mode. */
	onShiftTab?: () => void;
	/** `@` typed at the start of a word: open the ref picker with what follows it. */
	onMentionOpen?: (query: string) => void;
	/** The text typed after the `@` changed. */
	onMentionQuery?: (query: string) => void;
	/** The `@` closed: a space, a move away, Escape, or a ref picked. */
	onMentionClose?: () => void;
	/** `/` typed at the very start of the message: open the skills menu with what follows it. */
	onSlashOpen?: (query: string) => void;
	/** The text typed after the `/` changed. */
	onSlashQuery?: (query: string) => void;
	/** The `/` closed. */
	onSlashClose?: () => void;
}

export interface Composer {
	editor: Editor;
	/** The message's markdown now. */
	markdown(): string;
	/** Replace the message (an external `value` set), the caret at its end. */
	setMarkdown(markdown: string): void;
	setDisabled(disabled: boolean): void;
	/** Swap the empty-state hint: the chat's mode can change after creation. */
	/** The placeholder drawn in the empty field, and what assistive tech
	 *  reads as it (`aria-placeholder`), which defaults to the same text. */
	setPlaceholder(text: string, spoken?: string): void;
	focus(): void;
	/** Put what the ref picker chose where the `@` was. */
	insertRef(label: string, url: string): void;
	/** Put a ref where a row or tab was dropped: under the pointer when it
	 *  is over the text, otherwise at the end of the message. */
	insertRefHere(label: string, url: string, at?: { x: number; y: number }): void;
	/** Replace the message with `text`, as written (not read as markdown),
	 *  and mentions of `refs` after it, caret at the end, in one step that
	 *  one undo takes back: an accepted suggested question. */
	fill(text: string, refs: { url: string; label: string }[]): void;
	/** The `@` menu closed with nothing picked; what was typed after the `@` stays.
	 *  `refocus: false` when it closed because the focus went elsewhere. */
	closeRef(opts?: { refocus?: boolean }): void;
	/** A skill was picked: the `/` and what was typed after it leave the message. */
	takeSlash(): void;
	/** The `/` menu closed with nothing picked; the text stays. */
	closeSlash(opts?: { refocus?: boolean }): void;
	destroy(): void;
}

function inCode(state: EditorState): boolean {
	return !!state.selection.$from.parent.type.spec.code;
}

/** A new line in a code block, keeping the indent of the line the caret is on. */
function codeNewline(editor: Editor): boolean {
	const { state } = editor;
	const { $from, from, to } = state.selection;
	const before = $from.parent.textBetween(0, $from.parentOffset, '\n');
	const line = before.slice(before.lastIndexOf('\n') + 1);
	const indent = /^[ \t]*/.exec(line)?.[0] ?? '';
	editor.view.dispatch(state.tr.insertText(`\n${indent}`, from, to).scrollIntoView());
	return true;
}

function indentCode(editor: Editor): boolean {
	const { from, to } = editor.state.selection;
	editor.view.dispatch(editor.state.tr.insertText(INDENT, from, to).scrollIntoView());
	return true;
}

/** Take up to one indent off the start of the caret's line in a code block. */
function dedentCode(editor: Editor): boolean {
	const { $from } = editor.state.selection;
	const before = $from.parent.textBetween(0, $from.parentOffset, '\n');
	const lineStart = $from.start() + before.lastIndexOf('\n') + 1;
	const after = $from.parent.textBetween(lineStart - $from.start(), $from.parent.content.size, '\n');
	const spaces = /^ {1,2}/.exec(after)?.[0].length ?? 0;
	if (spaces) editor.view.dispatch(editor.state.tr.delete(lineStart, lineStart + spaces));
	return true;
}

/** Enter at the end of a line of three backticks (and a language): that paragraph becomes a code block. */
function openFence(editor: Editor): boolean {
	const { $from, empty } = editor.state.selection;
	const para = $from.parent;
	if (!empty || para.type.name !== 'paragraph' || $from.parentOffset !== para.content.size) return false;
	const m = FENCE_OPENER.exec(para.textContent);
	if (!m || para.childCount !== 1) return false;
	return editor
		.chain()
		.deleteRange({ from: $from.start(), to: $from.end() })
		.setCodeBlock(m[1] ? { language: m[1] } : undefined)
		.run();
}

/**
 * An empty last line in a quote ends the quote: the line break before it
 * comes out, and the caret goes to a new paragraph after the quote. A quote
 * of one empty paragraph is lifted out.
 */
function leaveQuote(editor: Editor): boolean {
	const { state } = editor;
	const { $from, empty } = state.selection;
	if (!empty || $from.depth < 2 || $from.node(-1).type.name !== 'blockquote') return false;
	const para = $from.parent;
	if (para.content.size === 0) return editor.commands.lift('blockquote');
	const quote = $from.node(-1);
	const lastInQuote = $from.index(-1) === quote.childCount - 1;
	if ($from.parentOffset !== para.content.size || !lastInQuote || para.lastChild?.type.name !== 'hardBreak') return false;
	const tr = state.tr.delete($from.pos - 1, $from.pos);
	const after = tr.mapping.map($from.after(-1));
	tr.insert(after, state.schema.nodes.paragraph.create());
	tr.setSelection(TextSelection.create(tr.doc, after + 1));
	editor.view.dispatch(tr.scrollIntoView());
	return true;
}

/** To-do brackets typed at the start of a list item: a message keeps them as text. */
const TODO = /^\[[ xX]\] ?/;

/**
 * The next list item, or the end of the list on an empty one. An item
 * that starts with to-do brackets (`[ ] `) starts the next with them too,
 * and one holding nothing else is empty.
 */
function nextItem(editor: Editor): boolean {
	const { $from } = editor.state.selection;
	const text = $from.parent.textContent;
	const todo = TODO.test(text) && $from.index(-1) === 0;
	if (todo && text.replace(TODO, '') === '') {
		editor.commands.deleteRange({ from: $from.start(), to: $from.end() });
	}
	if (editor.commands.splitListItem('listItem')) {
		if (todo) editor.commands.insertContent('[ ] ');
		return true;
	}
	return editor.commands.liftListItem('listItem');
}

/**
 * Shift+Enter, and Enter on a phone: the next list item (or the end of the
 * list on an empty one), a new line in code, the end of a quote on its
 * empty last line, a code block from a line of backticks; otherwise a line
 * break.
 */
function breakLine(editor: Editor): boolean {
	if (inCode(editor.state)) return codeNewline(editor);
	if (openFence(editor)) return true;
	if (editor.isActive('listItem')) return nextItem(editor);
	if (leaveQuote(editor)) return true;
	return editor.commands.setHardBreak();
}

/** A list's or a quote's sign typed at the start of a line, before its space. */
const LINE_SIGN = /^(?:([-+*])|(\d+)\.|(>))$/;

/**
 * A list's or a quote's sign typed at the start of a line after a line
 * break (`Shopping:`, Shift+Enter, `- `) starts the list or quote there, as
 * the same sign does at the start of a paragraph: the line becomes a
 * paragraph of its own and is wrapped. Only in a paragraph at the top of
 * the message.
 */
const LineSigns = Extension.create({
	name: 'lineSigns',
	addProseMirrorPlugins() {
		return [
			new Plugin({
				props: {
					handleTextInput(view, from, to, text) {
						if (text !== ' ' || from !== to) return false;
						const $from = view.state.doc.resolve(from);
						const para = $from.parent;
						if (para.type.name !== 'paragraph' || $from.depth !== 1) return false;
						let breakAt = -1;
						para.forEach((child, offset) => {
							if (child.type.name === 'hardBreak' && offset < $from.parentOffset) breakAt = offset;
						});
						if (breakAt < 0) return false;
						const m = LINE_SIGN.exec(para.textBetween(breakAt + 1, $from.parentOffset));
						if (!m) return false;
						const { schema } = view.state;
						const [type, attrs] = m[1]
							? [schema.nodes.bulletList, null]
							: m[2]
								? [schema.nodes.orderedList, { start: Number(m[2]) }]
								: [schema.nodes.blockquote, null];
						const at = $from.start() + breakAt;
						const tr = view.state.tr.delete(at, from);
						tr.split(at);
						const range = tr.doc.resolve(at + 2).blockRange();
						const wrapping = range && findWrapping(range, type, attrs);
						if (!range || !wrapping) return false;
						tr.wrap(range, wrapping);
						tr.setSelection(TextSelection.near(tr.doc.resolve(range.start + wrapping.length + 1)));
						view.dispatch(tr.scrollIntoView());
						return true;
					},
				},
			}),
		];
	},
});


/** Text as the composer takes it with no markdown read in it: its lines as they are. */
function textSlice(schema: Schema, text: string): Slice {
	const blocks = textBlocks(schema, text);
	if (blocks.childCount === 1) return new Slice(blocks.firstChild!.content, 0, 0);
	return new Slice(blocks, 1, 1);
}

/** Whether pasted text reads as markdown to the composer: a block's sign, or a mark's or a link's in a line, and no line of code. */
function pastedMarkdown(text: string): boolean {
	return looksLikeMarkdown(text) || (looksLikeMarkdown(text, { inline: true }) && !readsAsCode(text));
}

/**
 * Paste and drop. Files are attachments, and so is text past
 * `PASTE_AS_FILE_THRESHOLD`. Text in a code block goes in as it is. Text
 * that reads as markdown is converted by the page's converter and lands as
 * a page's paste does (`placeConverted`), when it holds only what a message
 * does; anything else, and a paste with Shift (Paste and Match Style), goes
 * in as its text, each line a line. A message is plain text in the end, so
 * pasted HTML is read as its text.
 */
function messagePaste(onAttach?: (files: File[]) => void): Extension {
	return Extension.create({
		name: 'messagePaste',
		addProseMirrorPlugins() {
			return [
				new Plugin({
					props: {
						handlePaste(view, event) {
							const data = event.clipboardData;
							if (!data) return false;
							const files = Array.from(data.files ?? []);
							if (files.length) {
								if (!onAttach) return false;
								event.preventDefault();
								onAttach(files);
								return true;
							}
							const text = (data.getData('text/plain') ?? '').replace(/\r\n?/g, '\n');
							if (!text) return false;
							if (onAttach && text.length > PASTE_AS_FILE_THRESHOLD) {
								event.preventDefault();
								onAttach([new File([text], 'Pasted Text.txt', { type: 'text/plain' })]);
								return true;
							}
							const { state } = view;
							if (inCode(state)) {
								view.dispatch(state.tr.insertText(text).scrollIntoView());
								return true;
							}
							const input = (view as unknown as { input?: { shiftKey?: boolean } }).input;
							if (!input?.shiftKey && pastedMarkdown(text)) {
								const result = convertNow(text, 'paste');
								if (result.kind === 'converted' && holdsOnlyMessage(result.html)) {
									const content = htmlSlice(state.schema, result.html).content;
									if (content.size) {
										const tr = state.tr.deleteSelection();
										placeConverted(tr, tr.selection.from, content);
										view.dispatch(tr.scrollIntoView());
										return true;
									}
								}
							}
							view.dispatch(state.tr.replaceSelection(textSlice(state.schema, text)).scrollIntoView());
							return true;
						},
						handleDrop(_view, event) {
							const files = Array.from(event.dataTransfer?.files ?? []);
							if (!files.length || !onAttach) return false;
							event.preventDefault();
							onAttach(files);
							return true;
						},
						// Text from elsewhere (a drop, a paste the editor makes itself) as its lines.
						clipboardTextParser(text, $context) {
							return textSlice($context.doc.type.schema, text);
						},
						// A copy out of the composer is what it would send.
						clipboardTextSerializer(slice, view) {
							const first = slice.content.firstChild;
							if (!first) return '';
							const blocks = first.isInline
								? Fragment.from(view.state.schema.nodes.paragraph.create(null, slice.content))
								: slice.content;
							return messageMarkdown(blocks);
						},
					},
				}),
			];
		},
	});
}

/**
 * The keyboard hints a phone reads off the editable. `enterkeyhint=enter`
 * because Return breaks the line here (the send button sends).
 * Autocorrect and sentence capitalization on: this is prose typed to a
 * model on glass, and the prediction strip earns its row. Spellcheck stays
 * off: the red underlines were the complaint, not the suggestions. On a
 * desktop the defaults are right and nothing is set.
 */
export function composerAttributes(mobile: boolean, label = 'Message'): Record<string, string> {
	const attrs: Record<string, string> = {
		class: 'composer-field prose prose--compact',
		'aria-label': label,
		'aria-multiline': 'true',
		role: 'textbox',
	};
	if (mobile) {
		attrs.enterkeyhint = 'enter';
		attrs.autocorrect = 'on';
		attrs.autocapitalize = 'sentences';
		attrs.spellcheck = 'false';
	}
	return attrs;
}

export function createComposer(o: ComposerOptions): Composer {
	let hint = o.placeholder ?? 'Ask Virtues';
	let mentionFrom = 0;
	let slashFrom = 0;
	// Whether the open `/` is at the start of the message, the only place it
	// means a skill; anywhere else it is a slash in a sentence.
	let slashAtStart = false;
	// How the message on screen was read, and whether it has been edited
	// since: one read as text for want of a converter is read again once
	// the converter loads, unless the person has changed it by then.
	let reading: Reading = 'text';
	let edited = false;
	let settingDoc = false;

	const submit = () => {
		o.onSubmit();
		return true;
	};

	const keys = Extension.create({
		name: 'composerKeys',
		// Ahead of the list item's, the code block's and the hard break's own.
		priority: 1000,
		addKeyboardShortcuts() {
			const editor = this.editor;
			return {
				Enter: () => {
					if (inCode(editor.state)) return false;
					if (o.isMobile()) return breakLine(editor);
					if (openFence(editor)) return true;
					return submit();
				},
				'Shift-Enter': () => breakLine(editor),
				'Mod-Enter': submit,
				Tab: () => {
					if (inCode(editor.state)) return indentCode(editor);
					if (editor.isActive('listItem')) {
						editor.commands.sinkListItem('listItem');
						return true;
					}
					return false;
				},
				'Shift-Tab': () => {
					if (inCode(editor.state)) return dedentCode(editor);
					const { $from } = editor.state.selection;
					let lists = 0;
					for (let d = $from.depth; d > 0; d--) if ($from.node(d).type.name === 'listItem') lists++;
					if (lists > 1) return editor.commands.liftListItem('listItem');
					if (!o.onShiftTab) return false;
					o.onShiftTab();
					return true;
				},
				Escape: () => {
					if (o.onEscape?.()) return true;
					editor.commands.blur();
					return true;
				},
			};
		},
	});

	const extensions = [
		...contractExtensions(
			{
				nodes: { mention: { selectable: true, addNodeView: () => svelteNodeView(MentionNode, 'span') } },
				marks: {
					// A link is made by `[label](url)` typed or pasted, never by
					// typing an address: the thread shows a message as typed.
					link: {
						addOptions() {
							return { ...this.parent?.(), autolink: false, linkOnPaste: false };
						},
					},
				},
			},
			MESSAGE,
		),
		UndoRedo,
		drawnCaret(),
		// Text dragged inside a message lands where a caret-high bar shows.
		dropLine(),
		ListKeymap,
		Placeholder.configure({ placeholder: () => hint }),
		triggers([
			{
				char: '@',
				onOpen: (_coords, from) => {
					mentionFrom = from;
					o.onMentionOpen?.(triggerState(editor.state, '@').query);
				},
				onQuery: (q) => o.onMentionQuery?.(q),
				onClose: () => o.onMentionClose?.(),
			},
			{
				char: '/',
				onOpen: (_coords, from) => {
					slashFrom = from;
					slashAtStart = from <= 1;
					if (slashAtStart) o.onSlashOpen?.(triggerState(editor.state, '/').query);
				},
				onQuery: (q) => {
					if (slashAtStart) o.onSlashQuery?.(q);
				},
				onClose: () => {
					if (slashAtStart) o.onSlashClose?.();
					slashAtStart = false;
				},
			},
		]),
		messagePaste(o.onAttach),
		LineSigns,
		keys,
	];

	const editor = new Editor({
		element: o.element,
		editable: !o.disabled,
		extensions,
		editorProps: {
			attributes: () => composerAttributes(o.isMobile(), o.label),
		},
		onUpdate: ({ editor: e }) => {
			if (!settingDoc) edited = true;
			o.onChange(messageText(e.state.doc));
		},
		onFocus: () => o.onFocusChange(true),
		onBlur: () => o.onFocusChange(false),
	});

	const measure = () => o.onHeight(editor.view.dom.getBoundingClientRect().height);
	const observer = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(measure);
	observer?.observe(editor.view.dom);
	requestAnimationFrame(() => {
		if (!editor.isDestroyed) measure();
	});

	function show(doc: PmNode) {
		settingDoc = true;
		try {
			const { state } = editor;
			const tr = state.tr.replaceWith(0, state.doc.content.size, doc.content);
			tr.setSelection(TextSelection.atEnd(tr.doc)).setMeta('addToHistory', false);
			editor.view.dispatch(tr);
		} finally {
			settingDoc = false;
		}
	}

	/** A mention (or a link, for an address out) at the caret, then a space. */
	/** A mention for a route, or a link for an address elsewhere. */
	function refNode(label: string, url: string): PmNode {
		const { schema } = editor.state;
		return isRoute(url)
			? schema.nodes.mention.create({ to: url, label })
			: schema.text(label, [schema.marks.link.create({ href: url })]);
	}

	function refAtCaret(label: string, url: string) {
		const tr = editor.state.tr.replaceSelectionWith(refNode(label, url), false);
		tr.insertText(' ');
		editor.view.dispatch(tr.scrollIntoView());
		editor.commands.focus();
	}

	function setMarkdown(markdown: string) {
		if (markdown === messageText(editor.state.doc)) return;
		const next = readMessage(editor.schema, markdown);
		reading = next.reading;
		edited = false;
		show(next.doc);
	}

	const initial = readMessage(editor.schema, o.markdown ?? '');
	reading = initial.reading;
	if (o.markdown) show(initial.doc);

	if (reading === 'unavailable') {
		void loadConverter().then((ok) => {
			if (!ok || edited || editor.isDestroyed || reading !== 'unavailable') return;
			const text = messageText(editor.state.doc);
			const next = readMessage(editor.schema, text);
			reading = next.reading;
			if (next.reading === 'converted') show(next.doc);
		});
	}

	return {
		editor,
		markdown: () => messageText(editor.state.doc),
		setMarkdown,
		setDisabled(disabled) {
			if (editor.isEditable === disabled) editor.setEditable(!disabled, false);
		},
		setPlaceholder(text, spoken = text) {
			editor.view.dom.setAttribute('aria-placeholder', spoken);
			if (text === hint) return;
			hint = text;
			// Placeholder draws on the next transaction.
			editor.view.dispatch(editor.state.tr.setMeta('addToHistory', false));
		},
		focus() {
			editor.commands.focus();
		},
		insertRef(label, url) {
			const { state } = editor;
			const caret = state.selection.from;
			const fallback = mentionFrom < caret ? { from: mentionFrom, to: mentionFrom + 1 } : { from: caret, to: caret };
			const range = pickedRange(state, '@', fallback);
			closeTrigger(editor.view, '@');
			const { schema } = editor.state;
			const node = isRoute(url)
				? schema.nodes.mention.create({ to: url, label })
				: schema.text(label, [schema.marks.link.create({ href: url })]);
			// A space after it, unless it is alone on its line.
			const $at = editor.state.doc.resolve(range.from);
			const para = $at.parent;
			const text = (from: number, to: number) =>
				para.textBetween(from, to, undefined, (leaf) => (leaf.type.name === 'hardBreak' ? '\n' : '\ufffc'));
			const end = $at.parentOffset + (range.to - range.from);
			const before = text(0, $at.parentOffset).split('\n').pop() ?? '';
			const after = text(end, para.content.size).split('\n')[0] ?? '';
			const alone = !before.trim() && !after.trim();
			const tr = editor.state.tr.replaceWith(range.from, range.to, alone ? node : [node, schema.text(' ')]);
			editor.view.dispatch(tr.scrollIntoView());
			editor.commands.focus();
		},
		insertRefHere(label, url, at) {
			const { state, view } = editor;
			const hit = at ? view.posAtCoords({ left: at.x, top: at.y }) : null;
			const pos = hit ? hit.pos : state.doc.content.size;
			view.dispatch(state.tr.setSelection(Selection.near(state.doc.resolve(pos))));
			refAtCaret(label, url);
		},
		fill(text, refs) {
			const { schema, doc } = editor.state;
			const inline: PmNode[] = [schema.text(text)];
			for (const r of refs) inline.push(schema.text(' '), refNode(r.label, r.url));
			if (refs.length > 0) inline.push(schema.text(' '));
			const tr = editor.state.tr.replaceWith(0, doc.content.size, schema.nodes.paragraph.create(null, inline));
			tr.setSelection(Selection.atEnd(tr.doc));
			editor.view.dispatch(tr.scrollIntoView());
			editor.commands.focus();
		},
		closeRef({ refocus = true } = {}) {
			closeTrigger(editor.view, '@');
			if (refocus) editor.commands.focus();
		},
		takeSlash() {
			const range = pickedRange(editor.state, '/', { from: slashFrom, to: slashFrom + 1 });
			closeTrigger(editor.view, '/');
			editor.view.dispatch(editor.state.tr.delete(range.from, range.to));
			editor.commands.focus();
		},
		closeSlash({ refocus = true } = {}) {
			closeTrigger(editor.view, '/');
			if (refocus) editor.commands.focus();
		},
		destroy() {
			observer?.disconnect();
			editor.destroy();
		},
	};
}

