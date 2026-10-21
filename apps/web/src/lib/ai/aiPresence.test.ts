// @vitest-environment happy-dom
/**
 * A chat edit's animation (`animateChatEdit`): on a block page open in an
 * editor it flashes the blocks the edit wrote, by id.
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
vi.mock('$lib/components/pages/nodes/FootnoteNode.svelte', () => ({ default: () => {} }));
vi.mock('$lib/components/pages/nodes/FigureNode.svelte', () => ({ default: () => {} }));
vi.mock('svelte-sonner', () => ({ toast: Object.assign(vi.fn(), { error: vi.fn() }) }));

import type { Editor } from '@tiptap/core';
import * as Y from 'yjs';
import { createPageEditor } from '$lib/document/editor';
import { aiPresence, flashedBlocks } from '$lib/document/presence';
import { registerTreeEditor, unregisterTreeEditor } from '$lib/document/registry';
import { contract } from '$lib/document/schema';
import { pageDoc, settle } from '$lib/document/test-utils';
import { animateChatEdit } from './aiPresence';

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
		animateChatEdit('page_1', [lunch, seats]);
		await settle(20);
		expect(flashedBlocks(editor.state).sort()).toEqual([lunch, seats].sort());
		unregisterTreeEditor('page_1', editor);
	});

	it('waits for blocks the sync has not brought yet', async () => {
		const { doc } = pageDoc('<p>First.</p>');
		const editor = treeEditor(doc);
		registerTreeEditor('page_2', editor);
		animateChatEdit('page_2', ['blk_new1']);
		await settle(20);
		expect(flashedBlocks(editor.state)).toEqual([]);
		// The edit arrives from the server, later than a first look.
		await settle(400);
		editor.commands.insertContentAt(editor.state.doc.content.size, '<p data-id="blk_new1">Added.</p>');
		await settle(20);
		expect(flashedBlocks(editor.state)).toEqual(['blk_new1']);
		unregisterTreeEditor('page_2', editor);
	});

	it('flashes what has arrived once the wait runs out, and stops watching', async () => {
		vi.useFakeTimers();
		try {
			const { doc } = pageDoc('<p>First.</p>');
			const editor = treeEditor(doc);
			registerTreeEditor('page_4', editor);
			const [first] = blockIds(editor);
			const off = vi.spyOn(editor, 'off');
			animateChatEdit('page_4', [first, 'blk_never']);
			await vi.advanceTimersByTimeAsync(1000);
			expect(flashedBlocks(editor.state)).toEqual([]);
			await vi.advanceTimersByTimeAsync(1100);
			expect(flashedBlocks(editor.state)).toEqual([first]);
			expect(off).toHaveBeenCalledWith('update', expect.any(Function));
			expect(vi.getTimerCount()).toBeLessThanOrEqual(1);
			unregisterTreeEditor('page_4', editor);
		} finally {
			vi.useRealTimers();
		}
	});

	it('does nothing when the page is not open, or the edit names no blocks', async () => {
		expect(() => animateChatEdit('page_closed', ['blk_1'])).not.toThrow();
		const { doc } = pageDoc('<p>First.</p>');
		const editor = treeEditor(doc);
		registerTreeEditor('page_3', editor);
		animateChatEdit('page_3', []);
		await settle(20);
		expect(flashedBlocks(editor.state)).toEqual([]);
		unregisterTreeEditor('page_3', editor);
	});
});
