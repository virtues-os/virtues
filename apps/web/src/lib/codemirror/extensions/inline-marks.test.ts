// @vitest-environment happy-dom
import { afterEach, describe, expect, it } from 'vitest';

import { createTestView, destroyTestView, forceFocus, visibleText } from '../test-utils';
import { type InlineMark, inlineMarks } from './inline-marks';
import { livePreview } from './live-preview';
import { mouseFreeze } from './mouse-freeze';

import type { EditorView } from '@codemirror/view';

let view: EditorView | undefined;
afterEach(() => {
	if (view) destroyTestView(view);
	view = undefined;
});

/** Every mark in the document, with the open-construct pass on when focused. */
const marksOf = (v: EditorView): InlineMark[] =>
	inlineMarks(v.state, 0, v.state.doc.length, { focused: v.hasFocus });

const byClass = (marks: InlineMark[], cls: string) => marks.filter((m) => m.cls === cls);

const mountLive = (doc: string, selection = 0) =>
	createTestView(doc, { selection, extensions: [mouseFreeze, livePreview] });
// Open marks render through the real live preview; same mount as mountLive,
// kept as a name so the open-construct tests read as what they test.
const mountOpen = mountLive;

describe('==highlight== comes from the parser', () => {
	it('==x== is a highlight with two-character delimiters', () => {
		view = mountLive('==x== after', 8);
		const [mark] = byClass(marksOf(view), 'cm-highlight');
		expect(mark).toEqual({ cls: 'cm-highlight', openFrom: 0, openTo: 2, closeFrom: 3, closeTo: 5 });
		expect(view.contentDOM.querySelector('.cm-highlight')?.textContent).toBe('x');
		// Caret elsewhere: the delimiters are hidden.
		expect(visibleText(view)).toBe('x after');
	});

	it('a == b == c is a sentence, not a highlight', () => {
		view = mountLive('a == b == c', 0);
		expect(byClass(marksOf(view), 'cm-highlight')).toEqual([]);
		expect(visibleText(view)).toBe('a == b == c');
	});

	it('=== is never a delimiter', () => {
		view = mountLive('===x===', 0);
		expect(byClass(marksOf(view), 'cm-highlight')).toEqual([]);
	});

	it('does not see == inside inline code', () => {
		view = mountLive('`a ==x== b`', 0);
		const marks = marksOf(view);
		expect(byClass(marks, 'cm-highlight')).toEqual([]);
		expect(byClass(marks, 'cm-inline-code')).toHaveLength(1);
	});

	it('does not see == inside a fence', () => {
		view = mountLive('```\n==x==\n```\n', 0);
		expect(byClass(marksOf(view), 'cm-highlight')).toEqual([]);
		expect(view.contentDOM.querySelector('.cm-highlight')).toBeNull();
	});

	it('nests inside emphasis like any other construct', () => {
		view = mountLive('**==x==**', 0);
		const marks = marksOf(view);
		expect(byClass(marks, 'cm-strong')).toHaveLength(1);
		expect(byClass(marks, 'cm-highlight')[0]).toMatchObject({ openFrom: 2, openTo: 4, closeFrom: 5, closeTo: 7 });
	});
});

