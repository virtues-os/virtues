// @vitest-environment happy-dom
/**
 * The inline writer on a block page (`treeAiSession.ts`), on two devices
 * synced through their Yjs updates: what it writes, where, and what it
 * leaves when it is stopped.
 *
 * The node views' Svelte components cannot be compiled here (no Svelte
 * plugin in vitest.config.ts); these editors draw none (`extras: {}`), and
 * the modules that import the components are given stand-ins.
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
vi.mock('./aiSession.svelte', () => ({ aiSession: { set: vi.fn(), reset: vi.fn(), active: false } }));
vi.mock('./aiCursorSession', () => ({ abortAiSession: vi.fn() }));
vi.mock('svelte-sonner', () => ({ toast: Object.assign(vi.fn(), { error: vi.fn() }) }));

import type { Editor } from '@tiptap/core';
import { yUndoPluginKey } from '@tiptap/y-tiptap';
import * as Y from 'yjs';
import { codeAt, createPageEditor } from '$lib/document/editor';
import { aiPresence, aiTelegraphOf } from '$lib/document/presence';
import { aiSession } from './aiSession.svelte';
import { contract } from '$lib/document/schema';
import { acceptProposal, proposalIds, rejectProposal } from '$lib/document/suggestions';
import { pageDoc, posIn, settle } from '$lib/document/test-utils';
import type { AiCompleteRequest } from './inlineComplete';
import { createTreeAiDriver } from './treeAiSession';

const editors: Editor[] = [];

afterEach(() => {
	for (const e of editors.splice(0)) if (!e.isDestroyed) e.destroy();
	document.body.replaceChildren();
});

function editorOn(doc: Y.Doc): Editor {
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

/** A page on two devices whose updates reach each other at once. */
function twoDevices(html: string) {
	const a = pageDoc(html).doc;
	const b = new Y.Doc();
	Y.applyUpdate(b, Y.encodeStateAsUpdate(a));
	a.on('update', (u: Uint8Array, origin: unknown) => {
		if (origin !== 'wire') Y.applyUpdate(b, u, 'wire');
	});
	b.on('update', (u: Uint8Array, origin: unknown) => {
		if (origin !== 'wire') Y.applyUpdate(a, u, 'wire');
	});
	return { here: editorOn(a), there: editorOn(b) };
}

/** A completion the test feeds by hand; aborting the signal ends it as fetch does. */
function feed() {
	const queue: (string | null)[] = [];
	let wake: (() => void) | null = null;
	const requests: AiCompleteRequest[] = [];
	async function* stream(req: AiCompleteRequest, signal: AbortSignal): AsyncGenerator<string> {
		requests.push(req);
		while (true) {
			while (!queue.length) {
				await new Promise<void>((resolve, reject) => {
					wake = resolve;
					signal.addEventListener('abort', () => reject(new DOMException('aborted', 'AbortError')), { once: true });
				});
			}
			const next = queue.shift();
			if (next == null) return;
			yield next;
		}
	}
	return {
		stream,
		requests,
		push(text: string) {
			queue.push(text);
			wake?.();
		},
		end() {
			queue.push(null);
			wake?.();
		},
	};
}

function driverWith(f: ReturnType<typeof feed>, convert = vi.fn(async (_md: string) => ({ html: '' }))) {
	return { driver: createTreeAiDriver({ stream: f.stream, convert, flushMs: 0, dissolveMs: 0, telegraphMs: 0 }), convert };
}

/** Each top-level block: its type and its text. */
function blocks(editor: Editor): string[] {
	const out: string[] = [];
	editor.state.doc.forEach((node) => out.push(`${node.type.name}: ${node.textContent}`));
	return out;
}

/** Each text run carrying a proposal: its mark, its id and its text. */
function proposals(editor: Editor): { mark: string; id: string; text: string }[] {
	const out: { mark: string; id: string; text: string }[] = [];
	editor.state.doc.descendants((node) => {
		if (!node.isText) return true;
		for (const m of node.marks) {
			if (m.type.name === 'proposedDeletion' || m.type.name === 'proposedInsertion') {
				out.push({ mark: m.type.name, id: String(m.attrs.proposal), text: node.text ?? '' });
			}
		}
		return false;
	});
	return out;
}

function idOf(editor: Editor, text: string): string {
	let id = '';
	editor.state.doc.descendants((node) => {
		if (!id && node.isTextblock && node.textContent.includes(text)) id = String(node.attrs[contract.id.attr]);
		return !id;
	});
	return id;
}

