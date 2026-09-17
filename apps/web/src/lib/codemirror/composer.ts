/**
 * Chat composer — a second, smaller configuration of the page editor's kernel.
 *
 * The composer used to be a hand-rolled `contenteditable` with a tree walker
 * that flattened mention pills back into markdown on send. That was the one
 * text surface in the product that did not run on CodeMirror, and it paid for
 * it: no undo, a saved-text-node hack for `@`, line structure living in the
 * DOM instead of in `\n` (VIR-333), and no list continuation (VIR-328).
 *
 * Here the document IS the message — a markdown string, exactly what the
 * model receives — and the extensions Pages already carry do the rest:
 * `entityLinks` renders `[label](url)` as a pill, `createRefPicker` detects
 * `@`, `markdownKeymap` continues lists on Shift+Enter, `listRenumber` keeps
 * numbers sequential. What is deliberately NOT here: Yjs, live preview of
 * headings/emphasis, slash commands. A message box that renders while you
 * type reads as a word processor. See agents/record/editor-kernel-2026-09-14.md.
 */

import { defaultKeymap, history, historyKeymap, insertNewline } from '@codemirror/commands';
import { markdown, markdownKeymap } from '@codemirror/lang-markdown';
import { syntaxTree } from '@codemirror/language';
import { GFM } from '@lezer/markdown';
import { Compartment, EditorState, type Extension, Prec } from '@codemirror/state';
import { EditorView, keymap, placeholder as cmPlaceholder } from '@codemirror/view';

import { smoothCaret } from './extensions/caret';
import { markdownKeybindings } from './extensions/keybindings';
import { listRenumber } from './extensions/list-renumber';
import { mouseFreeze } from './extensions/mouse-freeze';
import { entityLinks } from './extensions/ref-links';
import { virtuesTheme } from './theme';

/** Pasted text longer than this becomes an attachment rather than a wall in
 *  the composer — the same threshold the contenteditable used. */
const PASTE_AS_FILE_THRESHOLD = 1500;

/** A fence line: three or more backticks or tildes, optionally an info string. */
const FENCE_LINE = /^\s*(`{3,}|~{3,})/;

/**
 * Is the cursor inside a code fence, where Enter must mean "new line"?
 *
 * Verified failure without this: type three backticks, a line of code, press
 * Enter — and the half-written fence went to the model, because Enter is Send
 * on a desktop. Inside a fence the keys behave like a code editor: Enter and
 * Shift+Enter both break the line, Tab indents, and nothing continues a list.
 * Cmd+Enter still sends from anywhere, so a fence is never a trap.
 *
 * The cursor at the very end of a CLOSED fence (right after the closing
 * backticks) counts as outside: the writer just finished the block and Enter
 * should send. An unclosed fence runs to the end of the document, so the
 * cursor at the end of the text is still inside it — which is exactly the
 * case that used to send.
 */
export function inFencedCode(state: EditorState): boolean {
	const pos = state.selection.main.head;
	// The node type lives in @lezer/common, which is not a direct dependency;
	// it is derived from the call instead of imported.
	let node: ReturnType<ReturnType<typeof syntaxTree>['resolveInner']> | null =
		syntaxTree(state).resolveInner(pos, -1);
	for (; node; node = node.parent) {
		if (node.name !== 'FencedCode' && node.name !== 'CodeBlock') continue;
		if (pos < node.to) return true;
		// At the fence's end: inside only if it never closed. A closing fence
		// is a line of nothing but the fence characters.
		const first = state.doc.lineAt(node.from);
		const last = state.doc.lineAt(node.to);
		const closed = last.number > first.number && /^(`{3,}|~{3,})$/.test(last.text.trim());
		return !closed;
	}
	return false;
}

/**
 * Close a fence the writer left open, so the message is well-formed markdown
 * for every renderer that will ever see it. Walks lines the way CommonMark
 * does: a fence opens on a backtick or tilde run and closes on a run of the
 * same character at least as long.
 */
export function closeOpenFence(text: string): string {
	let open: string | null = null;
	for (const line of text.split('\n')) {
		const m = FENCE_LINE.exec(line);
		if (!m) continue;
		const run = m[1];
		if (open === null) open = run;
		else if (run[0] === open[0] && run.length >= open.length && line.trim() === run) open = null;
	}
	return open === null ? text : `${text}\n${open}`;
}

