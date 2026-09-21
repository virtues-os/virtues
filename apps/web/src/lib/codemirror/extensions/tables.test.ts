// @vitest-environment happy-dom
import { deleteCharBackward } from '@codemirror/commands';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { createTestView, destroyTestView } from '../test-utils';
import { backspaceAtTableBoundary, parseCells, parseTable, serializeCell, serializeTable, tables } from './tables';

import type { EditorView } from '@codemirror/view';

// The right-click menu is a Svelte-runes store; nothing here opens it, and
// loading it would need the Svelte compiler in the test pipeline.
vi.mock('$lib/stores/contextMenu.svelte', () => ({ contextMenu: { show: vi.fn(), hide: vi.fn() } }));

// widget-height.ts observes every table wrapper; happy-dom ships a
// ResizeObserver, but a bare one is enough if a future version drops it.
if (typeof globalThis.ResizeObserver === 'undefined') {
	globalThis.ResizeObserver = class {
		observe() {}
		unobserve() {}
		disconnect() {}
	} as unknown as typeof ResizeObserver;
}

let view: EditorView | undefined;
afterEach(() => {
	if (view) destroyTestView(view);
	view = undefined;
});

const TABLE = '| a | b |\n| --- | --- |\n| 1 | 2 |';

const mount = (doc: string, selection = 0, keymaps = false) =>
	createTestView(doc, { selection, keymaps, extensions: [tables] });

const cells = (v: EditorView) => Array.from(v.contentDOM.querySelectorAll<HTMLElement>('.cm-table-widget td'));

const keydown = (el: Element, key: string, init: KeyboardEventInit = {}) =>
	el.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, ...init }));

describe('tables: pipe escaping', () => {
	it('reads `\\|` as a literal pipe in the cell', () => {
		expect(parseCells('| a \\| b | c |')).toEqual(['a | b', 'c']);
	});

	it('a trailing `\\|` ends the last cell, it is not the closing pipe', () => {
		expect(parseCells('| a | b \\|')).toEqual(['a', 'b |']);
	});

	it('keeps an empty cell as an empty cell', () => {
		expect(parseCells('| a |  | c |')).toEqual(['a', '', 'c']);
		expect(parseCells('| a | b | |')).toEqual(['a', 'b', '']);
	});

	it('serializes a literal pipe as `\\|` and a newline as a space', () => {
		expect(serializeCell('x|y')).toBe('x\\|y');
		expect(serializeCell('x\ny')).toBe('x y');
	});

	it('round-trips escaped pipes, blank columns and a backslash before a pipe', () => {
		const headers = ['a | b', '', 'c'];
		const alignments = ['left', 'center', 'right'] as const;
		const rows = [
			['', 'x|y', 'z'],
			['q\\|r', '', ''],
		];
		const src = serializeTable(headers, [...alignments], rows);
		expect(src).toBe('| a \\| b |  | c |\n| --- | :---: | ---: |\n|  | x\\|y | z |\n| q\\\\|r |  |  |');
		expect(parseTable(src)).toEqual({ headers, alignments: [...alignments], rows });
	});
});

describe('tables: the rendered cell', () => {
	it('shows a plain pipe for `\\|` in the source', () => {
		view = mount('| a \\| b | c |\n| --- | --- |\n| 1 \\| 2 | 3 |', 0);
		const th = view.contentDOM.querySelector('.cm-table-widget th');
		expect(th?.textContent).toBe('a | b');
		expect(cells(view)[0].textContent).toBe('1 | 2');
	});

	it('a pipe typed into a cell serializes as `\\|`', () => {
		view = mount(TABLE, 0);
		const td = cells(view)[0];
		td.focus();
		td.textContent = 'x|y';
		// Escape commits the widget's cells to the document synchronously.
		keydown(td, 'Escape');
		expect(view.state.doc.toString()).toBe('| a | b |\n| --- | --- |\n| x\\|y | 2 |');
		// And the re-rendered cell reads back as the pipe, not the escape.
		expect(cells(view)[0].textContent).toBe('x|y');
	});

	it('is wrapper > scroll box > table, with the controls on the wrapper', () => {
		view = mount(TABLE, 0);
		const wrapper = view.contentDOM.querySelector<HTMLElement>('.cm-table-wrapper');
		const scroll = wrapper?.querySelector<HTMLElement>(':scope > .cm-table-scroll');
		expect(scroll?.querySelector(':scope > table.cm-table-widget')).not.toBeNull();
		for (const cls of ['cm-table-col-controls', 'cm-table-row-controls', 'cm-table-add-row-strip', 'cm-table-add-col-strip']) {
			expect(wrapper?.querySelector(`.${cls}`)?.parentElement).toBe(wrapper);
		}
	});
});

