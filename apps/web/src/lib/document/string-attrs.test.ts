// @vitest-environment happy-dom
/**
 * Yjs XML attributes carry their JSON type, and the binding hands them to
 * ProseMirror as they are. yrs 0.18 can only write strings: a heading
 * it writes at level "2" reaches the editor as the string, matches none of
 * the contract's render rules (they set the number 2), and the editor throws
 * when it draws the page. The server must write typed attributes; this pins
 * both halves of why.
 */

import { DOMSerializer, type Node as PmNode } from '@tiptap/pm/model';
import { yXmlFragmentToProseMirrorRootNode } from '@tiptap/y-tiptap';
import { describe, expect, it } from 'vitest';
import * as Y from 'yjs';
import { contract, contractSchema } from './schema';

const schema = contractSchema();

function headingAt(level: unknown): PmNode {
	const ydoc = new Y.Doc();
	const fragment = ydoc.getXmlFragment(contract.fragment);
	const heading = new Y.XmlElement('heading');
	heading.setAttribute('level', level as string);
	const text = new Y.XmlText();
	text.insert(0, 'Title');
	heading.insert(0, [text]);
	fragment.insert(0, [heading]);
	return yXmlFragmentToProseMirrorRootNode(fragment, schema);
}

function render(doc: PmNode): string {
	const div = document.createElement('div');
	div.appendChild(DOMSerializer.fromSchema(schema).serializeFragment(doc.content));
	return div.innerHTML;
}

describe('Yjs attribute types', () => {
	it('a string attribute passes through the binding and has no render rule', () => {
		const doc = headingAt('2');
		expect(doc.firstChild!.attrs.level).toBe('2');
		expect(() => render(doc)).toThrow(/no render rule/);
	});

	it('a typed attribute renders', () => {
		const doc = headingAt(2);
		expect(doc.firstChild!.attrs.level).toBe(2);
		expect(render(doc)).toBe('<h2>Title</h2>');
	});
});