export interface ComposerOptions {
	parent: HTMLElement;
	doc?: string;
	placeholder?: string;
	disabled?: boolean;
	/**
	 * Read at keystroke time, not creation time: the layout store can flip on
	 * resize. On a phone Return inserts a newline and the send button sends —
	 * what every messaging app and every AI app does. On a desktop Enter
	 * sends and Shift+Enter breaks the line.
	 */
	isMobile: () => boolean;
	onSubmit: () => void;
	onChange: (doc: string) => void;
	onFocusChange: (focused: boolean) => void;
	/** Called with the content height in px whenever it may have changed. */
	onHeight: (px: number) => void;
	/** Files pasted or dropped, and long pasted text. Absent → default paste. */
	onAttach?: (files: File[]) => void;
	/** Escape with nothing else to close. Return true if handled. */
	onEscape?: () => boolean;
	extensions?: Extension[];
}

export interface ComposerEditor {
	view: EditorView;
	setDisabled(disabled: boolean): void;
	/** Replace the whole document (external `value` set). */
	setDoc(doc: string): void;
	destroy(): void;
}

// Raised precedence: theme rules from a higher-precedence extension land later
// in the sheet and win. At plain precedence virtuesTheme's 8px content padding
// beat these and a one-line composer measured 42px instead of 28.
const composerTheme = Prec.high(EditorView.theme({
	'&': {
		// Match the old `.chat-input` box: the composer is body text, not the
		// page editor's 1.7 reading rhythm.
		fontSize: '1rem',
		lineHeight: '1.5',
	},
	'& .cm-content': {
		padding: '0.125rem 0',
	},
	'& .cm-line': {
		padding: '0 0.25rem',
	},
	'& .cm-placeholder': {
		color: 'var(--color-foreground-subtle)',
	},
}));

