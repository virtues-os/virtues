/**
 * Test harness for the editor's extensions
 *
 * Mounts a real EditorView under happy-dom (`// @vitest-environment happy-dom`
 * at the top of each test file) with the same markdown language the app uses,
 * so a test asserts against the DOM the extension actually produces —
 * `contentDOM.querySelector('.cm-heading-1')` — rather than against a
 * decoration set it has to interpret itself. This is the pattern the
 * @codemirror/view suite and the OSS live-preview editors (Joplin,
 * atomic-editor) all use.
 *
 * What happy-dom cannot do: layout. Every rect is 0×0, so anything measured
 * (coordsAtPos, wrapped-line geometry, widget heights) is out of scope here
 * and belongs to a browser check. Decoration presence, classes, hidden text,
 * widgets, keymaps and transactions are all fair game.
 *
 * A view comes back with its document parsed to the end. A state's first
 * parse stops when its time slice runs out (20ms of the clock, in
 * @codemirror/language) and the rest is parsed in idle callbacks; a test
 * that read the decorations of that first parse passed or failed with the
 * machine's load. `partlyParsedView` makes the short first parse on purpose.
 */

import { defaultKeymap } from '@codemirror/commands';
import { markdownKeymap } from '@codemirror/lang-markdown';
import { forceParsing } from '@codemirror/language';
import { EditorSelection, EditorState, type Extension } from '@codemirror/state';
import { EditorView, keymap } from '@codemirror/view';

import { virtuesMarkdown } from './language';

export interface TestViewOptions {
	/** Caret position, or a range. Default: 0 (the same as a freshly opened doc). */
	selection?: number | { anchor: number; head: number };
	/** Give the editor focus so reveal-on-touch fires. Default: true. */
	focus?: boolean;
	/** Extra extensions, installed after the markdown language. */
	extensions?: Extension[];
	/** Include the app's markdown + default keymaps (for Enter/Backspace tests). */
	keymaps?: boolean;
}

/** Mount a view in `document.body`. Call `view.destroy()` when done. */
export function createTestView(doc: string, options: TestViewOptions = {}): EditorView {
	const { selection = 0, focus = true, extensions = [], keymaps = false } = options;
	const parent = document.createElement('div');
	document.body.appendChild(parent);
	const view = new EditorView({
		parent,
		state: EditorState.create({
			doc,
			selection: initialSelection(selection),
			extensions: [
				virtuesMarkdown(),
				EditorView.lineWrapping,
				...(keymaps ? [keymap.of([...markdownKeymap, ...defaultKeymap])] : []),
				...extensions,
			],
		}),
	});
	if (focus) forceFocus(view);
	// Last, so what the extensions draw from the whole tree is drawn on
	// the transaction that delivers it, as the idle parse delivers it.
	forceParsing(view, view.state.doc.length, 60_000);
	return view;
}

/**
 * A view whose first parse ran out of time after one step, as it does on a
 * long page or a busy machine: its tree ends part way down the document
 * until `forceParsing` (or the idle parse) delivers the rest.
 */
export function partlyParsedView(doc: string, options: TestViewOptions = {}): EditorView {
	const { selection = 0, focus = true, extensions = [] } = options;
	const realNow = Date.now;
	let now = realNow();
	// Every reading of the clock is a second later, so the slice is spent
	// after its first step.
	Date.now = () => (now += 1000);
	let state: EditorState;
	try {
		state = EditorState.create({
			doc,
			selection: initialSelection(selection),
			extensions: [virtuesMarkdown(), EditorView.lineWrapping, ...extensions],
		});
	} finally {
		Date.now = realNow;
	}
	const parent = document.createElement('div');
	document.body.appendChild(parent);
	const view = new EditorView({ parent, state });
	if (focus) forceFocus(view);
	return view;
}

function initialSelection(selection: NonNullable<TestViewOptions['selection']>): EditorSelection {
	return typeof selection === 'number'
		? EditorSelection.single(selection)
		: EditorSelection.single(selection.anchor, selection.head);
}

/**
 * Focus for tests. `hasFocus` is `document.hasFocus() && activeElement ==
 * contentDOM`, so focusing the element is enough for reveal-on-touch — but
 * a focused view also writes the DOM selection on every update, and happy-dom
 * fires `selectionchange` synchronously from that write, re-entering
 * CodeMirror while its update is still in progress ("Calls to
 * EditorView.update are not allowed while an update is in progress"). A real
 * browser fires that event in a later task. The DOM selection is never what a
 * test asserts on, so the write is stubbed out for the life of the view.
 */
export function forceFocus(view: EditorView, focused = true): void {
	const docView = (view as unknown as { docView: { updateSelection: () => void } }).docView;
	docView.updateSelection = () => {};
	if (focused) view.contentDOM.focus();
	else view.contentDOM.blur();
	// Deliver the focusChanged update the browser event would have produced.
	view.dispatch({});
}

/** Move the caret and let the extensions rebuild. */
export function setCursor(view: EditorView, pos: number): void {
	view.dispatch({ selection: EditorSelection.cursor(pos) });
}

/** Select a range. */
export function setSelection(view: EditorView, anchor: number, head: number): void {
	view.dispatch({ selection: EditorSelection.range(anchor, head) });
}

/** The rendered text of the content DOM, which is what the reader sees. */
export function visibleText(view: EditorView): string {
	return view.contentDOM.textContent ?? '';
}

/** The `.cm-line` elements, in order. */
export function lines(view: EditorView): HTMLElement[] {
	return Array.from(view.contentDOM.querySelectorAll<HTMLElement>('.cm-line'));
}

/** Release a mounted view and its parent node. */
export function destroyTestView(view: EditorView): void {
	const parent = view.dom.parentElement;
	view.destroy();
	parent?.remove();
}
