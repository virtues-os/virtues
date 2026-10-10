// @vitest-environment happy-dom
/**
 * A chat edit's animation (`animateChatEdit`): on a block page open in an
 * editor it flashes the blocks the edit wrote, by id; on a markdown page it
 * trails the text in CodeMirror, as it always has.
 */

import { afterEach, describe, expect, it, vi } from 'vitest';

vi.hoisted(() => {
	(globalThis as Record<string, unknown>).$state = <T>(v: T) => v;
});

vi.mock('$lib/components/pages/nodes/MentionNode.svelte', () => ({ default: () => {} }));
vi.mock('$lib/components/pages/nodes/MediaNode.svelte', () => ({ default: () => {} }));
vi.mock('$lib/components/pages/nodes/AppletNode.svelte', () => ({ default: () => {} }));
vi.mock('$lib/components/pages/nodes/CalloutNode.svelte', () => ({ default: () => {} }));
vi.mock('$lib/components/pages/nodes/CodeBlockNode.svelte', () => ({ default: () => {} }));
vi.mock('svelte-sonner', () => ({ toast: Object.assign(vi.fn(), { error: vi.fn() }) }));

import type { Editor } from '@tiptap/core';
import type { EditorView } from '@codemirror/view';
import * as Y from 'yjs';
import { createPageEditor } from '$lib/document/editor';
import { aiPresence, flashedBlocks } from '$lib/document/presence';
import { registerTreeEditor, unregisterTreeEditor } from '$lib/document/registry';
import { contract } from '$lib/document/schema';
import { pageDoc, settle } from '$lib/document/test-utils';
import { animateChatEdit, registerPageEditor, unregisterPageEditor } from './aiPresence';

const editors: Editor[] = [];

afterEach(() => {
	for (const e of editors.splice(0)) if (!e.isDestroyed) e.destroy();
	document.body.replaceChildren();
});

function treeEditor(doc: Y.Doc): Editor {
	const element = document.createElement('div');
	document.body.append(element);
	const editor = createPageEditor({
		element,
		doc: { ydoc: doc, provider: null },
		editable: true,
		placeholder: 'Write',
		extras: {},
		plugins: [aiPresence()],
	});
	editors.push(editor);
	return editor;
}

function blockIds(editor: Editor): string[] {
	const out: string[] = [];
	editor.state.doc.forEach((n) => out.push(String(n.attrs[contract.id.attr])));
	return out;
}

describe('a chat edit on a block page', () => {
	it('flashes the blocks it wrote in the editor open on the page', async () => {
		const { doc } = pageDoc('<h2>Plan</h2><p>Lunch with Nick.</p><p>Book seats.</p>');
		const editor = treeEditor(doc);
		registerTreeEditor('page_1', editor);
		const [, lunch, seats] = blockIds(editor);
		animateChatEdit('page_1', 'Lunch with [@Nick](/person/person_1).', [lunch, seats]);
		await settle(20);
		expect(flashedBlocks(editor.state).sort()).toEqual([lunch, seats].sort());
		unregisterTreeEditor('page_1', editor);
	});

	it('waits once for blocks the sync has not brought yet', async () => {
		const { doc } = pageDoc('<p>First.</p>');
		const editor = treeEditor(doc);
		registerTreeEditor('page_2', editor);
		animateChatEdit('page_2', 'Added.', ['blk_new1']);
		await settle(20);
		expect(flashedBlocks(editor.state)).toEqual([]);
		// The edit arrives from the server.
		editor.commands.insertContentAt(editor.state.doc.content.size, '<p data-id="blk_new1">Added.</p>');
		await settle(200);
		expect(flashedBlocks(editor.state)).toEqual(['blk_new1']);
		unregisterTreeEditor('page_2', editor);
	});

	it('does not reach a CodeMirror editor for the same page', async () => {
		const { doc } = pageDoc('<p>First.</p>');
		const editor = treeEditor(doc);
		const view = fakeView('First.');
		registerTreeEditor('page_3', editor);
		registerPageEditor('page_3', view);
		animateChatEdit('page_3', 'First.', []);
		await settle(20);
		expect(view.dispatch).not.toHaveBeenCalled();
		unregisterTreeEditor('page_3', editor);
		unregisterPageEditor('page_3', view);
	});
});

/** A CodeMirror view as far as the animation uses it: its text and dispatch. */
function fakeView(text: string) {
	return {
		state: { doc: { toString: () => text } },
		dispatch: vi.fn(),
	} as unknown as EditorView & { dispatch: ReturnType<typeof vi.fn> };
}

describe('a chat edit on a markdown page', () => {
	it('trails the written text in CodeMirror, as before', async () => {
		const view = fakeView('# Trip\n\nLunch with Nick on Friday.\n');
		registerPageEditor('page_md', view);
		animateChatEdit('page_md', 'Lunch with Nick on Friday.');
		await settle(20);
		// A trail, then the caret, over the text it found.
		expect(view.dispatch).toHaveBeenCalledTimes(2);
		unregisterPageEditor('page_md', view);
	});

	it('does nothing when the page is not open', () => {
		expect(() => animateChatEdit('page_closed', 'Anything', ['blk_1'])).not.toThrow();
	});
});
