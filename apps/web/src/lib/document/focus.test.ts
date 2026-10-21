// @vitest-environment happy-dom
/**
 * Focus and typewriter mode (`focusMode`): which blocks are dimmed, when the
 * dimmed set is built again, and which caret moves scroll the page.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

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
import { TextSelection, type EditorState } from '@tiptap/pm/state';
import type { DecorationSet } from '@tiptap/pm/view';
import { focusMode } from './focus';
import { htmlEditor, posIn } from './test-utils';

const editors: Editor[] = [];
let scrolls: ReturnType<typeof vi.fn>;

beforeEach(() => {
	vi.spyOn(window, 'requestAnimationFrame').mockImplementation((cb) => {
		cb(0);
		return 0;
	});
	scrolls = vi.fn();
	(document.scrollingElement as HTMLElement).scrollBy = scrolls as never;
});

afterEach(() => {
	for (const e of editors.splice(0)) if (!e.isDestroyed) e.destroy();
	document.body.replaceChildren();
	vi.restoreAllMocks();
});

function open(html: string): Editor {
	const editor = htmlEditor(html, [focusMode(true)]);
	editors.push(editor);
	editor.view.focus();
	vi.spyOn(editor.view, 'hasFocus').mockReturnValue(true);
	return editor;
}

/** The decorations focus mode gives the editor now. */
function dimSet(editor: Editor): DecorationSet | null {
	const plugin = editor.state.plugins.find((p) => (p as unknown as { key: string }).key.startsWith('focusMode$'));
	const decorations = plugin?.props.decorations as ((s: EditorState) => DecorationSet | null) | undefined;
	return decorations?.call(plugin, editor.state) ?? null;
}

function caretTo(editor: Editor, pos: number, pointer = false): void {
	const tr = editor.state.tr.setSelection(TextSelection.create(editor.state.doc, pos));
	if (pointer) tr.setMeta('pointer', true);
	editor.view.dispatch(tr);
}

const PAGE = '<p>Pack a jacket.</p><p>Call Nick about Friday.</p><p>Book seats.</p>';

describe('focus mode', () => {
	it('dims every top-level block but the caret\'s', () => {
		const editor = open(PAGE);
		caretTo(editor, posIn(editor, 'Nick'));
		expect([...editor.view.dom.querySelectorAll('.doc-dim')].map((el) => el.textContent)).toEqual([
			'Pack a jacket.',
			'Book seats.',
		]);
	});

	it('keeps the dimmed set while the caret stays in its block, and builds it again when it leaves', () => {
		const editor = open(PAGE);
		caretTo(editor, posIn(editor, 'Nick'));
		const first = dimSet(editor);
		caretTo(editor, posIn(editor, 'Friday'));
		expect(dimSet(editor)).toBe(first);
		caretTo(editor, posIn(editor, 'Book'));
		expect(dimSet(editor)).not.toBe(first);
		expect([...editor.view.dom.querySelectorAll('.doc-dim')].map((el) => el.textContent)).toEqual([
			'Pack a jacket.',
			'Call Nick about Friday.',
		]);
	});

	it('dims a block another edit adds', () => {
		const editor = open(PAGE);
		caretTo(editor, posIn(editor, 'Nick'));
		editor.commands.insertContentAt(editor.state.doc.content.size, '<p>Pay the deposit.</p>');
		expect(editor.view.dom.querySelectorAll('.doc-dim')).toHaveLength(3);
	});
});

describe('typewriter mode', () => {
	it('scrolls for a keyboard move and for typed text', () => {
		const editor = open(PAGE);
		caretTo(editor, posIn(editor, 'Book'));
		expect(scrolls).toHaveBeenCalledTimes(1);
		editor.view.dispatch(editor.state.tr.insertText('x'));
		expect(scrolls).toHaveBeenCalledTimes(2);
	});

	it('never scrolls for a click', () => {
		const editor = open(PAGE);
		caretTo(editor, posIn(editor, 'Book'), true);
		expect(scrolls).not.toHaveBeenCalled();
	});

	it('never scrolls for a selection made while the pointer is down, or just after it comes up', () => {
		const editor = open(PAGE);
		editor.view.dom.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
		caretTo(editor, posIn(editor, 'Book'));
		window.dispatchEvent(new MouseEvent('mouseup', { bubbles: true }));
		caretTo(editor, posIn(editor, 'seats'));
		expect(scrolls).not.toHaveBeenCalled();
	});
});
