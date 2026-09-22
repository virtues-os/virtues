// @vitest-environment happy-dom
import { afterEach, describe, expect, it, vi } from 'vitest';

// The header's language picker opens the app's context-menu store, a runes
// class that only loads through the Svelte compiler. Nothing here clicks it.
vi.mock('$lib/stores/contextMenu.svelte', () => ({ contextMenu: { show: vi.fn() } }));

import { createTestView, destroyTestView, forceFocus, setCursor, visibleText } from '../test-utils';
import { codeBlocks } from './code-blocks';
import { mouseFreeze } from './mouse-freeze';

import type { EditorView } from '@codemirror/view';

let view: EditorView | undefined;
afterEach(() => {
	if (view) destroyTestView(view);
	view = undefined;
});

const DOC = '```js\nconst a = 1;\n```\n\nafter';

const mount = (doc: string, selection = 0, focus = true) =>
	createTestView(doc, { selection, focus, extensions: [mouseFreeze, codeBlocks] });

describe('code blocks: header and lines', () => {
	it('adds the header with the language and classes every block line', () => {
		view = mount(DOC, DOC.length);
		expect(view.contentDOM.querySelector('.cm-code-language')?.textContent).toBe('js');
		expect(view.contentDOM.querySelectorAll('.cm-codeblock-line').length).toBe(3);
		expect(view.contentDOM.querySelector('.cm-codeblock-first')).not.toBeNull();
		expect(view.contentDOM.querySelector('.cm-codeblock-last')).not.toBeNull();
	});
});

describe('code blocks: fence reveal', () => {
	it('hides both fences when the caret is elsewhere', () => {
		view = mount(DOC, DOC.length);
		expect(visibleText(view)).not.toContain('```');
		expect(visibleText(view)).toContain('const a = 1;');
	});

	it('reveals only the fence line the caret is on, when focused', () => {
		view = mount(DOC, 2);
		expect(visibleText(view)).toContain('```js');
		// The closing fence stays hidden: one fence line at a time.
		expect(visibleText(view).match(/```/g)?.length).toBe(1);
		setCursor(view, DOC.indexOf('\n```') + 2);
		expect(visibleText(view)).not.toContain('```js');
		expect(visibleText(view)).toContain('```');
	});

	it('does not reveal inside the block', () => {
		view = mount(DOC, DOC.indexOf('const') + 2);
		expect(visibleText(view)).not.toContain('```');
	});

	it('does not reveal without focus, and follows focus both ways', () => {
		view = mount(DOC, 2, false);
		expect(visibleText(view)).not.toContain('```');
		forceFocus(view, true);
		expect(visibleText(view)).toContain('```js');
		forceFocus(view, false);
		expect(visibleText(view)).not.toContain('```');
	});

	it('a read-only view with its selection at 0 shows no fence', () => {
		view = mount(DOC, 0, false);
		expect(visibleText(view)).not.toContain('```');
	});
});