export function createComposerEditor(options: ComposerOptions): ComposerEditor {
	const {
		parent,
		doc = '',
		placeholder = 'Ask Virtues',
		disabled = false,
		isMobile,
		onSubmit,
		onChange,
		onFocusChange,
		onHeight,
		onAttach,
		onEscape,
		extensions: extraExtensions = [],
	} = options;

	const editable = new Compartment();
	const contentAttrs = new Compartment();

	function submit(): boolean {
		onSubmit();
		return true;
	}

	// Shift+Enter: continue the surrounding list/quote when there is one
	// (bullets repeat, numbers increment, an empty item exits the list), a
	// plain newline otherwise. `insertNewlineContinueMarkup` is the first entry
	// of markdownKeymap's Enter binding; calling markdownKeymap's commands by
	// name keeps this in step with whatever Pages bind.
	const continueOrBreak = (view: EditorView): boolean => {
		// Code is literal: no list or quote continuation inside a fence.
		if (inFencedCode(view.state)) return insertNewline(view);
		for (const binding of markdownKeymap) {
			if (binding.key === 'Enter' && binding.run && binding.run(view)) return true;
		}
		return insertNewline(view);
	};

	// Inside a fence Tab indents like a code editor. Outside, the shared
	// markdown keybindings claim Tab only on list lines and otherwise leave it
	// to move focus, which is what an accessible text box does.
	const FENCE_INDENT = '  ';
	const indentInFence = (view: EditorView): boolean => {
		if (!inFencedCode(view.state)) return false;
		view.dispatch(view.state.replaceSelection(FENCE_INDENT), { userEvent: 'input.indent' });
		return true;
	};
	const dedentInFence = (view: EditorView): boolean => {
		if (!inFencedCode(view.state)) return false;
		const line = view.state.doc.lineAt(view.state.selection.main.head);
		const spaces = /^ {1,2}/.exec(line.text)?.[0].length ?? 0;
		if (spaces === 0) return true;
		view.dispatch({ changes: { from: line.from, to: line.from + spaces }, userEvent: 'delete.dedent' });
		return true;
	};

	const composerKeymap = Prec.highest(
		keymap.of([
			{
				key: 'Enter',
				run: (view) =>
					isMobile() || inFencedCode(view.state) ? continueOrBreak(view) : submit(),
			},
			{ key: 'Shift-Enter', run: continueOrBreak },
			{ key: 'Mod-Enter', run: submit },
			{ key: 'Tab', run: indentInFence, shift: dedentInFence },
			{
				key: 'Escape',
				run: (view) => {
					if (onEscape?.()) return true;
					view.contentDOM.blur();
					return true;
				},
			},
		]),
	);

	const pasteAndDrop = EditorView.domEventHandlers({
		paste(event) {
			if (!onAttach) return false;
			const dt = event.clipboardData;
			if (!dt) return false;

			const files: File[] = [];
			for (const item of Array.from(dt.items ?? [])) {
				if (item.kind === 'file') {
					const f = item.getAsFile();
					if (f) files.push(f);
				}
			}
			if (files.length > 0) {
				event.preventDefault();
				onAttach(files);
				return true;
			}

			const text = dt.getData('text/plain') ?? '';
			if (text.length > PASTE_AS_FILE_THRESHOLD) {
				event.preventDefault();
				onAttach([new File([text], 'Pasted Text.txt', { type: 'text/plain' })]);
				return true;
			}
			// Ordinary text: CodeMirror's own paste handling inserts it.
			return false;
		},
		drop(event) {
			if (!onAttach) return false;
			const files = Array.from(event.dataTransfer?.files ?? []);
			if (files.length === 0) return false;
			event.preventDefault();
			onAttach(files);
			return true;
		},
		focus() {
			onFocusChange(true);
			return false;
		},
		blur() {
			onFocusChange(false);
			return false;
		},
	});

	const view = new EditorView({
		parent,
		state: EditorState.create({
			doc,
			extensions: [
				history(),
				// GFM so list/task/strikethrough context resolves for the
				// continue-markup and formatting commands. No code languages:
				// nothing is highlighted in a message box.
				markdown({ extensions: GFM }),
				EditorView.lineWrapping,
				smoothCaret,
				composerKeymap,
				keymap.of([...historyKeymap, ...markdownKeymap, ...defaultKeymap]),
				markdownKeybindings,
				listRenumber,
				mouseFreeze,
				entityLinks,
				virtuesTheme,
				composerTheme,
				cmPlaceholder(placeholder),
				pasteAndDrop,
				editable.of(EditorView.editable.of(!disabled)),
				contentAttrs.of(EditorView.contentAttributes.of(contentAttributes(isMobile()))),
				EditorView.updateListener.of((update) => {
					if (update.docChanged) onChange(update.state.doc.toString());
					if (update.docChanged || update.geometryChanged || update.viewportChanged) {
						onHeight(update.view.contentHeight);
					}
				}),
				...extraExtensions,
			],
		}),
	});

	// First measurement, once the view has laid out.
	view.requestMeasure({
		read: (v) => v.contentHeight,
		write: (h) => onHeight(h),
	});

	let lastMobile = isMobile();

	return {
		view,
		setDisabled(next) {
			view.dispatch({ effects: editable.reconfigure(EditorView.editable.of(!next)) });
		},
		setDoc(next) {
			if (next === view.state.doc.toString()) return;
			view.dispatch({
				changes: { from: 0, to: view.state.doc.length, insert: next },
				selection: { anchor: next.length },
			});
			// The layout store may have flipped since creation; refresh the
			// keyboard hints the same time the host touches the document.
			const mobile = isMobile();
			if (mobile !== lastMobile) {
				lastMobile = mobile;
				view.dispatch({
					effects: contentAttrs.reconfigure(EditorView.contentAttributes.of(contentAttributes(mobile))),
				});
			}
		},
		destroy() {
			view.destroy();
		},
	};
}

/**
 * The keyboard hints a phone reads off the editable.
 *
 * `enterkeyhint=enter` because Return inserts a newline here (the send button
 * sends). Autocorrect and sentence capitalization ON: this is prose typed to a
 * model on glass, and the prediction strip earns its row. Spellcheck stays off
 * — the red underlines were the complaint, not the suggestions. On desktop the
 * defaults are right and nothing is set.
 */
function contentAttributes(mobile: boolean): Record<string, string> {
	const attrs: Record<string, string> = {
		'aria-label': 'Message',
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