describe('a continuation', () => {
	it('lands at its anchor after another device adds a paragraph above it', async () => {
		const { here, there } = twoDevices('<p>First line</p><p>Second line</p>');
		const f = feed();
		const { driver } = driverWith(f);
		here.commands.setTextSelection(posIn(here, 'First line', 'First line'.length));

		driver.start({ editor: here, intent: 'continue', instruction: 'Go on', pageTitle: 'Trip' });
		await settle();
		expect(driver.isActive()).toBe(true);
		f.push(' and then a few more words');
		await settle();
		expect(blocks(there)).toEqual(['paragraph: First line and then a few more words', 'paragraph: Second line']);

		there.commands.insertContentAt(0, '<p>Above</p>');
		await settle();
		f.push(' at the end.');
		f.end();
		await driver.whenIdle();

		const want = ['paragraph: Above', 'paragraph: First line and then a few more words at the end.', 'paragraph: Second line'];
		expect(blocks(here)).toEqual(want);
		expect(blocks(there)).toEqual(want);
		expect(driver.isActive()).toBe(false);
		// The model read the page around the caret, mentions as their labels.
		expect(f.requests[0]).toMatchObject({ intent: 'continue', context_before: 'First line', page_title: 'Trip' });
		expect(f.requests[0].context_after).toBe('\n\nSecond line');
	});

	it('starts a new block at a line break, and leaves none empty after the last one', async () => {
		const { here, there } = twoDevices('<h2>Plan</h2><p>After.</p>');
		const f = feed();
		const { driver } = driverWith(f);
		here.commands.setTextSelection(posIn(here, 'Plan', 4));
		driver.start({ editor: here, intent: 'continue', instruction: 'Go on' });
		f.push('\n\nThe first paragraph of it.\n\nAnd a second one.\n');
		f.end();
		await driver.whenIdle();
		const want = ['heading: Plan', 'paragraph: The first paragraph of it.', 'paragraph: And a second one.', 'paragraph: After.'];
		expect(blocks(here)).toEqual(want);
		expect(blocks(there)).toEqual(want);
		// Every block it made has an id of its own.
		const ids = new Set<string>();
		here.state.doc.forEach((n) => ids.add(String(n.attrs[contract.id.attr])));
		expect(ids.size).toBe(4);
	});

	it('is one undo step, however many chunks it came in', async () => {
		const { here } = twoDevices('<p>Start.</p>');
		const manager = (yUndoPluginKey.getState(here.state) as { undoManager: Y.UndoManager }).undoManager;
		// Without the session's hold, every chunk would be a step of its own.
		manager.captureTimeout = 0;
		const f = feed();
		const { driver } = driverWith(f);
		here.commands.setTextSelection(posIn(here, 'Start.', 6));
		driver.start({ editor: here, intent: 'continue', instruction: 'Go on' });
		f.push(' One chunk of words here,');
		await settle(30);
		f.push(' and then another chunk.');
		await settle(30);
		f.end();
		await driver.whenIdle();
		expect(blocks(here)).toEqual(['paragraph: Start. One chunk of words here, and then another chunk.']);
		expect(manager.captureTimeout).toBe(0);
		here.commands.undo();
		expect(blocks(here)).toEqual(['paragraph: Start.']);
	});
});

describe('stopping', () => {
	it('closes the undo step at once, so the keystroke that stopped it is its own step', async () => {
		const { here } = twoDevices('<p>Start.</p>');
		const manager = (yUndoPluginKey.getState(here.state) as { undoManager: Y.UndoManager }).undoManager;
		const f = feed();
		const { driver } = driverWith(f);
		here.commands.setTextSelection(posIn(here, 'Start.', 6));
		driver.start({ editor: here, intent: 'continue', instruction: 'Go on' });
		f.push(' Words from the writer here,');
		await settle();
		driver.abortIn(here);
		expect(manager.captureTimeout).not.toBe(Number.POSITIVE_INFINITY);
		// The person types at once, before the stream has wound down.
		here.commands.insertContentAt(1, 'X');
		await driver.whenIdle();
		expect(blocks(here)).toEqual(['paragraph: XStart. Words from the writer here,']);
		here.commands.undo();
		expect(blocks(here)).toEqual(['paragraph: Start. Words from the writer here,']);
	});
});

