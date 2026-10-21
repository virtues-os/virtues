// @vitest-environment happy-dom
/**
 * The assistant on the page (`aiPresence`): its trail keeps the spans it
 * already drew while words stream in, and its caret is placed again when
 * the page reflows.
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

import type { Editor } from '@tiptap/core';
import { aiPresence, setAiCaret, setAiTrail } from './presence';
import { htmlEditor } from './test-utils';

const editors: Editor[] = [];

afterEach(() => {
	for (const e of editors.splice(0)) if (!e.isDestroyed) e.destroy();
	document.body.replaceChildren();
	vi.unstubAllGlobals();
	vi.restoreAllMocks();
});

function open(html: string): Editor {
	const editor = htmlEditor(html, [aiPresence()]);
	editors.push(editor);
	return editor;
}

/** Stream `chunk` in at `pos`, as a session does: the words, then the trail over them. */
function stream(editor: Editor, pos: number, chunk: string): number {
	editor.view.dispatch(editor.state.tr.insertText(chunk, pos));
	setAiTrail(editor, { from: pos, to: pos + chunk.length });
	return pos + chunk.length;
}

describe("the assistant's trail", () => {
	it('keeps the spans it drew while later chunks stream in, so their fade does not start over', () => {
		const editor = open('<p>Plan: </p>');
		let pos = editor.state.doc.child(0).nodeSize - 1;
		pos = stream(editor, pos, 'pack ');
		const [first] = editor.view.dom.querySelectorAll('.doc-ai-trail');
		expect(first?.textContent).toBe('pack ');
		pos = stream(editor, pos, 'a jacket ');
		stream(editor, pos, 'and boots.');
		const spans = [...editor.view.dom.querySelectorAll('.doc-ai-trail')];
		expect(spans.map((s) => s.textContent)).toEqual(['pack ', 'a jacket ', 'and boots.']);
		expect(spans[0]).toBe(first);
	});
});

describe("the assistant's caret", () => {
	it('is placed again when the page resizes, and stops watching once hidden', () => {
		const observers: { cb: ResizeObserverCallback; watching: Element[] }[] = [];
		vi.stubGlobal(
			'ResizeObserver',
			class {
				private entry: { cb: ResizeObserverCallback; watching: Element[] };
				constructor(cb: ResizeObserverCallback) {
					this.entry = { cb, watching: [] };
					observers.push(this.entry);
				}
				observe(el: Element) {
					this.entry.watching.push(el);
				}
				disconnect() {
					this.entry.watching = [];
				}
			},
		);
		vi.spyOn(window, 'requestAnimationFrame').mockImplementation((cb) => {
			cb(0);
			return 1;
		});
		const editor = open('<p>Hello</p>');
		const host = editor.view.dom.parentElement!;
		setAiCaret(editor, { pos: 6, phase: 'active' });
		const caret = host.querySelector<HTMLElement>('.doc-ai-caret')!;
		expect(caret).not.toBeNull();
		expect(observers.at(-1)?.watching).toEqual([editor.view.dom]);

		// An image above loads: the text moves down without a transaction.
		vi.spyOn(editor.view, 'coordsAtPos').mockReturnValue({ left: 10, right: 10, top: 120, bottom: 140 });
		observers.at(-1)!.cb([], {} as ResizeObserver);
		expect(caret.style.transform).toContain('120px');

		vi.spyOn(editor.view, 'coordsAtPos').mockReturnValue({ left: 10, right: 10, top: 200, bottom: 220 });
		window.dispatchEvent(new Event('resize'));
		expect(caret.style.transform).toContain('200px');

		setAiCaret(editor, null);
		expect(host.querySelector('.doc-ai-caret')).toBeNull();
		expect(observers.at(-1)?.watching).toEqual([]);
	});
});