describe('emphasis while typing', () => {
	it('**foo with the caret after foo is bold up to the caret, opener revealed', () => {
		view = mountOpen('**foo', 5);
		const marks = marksOf(view);
		expect(marks).toEqual([
			{ cls: 'cm-strong', openFrom: 0, openTo: 2, closeFrom: 5, closeTo: 5, open: true },
		]);
		expect(view.contentDOM.querySelector('.cm-strong')?.textContent).toBe('foo');
		expect(view.contentDOM.querySelector('.cm-formatting-mark')?.textContent).toBe('**');
		expect(visibleText(view)).toBe('**foo');
	});

	it('holds across the space that would otherwise drop the styling', () => {
		view = mountOpen('**foo bar', 9);
		expect(byClass(marksOf(view), 'cm-strong')[0]).toMatchObject({ openTo: 2, closeFrom: 9, open: true });
	});

	it('is a prediction only while the editor has focus', () => {
		view = mountOpen('**foo', 5);
		forceFocus(view, false);
		expect(marksOf(view)).toEqual([]);
	});

	it('only the caret line carries open marks', () => {
		view = mountOpen('**foo\nplain', 8);
		expect(marksOf(view)).toEqual([]);
	});

	it('leaves a closed **foo** alone', () => {
		view = mountOpen('**foo**', 7);
		expect(marksOf(view)).toEqual([
			{ cls: 'cm-strong', openFrom: 0, openTo: 2, closeFrom: 5, closeTo: 7 },
		]);
	});

	it('a closer after a space still pairs while the caret is inside', () => {
		// CommonMark refuses `**foo **`, so the tree has no node; the pass pairs it.
		view = mountOpen('**foo **', 6);
		expect(marksOf(view)).toEqual([
			{ cls: 'cm-strong', openFrom: 0, openTo: 2, closeFrom: 6, closeTo: 8 },
		]);
	});

	it('a second opener after a closed construct is the open one', () => {
		view = mountOpen('**a** and **b', 13);
		expect(marksOf(view)).toEqual([
			{ cls: 'cm-strong', openFrom: 0, openTo: 2, closeFrom: 3, closeTo: 5 },
			{ cls: 'cm-strong', openFrom: 10, openTo: 12, closeFrom: 13, closeTo: 13, open: true },
		]);
	});

	it('* followed by a space is not an opener', () => {
		view = mountOpen('a * b', 5);
		expect(marksOf(view)).toEqual([]);
	});

	it('_ inside a word is not an opener', () => {
		view = mountOpen('snake_case', 10);
		expect(marksOf(view)).toEqual([]);
	});

	it('an opener with nothing typed after it yet is not styled', () => {
		view = mountOpen('**', 2);
		expect(marksOf(view)).toEqual([]);
	});

	it('an open backtick is inline code to the caret', () => {
		view = mountOpen('see `foo', 8);
		expect(marksOf(view)).toEqual([
			{ cls: 'cm-inline-code', openFrom: 4, openTo: 5, closeFrom: 8, closeTo: 8, open: true },
		]);
	});

	it('an unmatched == is a highlight to the caret', () => {
		view = mountOpen('==foo', 5);
		expect(byClass(marksOf(view), 'cm-highlight')[0]).toMatchObject({ openTo: 2, closeFrom: 5, open: true });
	});

	it('never fires inside a code span', () => {
		view = mountOpen('`**foo`', 5);
		expect(marksOf(view)).toEqual([
			{ cls: 'cm-inline-code', openFrom: 0, openTo: 1, closeFrom: 6, closeTo: 7 },
		]);
	});

	it('never fires inside a fence', () => {
		view = mountOpen('```\n**foo\n```\n', 9);
		expect(marksOf(view)).toEqual([]);
	});

	it('***both open is emphasis around strong', () => {
		view = mountOpen('***both', 7);
		expect(marksOf(view)).toEqual([
			{ cls: 'cm-emphasis', openFrom: 0, openTo: 1, closeFrom: 7, closeTo: 7, open: true },
			{ cls: 'cm-strong', openFrom: 1, openTo: 3, closeFrom: 7, closeTo: 7, open: true },
		]);
	});
});

describe('delimiter length is measured by node type', () => {
	it('***both*** measures the outer * and inner ** from the tree, with no open marks added', () => {
		view = mountOpen('***both***', 10);
		expect(marksOf(view)).toEqual([
			{ cls: 'cm-emphasis', openFrom: 0, openTo: 1, closeFrom: 9, closeTo: 10 },
			{ cls: 'cm-strong', openFrom: 1, openTo: 3, closeFrom: 7, closeTo: 9 },
		]);
	});
});