describe('a rewrite', () => {
	it('proposes: the selection marked for deletion and the rewrite as an insertion, one id between them', async () => {
		const { here, there } = twoDevices('<p>The quick brown fox.</p>');
		const f = feed();
		const { driver } = driverWith(f);
		const from = posIn(here, 'quick brown');
		here.commands.setTextSelection({ from, to: from + 'quick brown'.length });
		driver.start({ editor: here, intent: 'rewrite', instruction: 'Make it slower' });
		f.push('slow red');
		f.end();
		await driver.whenIdle();

		const marked = proposals(here);
		expect(marked).toEqual([
			{ mark: 'proposedDeletion', id: marked[0].id, text: 'quick brown' },
			{ mark: 'proposedInsertion', id: marked[0].id, text: 'slow red' },
		]);
		expect(proposals(there)).toEqual(marked);
		expect(f.requests[0]).toMatchObject({ intent: 'rewrite', selection: 'quick brown' });

		acceptProposal(here, marked[0].id);
		expect(blocks(there)).toEqual(['paragraph: The slow red fox.']);
	});

	it('washes the words it will replace while the model thinks, until its first words mark them', async () => {
		const { here } = twoDevices('<p>The quick brown fox.</p>');
		const f = feed();
		const { driver } = driverWith(f);
		const from = posIn(here, 'quick brown');
		const to = from + 'quick brown'.length;
		here.commands.setTextSelection({ from, to });
		driver.start({ editor: here, intent: 'rewrite', instruction: 'Make it slower' });
		await settle();
		// The selection is collapsed, so the format bar stays off; the words still show.
		expect(here.state.selection.empty).toBe(true);
		expect(aiTelegraphOf(here.state)).toEqual({ from, to });
		expect(here.view.dom.querySelector('.doc-ai-telegraph')?.textContent).toBe('quick brown');
		expect(vi.mocked(aiSession.set)).toHaveBeenCalledWith('telegraphing');
		f.push('slow red, and then the rest');
		await settle();
		expect(aiTelegraphOf(here.state)).toBeNull();
		expect(proposals(here)[0]).toMatchObject({ mark: 'proposedDeletion', text: 'quick brown' });
		f.end();
		await driver.whenIdle();

		// Stopped before any words arrive: the wash goes and the words stay as they were.
		const other = twoDevices('<p>Lunch at noon.</p>').here;
		const g = feed();
		const second = driverWith(g).driver;
		const at = posIn(other, 'at noon');
		other.commands.setTextSelection({ from: at, to: at + 'at noon'.length });
		second.start({ editor: other, intent: 'rewrite', instruction: 'Make it later' });
		await settle();
		expect(aiTelegraphOf(other.state)).not.toBeNull();
		second.abortIn(other);
		await second.whenIdle();
		expect(aiTelegraphOf(other.state)).toBeNull();
		expect(blocks(other)).toEqual(['paragraph: Lunch at noon.']);
		expect(proposals(other)).toEqual([]);
	});

	it('stopped mid-stream, leaves a whole proposal of what arrived, which reject takes back', async () => {
		const { here, there } = twoDevices('<p>Lunch on Friday at noon.</p>');
		const f = feed();
		const { driver } = driverWith(f);
		const from = posIn(here, 'on Friday');
		here.commands.setTextSelection({ from, to: from + 'on Friday'.length });
		driver.start({ editor: here, intent: 'rewrite', instruction: 'Make it Saturday' });
		f.push('on Saturday, if David Okafor');
		await settle();
		driver.abortIn(here);
		await driver.whenIdle();

		expect(() => here.state.doc.check()).not.toThrow();
		const marked = proposals(here);
		expect(marked.map((m) => m.mark)).toEqual(['proposedDeletion', 'proposedInsertion']);
		expect(new Set(marked.map((m) => m.id)).size).toBe(1);
		expect(marked[0].text).toBe('on Friday');
		expect(marked[1].text.startsWith('on Saturday')).toBe(true);
		expect(proposalIds(there.state)).toEqual([marked[0].id]);

		rejectProposal(here, marked[0].id);
		expect(blocks(there)).toEqual(['paragraph: Lunch on Friday at noon.']);
	});

	it('stopped before any words arrive, changes nothing', async () => {
		const { here } = twoDevices('<p>Lunch on Friday at noon.</p>');
		const before = here.state.doc.toJSON();
		const f = feed();
		const { driver } = driverWith(f);
		const from = posIn(here, 'on Friday');
		here.commands.setTextSelection({ from, to: from + 'on Friday'.length });
		driver.start({ editor: here, intent: 'rewrite', instruction: 'Make it Saturday' });
		await settle();
		driver.abortIn(here);
		await driver.whenIdle();
		expect(here.state.doc.toJSON()).toEqual(before);
	});

	it('stops from the status bar, which names no editor', async () => {
		const { here } = twoDevices('<p>Lunch on Friday at noon.</p>');
		const f = feed();
		const { driver } = driverWith(f);
		here.commands.setTextSelection(posIn(here, 'at noon.', 'at noon.'.length));
		driver.start({ editor: here, intent: 'continue', instruction: '' });
		f.push(' Bring the notes.');
		await settle();
		expect(driver.isActive()).toBe(true);
		driver.abort();
		await driver.whenIdle();
		expect(driver.isActive()).toBe(false);
		expect(blocks(here)).toEqual(['paragraph: Lunch on Friday at noon. Bring the notes.']);
	});

	it('marks only text: a mention in the selection carries no mark', async () => {
		const { here } = twoDevices(
			'<p>Lunch with <virtues-mention to="/person/person_1" label="Nick"></virtues-mention> on Friday.</p>',
		);
		const f = feed();
		const { driver } = driverWith(f);
		// The whole paragraph: a mention counts one position, which text offsets miss.
		const para = here.state.doc.firstChild!;
		here.commands.setTextSelection({ from: 1, to: para.nodeSize - 1 });
		driver.start({ editor: here, intent: 'rewrite', instruction: 'Shorter' });
		f.push('Lunch with Nick, Friday.');
		f.end();
		await driver.whenIdle();
		let mentionMarks = -1;
		here.state.doc.descendants((node) => {
			if (node.type.name === 'mention') mentionMarks = node.marks.length;
			return true;
		});
		expect(mentionMarks).toBe(0);
		expect(proposals(here).map((m) => m.mark)).toEqual(['proposedDeletion', 'proposedDeletion', 'proposedInsertion']);
	});

	// A line break in a rewrite must not split the block the selection is in:
	// rejecting it takes out only what the rewrite marked, and a split it made
	// would stay.
	for (const [page, word, rewrite, accepted] of [
		[
			'<p>Hello world. Goodbye.</p>',
			'Hello world.',
			'Hi.\n\nBye.',
			['paragraph: Hi.', 'paragraph: Bye. Goodbye.'],
		],
		[
			'<h2>Big plan for today</h2>',
			'Big plan',
			'Small\n\nplan',
			['heading: Small', 'paragraph: plan for today'],
		],
		[
			'<p>Hello world. Goodbye.</p>',
			'Hello world.',
			'\nHi.\nThere.\n\nBye.',
			['paragraph: ', 'paragraph: Hi.', 'paragraph: There.', 'paragraph: Bye. Goodbye.'],
		],
	] as const) {
		it(`over lines (${JSON.stringify(rewrite)}), rejected, leaves "${page}" as it was, and accepted, reads as written`, async () => {
			const original = blocks(twoDevices(page).here);
			for (const accept of [false, true]) {
				const { here, there } = twoDevices(page);
				const f = feed();
				const { driver } = driverWith(f);
				const from = posIn(here, word);
				here.commands.setTextSelection({ from, to: from + word.length });
				driver.start({ editor: here, intent: 'rewrite', instruction: 'Split it' });
				f.push(rewrite);
				f.end();
				await driver.whenIdle();
				expect(() => here.state.doc.check()).not.toThrow();
				const [id] = proposalIds(here.state);
				if (accept) acceptProposal(here, id);
				else rejectProposal(here, id);
				expect(blocks(there)).toEqual(accept ? [...accepted].filter((b) => b !== 'paragraph: ') : original);
				expect(proposalIds(there.state)).toEqual([]);
			}
		});
	}

	it('over lines, a tail no mark can hold stays put, and the break becomes a space', async () => {
		const { here } = twoDevices(
			'<p>Lunch on Friday with <virtues-mention to="/person/person_1" label="Nick"></virtues-mention>.</p>',
		);
		const original = here.state.doc.toJSON();
		const f = feed();
		const { driver } = driverWith(f);
		const from = posIn(here, 'on Friday');
		here.commands.setTextSelection({ from, to: from + 'on Friday'.length });
		driver.start({ editor: here, intent: 'rewrite', instruction: 'Two lines' });
		f.push('on Saturday\n\nat noon');
		f.end();
		await driver.whenIdle();
		expect(blocks(here)).toEqual(['paragraph: Lunch on Fridayon Saturday at noon with .']);
		rejectProposal(here, proposalIds(here.state)[0]);
		expect(here.state.doc.toJSON()).toEqual(original);
	});

	it('in a code block, replaces the selection outright', async () => {
		const { here, there } = twoDevices('<pre data-language="rust"><code>let x = 1;</code></pre>');
		const f = feed();
		const { driver } = driverWith(f);
		const from = posIn(here, 'let x = 1;', 'let x = '.length);
		here.commands.setTextSelection({ from, to: from + 1 });
		driver.start({ editor: here, intent: 'rewrite', instruction: 'Make it 42' });
		f.push('42');
		f.end();
		await driver.whenIdle();
		expect(blocks(there)).toEqual(['codeBlock: let x = 42;']);
		expect(proposals(here)).toEqual([]);
	});

	// Across a code block's edge the rewrite would land in the code block,
	// where nothing can be proposed, and take the prose out with nothing to
	// reject: it writes nothing.
	it('across the edge of a code block, writes nothing', async () => {
		const { here } = twoDevices('<p>Run this</p><pre data-language="bash"><code>ls -la</code></pre>');
		const original = here.state.doc.toJSON();
		const f = feed();
		const { driver } = driverWith(f);
		here.commands.setTextSelection({ from: posIn(here, 'Run this', 'Run '.length), to: posIn(here, 'ls -la', 'ls'.length) });
		driver.start({ editor: here, intent: 'rewrite', instruction: 'Shorter' });
		f.push('NEW');
		f.end();
		await driver.whenIdle();
		expect(here.state.doc.toJSON()).toEqual(original);
		expect(codeAt(here.state)).toBe('across');
	});
});

