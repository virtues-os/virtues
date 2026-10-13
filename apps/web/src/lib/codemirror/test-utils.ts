/**
 * Test harness for the composer's extensions
 *
 * Mounts a real EditorView under happy-dom (`// @vitest-environment happy-dom`
 * at the top of each test file) with the same markdown language the composer
 * uses, so a test asserts against what the extension actually does rather
 * than against a decoration set it has to interpret itself.
 *
 * What happy-dom cannot do: layout. Every rect is 0×0, so anything measured
 * (coordsAtPos, wrapped-line geometry) is out of scope here and belongs to a
 * browser check. Decorations, classes, keymaps and transactions are fair game.
 *
 * A view comes back with its document parsed to the end. A state's first
 * parse stops when its time slice runs out (20ms of the clock, in
 * @codemirror/language) and the rest is parsed in idle callbacks; a test
 * that read the result of that first parse passed or failed with the
 * machine's load.
 */

import { forceParsing } from '@codemirror/language';
import { EditorSelection, EditorState, type Extension } from '@codemirror/state';
import { EditorView } from '@codemirror/view';

import { virtuesMarkdown } from './language';

export interface TestViewOptions {
	/** Caret position. Default: 0 (the same as a freshly opened doc). */
	selection?: number;
	/** Extra extensions, installed after the markdown language. */
	extensions?: Extension[];
}

/** Mount a focused view in `document.body`. Release it with `destroyTestView`. */
export function createTestView(doc: string, options: TestViewOptions = {}): EditorView {
	const { selection = 0, extensions = [] } = options;
	const parent = document.createElement('div');
	document.body.appendChild(parent);
	const view = new EditorView({
		parent,
		state: EditorState.create({
			doc,
			selection: EditorSelection.single(selection),
			extensions: [virtuesMarkdown(), EditorView.lineWrapping, ...extensions],
		}),
	});
	forceFocus(view);
	// Last, so what the extensions draw from the whole tree is drawn on
	// the transaction that delivers it, as the idle parse delivers it.
	forceParsing(view, view.state.doc.length, 60_000);
	return view;
}

/**
 * Focus for tests. `hasFocus` is `document.hasFocus() && activeElement ==
 * contentDOM`, so focusing the element is enough, but a focused view also
 * writes the DOM selection on every update, and happy-dom fires
 * `selectionchange` synchronously from that write, re-entering CodeMirror
 * while its update is still in progress ("Calls to EditorView.update are not
 * allowed while an update is in progress"). A real browser fires that event
 * in a later task. The DOM selection is never what a test asserts on, so the
 * write is stubbed out for the life of the view.
 */
export function forceFocus(view: EditorView, focused = true): void {
	const docView = (view as unknown as { docView: { updateSelection: () => void } }).docView;
	docView.updateSelection = () => {};
	if (focused) view.contentDOM.focus();
	else view.contentDOM.blur();
	// Deliver the focusChanged update the browser event would have produced.
	view.dispatch({});
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
