// @vitest-environment happy-dom
import { afterEach, describe, expect, it } from 'vitest';

import { createTestView, destroyTestView } from '../test-utils';
import { listRenumber } from './list-renumber';

import type { EditorView } from '@codemirror/view';

let view: EditorView | undefined;
afterEach(() => {
	if (view) destroyTestView(view);
	view = undefined;
});

const mount = (doc: string) => createTestView(doc, { extensions: [listRenumber] });

describe('ordered-list renumbering rides in the same transaction', () => {
	it('renumbers a list that sits BELOW the edit, at the right positions', () => {
		// The renumber positions are in the new document; an insertion above
		// the list shifts them. Resolved against the old document they would
		// land four characters late and corrupt the item text.
		view = mount('top\n\n1. a\n1. b\n1. c');
		view.dispatch({ changes: { from: 3, insert: ' end' }, userEvent: 'input.type' });
		expect(view.state.doc.toString()).toBe('top end\n\n1. a\n2. b\n3. c');
	});

	it('survives a paste longer than the document it replaces', () => {
		view = mount('short');
		const pasted = 'intro paragraph that is longer than the old document\n\n1. one\n1. two\n10. three';
		view.dispatch({
			changes: { from: 0, to: 5, insert: pasted },
			userEvent: 'input.paste',
		});
		expect(view.state.doc.toString()).toBe(
			'intro paragraph that is longer than the old document\n\n1. one\n2. two\n3. three'
		);
	});

	it('ignores remote and programmatic changes', () => {
		view = mount('1. a\n1. b');
		view.dispatch({ changes: { from: 0, insert: 'x' } });
		expect(view.state.doc.toString()).toBe('x1. a\n1. b');
	});
});
