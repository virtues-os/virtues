// @vitest-environment happy-dom
import { afterEach, describe, expect, it } from 'vitest';

import { createTestView, destroyTestView, forceFocus, lines, setCursor, visibleText } from '../test-utils';
import { livePreview } from './live-preview';
import { mouseFreeze } from './mouse-freeze';

import type { EditorView } from '@codemirror/view';

let view: EditorView | undefined;
afterEach(() => {
	if (view) destroyTestView(view);
	view = undefined;
});

const mount = (doc: string, selection = 0) =>
	createTestView(doc, { selection, extensions: [mouseFreeze, livePreview] });

describe('live preview: headings', () => {
	it('styles the line and hides the marker when the caret is elsewhere', () => {
		view = mount('# Title\n\nbody', 10);
		expect(lines(view)[0].classList.contains('cm-heading-1')).toBe(true);
		expect(visibleText(view)).not.toContain('#');
	});

	it('hangs the marker in the margin on the caret line, without inline text', () => {
		view = mount('# Title\n\nbody', 2);
		const mark = view.contentDOM.querySelector('.cm-heading-mark');
		expect(mark?.textContent).toBe('#');
		// The widget is the only place the `#` appears.
		expect(view.contentDOM.querySelectorAll('.cm-heading-mark').length).toBe(1);
	});
});

describe('live preview: horizontal rule', () => {
	it('is always the rule, never the dashes', () => {
		view = mount('a\n\n---\n\nb', 4);
		expect(view.contentDOM.querySelector('hr.cm-hr-widget')).not.toBeNull();
		expect(visibleText(view)).not.toContain('---');
	});
});

describe('live preview: rebuild triggers', () => {
	it('re-renders when the caret moves onto a heading', () => {
		view = mount('# Title\n\nbody', 10);
		expect(view.contentDOM.querySelector('.cm-heading-mark')).toBeNull();
		setCursor(view, 1);
		expect(view.contentDOM.querySelector('.cm-heading-mark')).not.toBeNull();
	});

	it('the harness can drive focus both ways', () => {
		view = mount('**bold** text', 3);
		expect(view.hasFocus).toBe(true);
		forceFocus(view, false);
		expect(view.hasFocus).toBe(false);
	});
});

describe('live preview: focus gate', () => {
	it('a heading mark needs focus, not just the caret', () => {
		view = mount('# Title\n\nbody', 2);
		expect(view.contentDOM.querySelector('.cm-heading-mark')).not.toBeNull();
		forceFocus(view, false);
		expect(view.contentDOM.querySelector('.cm-heading-mark')).toBeNull();
		expect(visibleText(view)).not.toContain('#');
		forceFocus(view, true);
		expect(view.contentDOM.querySelector('.cm-heading-mark')).not.toBeNull();
	});

	it('a blockquote > needs focus', () => {
		view = mount('> quoted', 3);
		expect(visibleText(view)).toContain('>');
		forceFocus(view, false);
		expect(visibleText(view)).not.toContain('>');
		expect(lines(view)[0].classList.contains('cm-blockquote-line')).toBe(true);
	});

	it('a bullet needs focus to show its raw marker', () => {
		view = mount('- item', 3);
		expect(view.contentDOM.querySelector('.cm-list-marker-raw')?.textContent).toBe('-');
		forceFocus(view, false);
		expect(view.contentDOM.querySelector('.cm-list-marker-raw')).toBeNull();
		expect(view.contentDOM.querySelector('.cm-list-marker-bullet')?.textContent).toBe('•');
	});

	it('the read-only case: unfocused, selection at 0, line one is rendered', () => {
		view = createTestView('# Title\n- item', { selection: 0, focus: false, extensions: [mouseFreeze, livePreview] });
		expect(visibleText(view)).not.toContain('#');
		expect(view.contentDOM.querySelector('.cm-heading-mark')).toBeNull();
		expect(view.contentDOM.querySelector('.cm-list-marker-bullet')).not.toBeNull();
	});
});