describe('the final conversion', () => {
	it('replaces only what the session wrote with the blocks its markdown makes', async () => {
		const { here, there } = twoDevices('<p>Intro text.</p><p>After.</p>');
		const before = { intro: idOf(here, 'Intro text.'), after: idOf(here, 'After.') };
		const f = feed();
		const convert = vi.fn(async (_md: string) => ({ html: '<ul><li><p>one</p></li><li><p>two</p></li></ul>' }));
		const { driver } = driverWith(f, convert);
		here.commands.setTextSelection(posIn(here, 'Intro text.', 'Intro text.'.length));
		driver.start({ editor: here, intent: 'continue', instruction: 'List them' });
		f.push('\n\n- one\n- two\n');
		f.end();
		await driver.whenIdle();

		expect(convert).toHaveBeenCalledWith('\n\n- one\n- two\n', expect.anything());
		const want = ['paragraph: Intro text.', 'bulletList: onetwo', 'paragraph: After.'];
		expect(blocks(here)).toEqual(want);
		expect(blocks(there)).toEqual(want);
		expect(idOf(here, 'Intro text.')).toBe(before.intro);
		expect(idOf(here, 'After.')).toBe(before.after);
	});

	it('joins inline formatting to the block it was written in', async () => {
		const { here } = twoDevices('<p>Intro</p>');
		const f = feed();
		const convert = vi.fn(async () => ({ html: '<p><strong>bold</strong> words</p>' }));
		const { driver } = driverWith(f, convert);
		here.commands.setTextSelection(posIn(here, 'Intro', 5));
		driver.start({ editor: here, intent: 'continue', instruction: 'Go on' });
		f.push(' **bold** words');
		f.end();
		await driver.whenIdle();
		expect(blocks(here)).toEqual(['paragraph: Intro bold words']);
		let bold = '';
		here.state.doc.descendants((n) => {
			if (n.isText && n.marks.some((m) => m.type.name === 'bold')) bold += n.text;
			return true;
		});
		expect(bold).toBe('bold');
	});

	it('keeps a rewrite proposed after converting it', async () => {
		const { here } = twoDevices('<p>Plain words here.</p>');
		const f = feed();
		const convert = vi.fn(async () => ({ html: '<p><strong>Bold</strong> words</p>' }));
		const { driver } = driverWith(f, convert);
		const from = posIn(here, 'Plain words');
		here.commands.setTextSelection({ from, to: from + 'Plain words'.length });
		driver.start({ editor: here, intent: 'rewrite', instruction: 'Bolder' });
		f.push('**Bold** words');
		f.end();
		await driver.whenIdle();
		const marked = proposals(here);
		expect(marked.map((m) => [m.mark, m.text])).toEqual([
			['proposedDeletion', 'Plain words'],
			['proposedInsertion', 'Bold'],
			['proposedInsertion', ' words'],
		]);
		expect(new Set(marked.map((m) => m.id)).size).toBe(1);
	});

	// A mention, a rule or a code block carries no proposal mark, so a reject
	// would leave it: a rewrite whose conversion makes one keeps its text.
	for (const [what, output, html] of [
		[
			'a mention',
			'Lunch with [@Nick](/person/person_1) on **Saturday**',
			'<p>Lunch with <virtues-mention to="/person/person_1" label="Nick"></virtues-mention> on <strong>Saturday</strong></p>',
		],
		['a rule', '**One**\n\n---\n\nTwo', '<p><strong>One</strong></p><hr><p>Two</p>'],
		['a code block', 'Run:\n\n```\nls\n```', '<p>Run:</p><pre><code>ls</code></pre>'],
	] as const) {
		it(`keeps a rewrite as it streamed when converting it would make ${what}, and reject takes it all back`, async () => {
			const page = '<p>Lunch on Friday.</p><p>After.</p>';
			const { here, there } = twoDevices(page);
			const original = here.state.doc.toJSON();
			const f = feed();
			const convert = vi.fn(async () => ({ html }));
			const { driver } = driverWith(f, convert);
			// The whole block, so a line break leaves no tail to carry over.
			const from = posIn(here, 'Lunch on Friday.');
			here.commands.setTextSelection({ from, to: from + 'Lunch on Friday.'.length });
			driver.start({ editor: here, intent: 'rewrite', instruction: 'Rewrite' });
			f.push(output);
			f.end();
			await driver.whenIdle();
			expect(convert).toHaveBeenCalled();
			let atoms = 0;
			here.state.doc.descendants((node) => {
				if (['mention', 'horizontalRule', 'codeBlock'].includes(node.type.name)) atoms += 1;
				return true;
			});
			expect(atoms).toBe(0);
			rejectProposal(here, proposalIds(here.state)[0]);
			expect(here.state.doc.toJSON()).toEqual(original);
			expect(there.state.doc.toJSON()).toEqual(original);
		});
	}

	it('still converts a rewrite into a list of its own blocks, which reject takes back', async () => {
		const page = '<p>Coffee and tea.</p><p>After.</p>';
		const { here } = twoDevices(page);
		const original = here.state.doc.toJSON();
		const f = feed();
		const convert = vi.fn(async () => ({ html: '<ul><li><p>Coffee</p></li><li><p>Tea</p></li></ul>' }));
		const { driver } = driverWith(f, convert);
		here.commands.setTextSelection({ from: 1, to: 1 + 'Coffee and tea.'.length });
		driver.start({ editor: here, intent: 'rewrite', instruction: 'As a list' });
		f.push('- Coffee\n- Tea');
		f.end();
		await driver.whenIdle();
		expect(blocks(here).some((b) => b.startsWith('bulletList'))).toBe(true);
		rejectProposal(here, proposalIds(here.state)[0]);
		expect(here.state.doc.toJSON()).toEqual(original);
	});

	it('leaves the text as it streamed when the server cannot convert it', async () => {
		const { here } = twoDevices('<p>Intro</p>');
		const f = feed();
		const convert = vi.fn(async () => {
			throw new Error('no');
		});
		const { driver } = driverWith(f, convert);
		here.commands.setTextSelection(posIn(here, 'Intro', 5));
		driver.start({ editor: here, intent: 'continue', instruction: 'Go on' });
		f.push(' **bold** words');
		f.end();
		await driver.whenIdle();
		expect(blocks(here)).toEqual(['paragraph: Intro **bold** words']);
	});

	it('leaves the text when someone changed those words before the server answered', async () => {
		const { here, there } = twoDevices('<p>Intro</p>');
		const f = feed();
		let release: (v: { html: string }) => void = () => {};
		const convert = vi.fn(() => new Promise<{ html: string }>((r) => (release = r)));
		const { driver } = driverWith(f, convert);
		here.commands.setTextSelection(posIn(here, 'Intro', 5));
		driver.start({ editor: here, intent: 'continue', instruction: 'Go on' });
		f.push(' **bold** words');
		f.end();
		await settle(20);
		expect(convert).toHaveBeenCalled();
		there.commands.insertContentAt(posIn(there, 'words', 2), 'XX');
		await settle();
		release({ html: '<p><strong>bold</strong> words</p>' });
		await driver.whenIdle();
		expect(blocks(here)).toEqual(['paragraph: Intro **bold** woXXrds']);
	});
});
