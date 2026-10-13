// @vitest-environment happy-dom
/**
 * Version skew: a client whose contract predates a node type, opening a
 * document that holds one. Tiptap's binding cannot build the node, and its
 * error path deletes the element from the SHARED document, for every client.
 *
 * This pins the hazard the contract version guard exists for. The page socket
 * (`virtues-core/src/server/yjs.rs`) takes the client's version as
 * `?contract=N`, refuses to bind a client below the document's, closes one
 * bound below a version the server raises, and refuses an update that changes
 * the version (`check_update` in `crates/virtues-document`). If this test
 * starts failing, the binding has changed how it treats unknown nodes;
 * re-measure before relaxing the guard.
 */

import { Editor } from '@tiptap/core';
import Collaboration from '@tiptap/extension-collaboration';
import { describe, expect, it } from 'vitest';
import * as Y from 'yjs';
import { contract, contractExtensions } from './schema';

const FUTURE_TYPE = 'pollWidget';

function paragraph(text: string): Y.XmlElement {
	const p = new Y.XmlElement('paragraph');
	const t = new Y.XmlText();
	t.insert(0, text);
	p.insert(0, [t]);
	return p;
}

function names(fragment: Y.XmlFragment): string[] {
	return fragment.toArray().map((n) => (n as Y.XmlElement).nodeName);
}

describe('contract version skew', () => {
	it('an old client binding the document deletes what it cannot parse', () => {
		expect(contract.nodes[FUTURE_TYPE], `${FUTURE_TYPE} must stay outside the contract`).toBeUndefined();

		const ydoc = new Y.Doc();
		const fragment = ydoc.getXmlFragment(contract.fragment);
		const future = new Y.XmlElement(FUTURE_TYPE);
		future.setAttribute('question', 'Lunch?');
		fragment.insert(0, [paragraph('Before the widget.'), future, paragraph('After the widget.')]);
		expect(names(fragment)).toEqual(['paragraph', FUTURE_TYPE, 'paragraph']);

		const editor = new Editor({
			element: document.createElement('div'),
			extensions: [
				...contractExtensions(),
				Collaboration.configure({ document: ydoc, field: contract.fragment }),
			],
		});
		const after = names(fragment);
		editor.destroy();

		// The shared document lost the widget, not just this client's view.
		expect(after).toEqual(['paragraph', 'paragraph']);
	});
});