describe('live preview: lists', () => {
	const depthOf = (line: HTMLElement) =>
		Array.from(line.classList).find((c) => c.startsWith('cm-list-depth-'));

	it('nested bullets get depth classes from the tree', () => {
		const doc = '- a\n  - b\n    - c\n- d';
		view = mount(doc, doc.length);
		const ls = lines(view);
		expect(ls.map(depthOf)).toEqual([
			'cm-list-depth-0',
			'cm-list-depth-1',
			'cm-list-depth-2',
			'cm-list-depth-0',
		]);
		expect(ls.every((l) => l.classList.contains('cm-list-line'))).toBe(true);
	});

	it('an ordered item nests at the marker width, not at two spaces', () => {
		const doc = '1. one\n2. two\n   1. nested\n\nend';
		view = mount(doc, doc.length);
		expect(lines(view).slice(0, 3).map(depthOf)).toEqual([
			'cm-list-depth-0',
			'cm-list-depth-0',
			'cm-list-depth-1',
		]);
		const markers = Array.from(view.contentDOM.querySelectorAll('.cm-list-marker-ordered'));
		expect(markers.map((m) => m.textContent)).toEqual(['1.', '2.', '1.']);
	});

	it('the marker, its space and the indent are one widget, out of the text', () => {
		const doc = '- a\n  - b\n\nend';
		view = mount(doc, doc.length);
		expect(visibleText(view)).toBe('•a◦bend');
		const ls = lines(view).slice(0, 2);
		// One widget per item line, and it is the line's first child (after
		// the zero-width buffer CodeMirror puts around every widget).
		for (const l of ls) {
			expect(l.querySelectorAll('.cm-list-marker').length).toBe(1);
			const first = Array.from(l.children).find((c) => !c.classList.contains('cm-widgetBuffer'));
			expect(first?.classList.contains('cm-list-marker')).toBe(true);
		}
	});

	it('bullet glyphs follow depth', () => {
		const doc = '- a\n  - b\n    - c\n\nend';
		view = mount(doc, doc.length);
		const glyphs = Array.from(view.contentDOM.querySelectorAll('.cm-list-marker-bullet'));
		expect(glyphs.map((g) => g.textContent)).toEqual(['•', '◦', '▪']);
	});

	it('the caret line shows the raw marker in the same widget, nothing else moves', () => {
		const doc = '- a\n* b\n1. c';
		view = mount(doc, 5);
		const ls = lines(view);
		expect(ls[0].querySelector('.cm-list-marker')?.textContent).toBe('•');
		const raw = ls[1].querySelector('.cm-list-marker');
		expect(raw?.classList.contains('cm-list-marker-raw')).toBe(true);
		expect(raw?.textContent).toBe('*');
		expect(ls[2].querySelector('.cm-list-marker')?.textContent).toBe('1.');
		// The marker text lives only in the widget; the line text is bare.
		expect(visibleText(view)).toBe('•a*b1.c');
		setCursor(view, doc.length);
		expect(ls[2].querySelector('.cm-list-marker-raw')?.textContent).toBe('1.');
	});

	it('depth classes cap at 5', () => {
		const doc = ['- a', '  - b', '    - c', '      - d', '        - e', '          - f', '            - g'].join('\n');
		view = mount(doc, doc.length);
		expect(depthOf(lines(view)[6])).toBe('cm-list-depth-5');
	});

	it('task items get the depth and no bullet widget', () => {
		const doc = '- [ ] task\n  - [x] nested task';
		view = mount(doc, doc.length);
		const ls = lines(view);
		expect(ls.map(depthOf)).toEqual(['cm-list-depth-0', 'cm-list-depth-1']);
		expect(view.contentDOM.querySelector('.cm-list-marker')).toBeNull();
		// Without the checkbox extension the `- [ ]` text is still there — only
		// the structural indent in front of the nested one is gone.
		expect(visibleText(view)).toBe('- [ ] task- [x] nested task');
	});

	it('continuation lines get the indent and no widget', () => {
		const doc = '- a\n  continued\n\n- b\n\n  second paragraph\n\nplain';
		view = mount(doc, doc.length);
		const ls = lines(view);
		expect(depthOf(ls[1])).toBe('cm-list-depth-0');
		expect(ls[1].querySelector('.cm-list-marker')).toBeNull();
		// Loose list: the blank lines and the second paragraph are list lines too.
		expect(depthOf(ls[2])).toBe('cm-list-depth-0');
		expect(depthOf(ls[5])).toBe('cm-list-depth-0');
		expect(ls[5].querySelector('.cm-list-marker')).toBeNull();
		// The paragraph after the list is not.
		expect(ls[7].classList.contains('cm-list-line')).toBe(false);
		expect(visibleText(view)).toBe('•acontinued•bsecond paragraphplain');
	});

	it('a continuation line hides only the structural indent', () => {
		const doc = '- a\n      deeper';
		view = mount(doc, doc.length);
		expect(visibleText(view)).toBe('•a    deeper');
	});
});