describe('tables: paste and IME in a cell', () => {
	it('paste flattens to one line of plain text', () => {
		view = mount(TABLE, 0);
		const td = cells(view)[0];
		td.textContent = '';
		const ev = new Event('paste', { bubbles: true, cancelable: true });
		Object.defineProperty(ev, 'clipboardData', {
			value: { getData: (type: string) => (type === 'text/plain' ? 'one\ntwo  <b>three</b>' : '<b>html</b>') },
		});
		td.dispatchEvent(ev);
		expect(ev.defaultPrevented).toBe(true);
		expect(td.textContent).toBe('one two <b>three</b>');
		expect(Array.from(td.childNodes).every((n) => n.nodeType === Node.TEXT_NODE)).toBe(true);
	});

	it('the key handler stands down while a composition is open', () => {
		view = mount(TABLE, 0);
		const td = cells(view)[0];
		td.focus();
		td.textContent = 'zz';
		td.dispatchEvent(new Event('compositionstart', { bubbles: true }));
		// Escape would otherwise sync the cell and move focus out of it.
		keydown(td, 'Escape');
		expect(view.state.doc.toString()).toBe(TABLE);
		td.dispatchEvent(new Event('compositionend', { bubbles: true }));
		keydown(td, 'Escape');
		expect(view.state.doc.toString()).toBe('| a | b |\n| --- | --- |\n| zz | 2 |');
	});

	it('an isComposing keydown is ignored even without composition events', () => {
		view = mount(TABLE, 0);
		const td = cells(view)[0];
		td.textContent = 'zz';
		keydown(td, 'Escape', { isComposing: true });
		expect(view.state.doc.toString()).toBe(TABLE);
	});
});

describe('tables: Backspace after a table', () => {
	// Most cases call the command directly. The last one drives two real
	// keydowns through the editor with the app's keymaps installed, which
	// happy-dom does deliver: it proves the binding beats deleteCharBackward
	// on the first press and yields to it on the second.
	const DOC = TABLE + '\n\nafter';
	const end = TABLE.length;

	it('selects the table from the start of the line below it', () => {
		view = mount(DOC, end + 1);
		expect(backspaceAtTableBoundary(view)).toBe(true);
		expect(view.state.selection.main.from).toBe(0);
		expect(view.state.selection.main.to).toBe(end);
		expect(view.state.doc.toString()).toBe(DOC);
	});

	it('selects the table from its own end position', () => {
		view = mount(DOC, end);
		expect(backspaceAtTableBoundary(view)).toBe(true);
		expect([view.state.selection.main.from, view.state.selection.main.to]).toEqual([0, end]);
	});

	it('a second Backspace deletes the selected table', () => {
		view = mount(DOC, end + 1);
		backspaceAtTableBoundary(view);
		// The guard stands aside for a non-empty selection …
		expect(backspaceAtTableBoundary(view)).toBe(false);
		// … and the default deletes it.
		expect(deleteCharBackward(view)).toBe(true);
		expect(view.state.doc.toString()).toBe('\n\nafter');
		expect(view.state.selection.main.head).toBe(0);
	});

	it('from the paragraph below, steps onto the blank line instead of joining', () => {
		view = mount(DOC, end + 2);
		expect(backspaceAtTableBoundary(view)).toBe(true);
		expect(view.state.doc.toString()).toBe(DOC);
		expect(view.state.selection.main.head).toBe(end + 1);
	});

	it('does nothing away from a table', () => {
		view = mount(DOC, DOC.length);
		expect(backspaceAtTableBoundary(view)).toBe(false);
		view = (destroyTestView(view), mount('plain\n\ntext', 7));
		expect(backspaceAtTableBoundary(view)).toBe(false);
	});

	it('two Backspace keys: the first selects, the second deletes', () => {
		view = mount(DOC, end + 1, true);
		expect(keydown(view.contentDOM, 'Backspace', { keyCode: 8 })).toBe(false); // handled
		expect([view.state.selection.main.from, view.state.selection.main.to]).toEqual([0, end]);
		expect(view.state.doc.toString()).toBe(DOC);
		expect(keydown(view.contentDOM, 'Backspace', { keyCode: 8 })).toBe(false);
		expect(view.state.doc.toString()).toBe('\n\nafter');
	});
});
