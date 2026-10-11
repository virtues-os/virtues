// @vitest-environment happy-dom
/**
 * The block editor (`createPageEditor` and the plugins `DocumentEditor`
 * mounts), against pages the server writes.
 *
 * The node views' Svelte components cannot be compiled here (no Svelte
 * plugin in vitest.config.ts), so each is replaced by a stand-in that
 * records what it was mounted with and writes into its own DOM, as a real
 * component does. `$state` outside the compiler is a plain object.
 */

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const mounted = vi.hoisted(() => {
	(globalThis as Record<string, unknown>).$state = <T>(v: T) => v;
	return [] as { name: string; props: { view: { attrs: Record<string, unknown>; type: string } } }[];
});

vi.mock('svelte', async (orig) => ({
	...(await orig<typeof import('svelte')>()),
	mount: (component: (target: Element, props: unknown) => void, o: { target: Element; props: unknown }) => {
		component(o.target, o.props);
		return {};
	},
	unmount: () => undefined,
}));

/** A stand-in component: draws into its target, as a mounted component does. */
function standIn(name: string) {
	return (target: Element, props: (typeof mounted)[number]['props']) => {
		mounted.push({ name, props });
		const el = document.createElement('span');
		el.className = `stand-in-${name}`;
		el.textContent = name;
		target.append(el);
	};
}

vi.mock('$lib/components/pages/nodes/MentionNode.svelte', () => ({ default: standIn('MentionNode') }));
vi.mock('$lib/components/pages/nodes/MediaNode.svelte', () => ({ default: standIn('MediaNode') }));
vi.mock('$lib/components/pages/nodes/AppletNode.svelte', () => ({ default: standIn('AppletNode') }));
vi.mock('$lib/components/pages/nodes/CalloutNode.svelte', () => ({ default: standIn('CalloutNode') }));
vi.mock('$lib/components/pages/nodes/CodeBlockNode.svelte', () => ({ default: standIn('CodeBlockNode') }));
vi.mock('$lib/stores/contextMenu.svelte', () => ({ contextMenu: { show: vi.fn(), reachFirstRow: vi.fn() } }));
vi.mock('$lib/stores/linkEditor.svelte', () => ({ linkEditor: { show: vi.fn() } }));
vi.mock('$lib/stores/window-shell.svelte', () => ({
	windowShellStore: { openRouteBeside: vi.fn(), navigate: vi.fn() },
}));
vi.mock('svelte-sonner', () => ({ toast: Object.assign(vi.fn(), { error: vi.fn() }) }));
vi.mock('$lib/api/client', async (orig) => ({
	...(await orig<typeof import('$lib/api/client')>()),
	convertPastedMarkdown: vi.fn(async () => ({ html: '', notes: [] })),
	getApplet: vi.fn(async (id: string) => ({ id, name: 'Sleep this week' })),
}));

import { Editor } from '@tiptap/core';
import { NodeSelection, TextSelection } from '@tiptap/pm/state';
import { CellSelection } from '@tiptap/pm/tables';
import { GapCursor } from '@tiptap/pm/gapcursor';
import { yXmlFragmentToProseMirrorRootNode } from '@tiptap/y-tiptap';
import * as Y from 'yjs';
import { filterCommands, menuKey } from '$lib/components/menuCommand';
import { getDefaultSlashCommands } from '$lib/codemirror/extensions/slash-commands';
import { createPageEditor } from './editor';
import { contract, contractExtensions } from './schema';
import { appletCommands, insertEntity, moveBlock, treeCommands, type TreeCommandHost } from './commands';
import { pickedRange, putBackQuery, triggers, triggerState } from './triggers';
import { fileIcon, parseRef } from '$lib/utils/refRoutes';
import { clearFind, find, findMatches, findNext, findPrevious, findState, selectionEchoes, setFindQuery } from './find';
import { focusMode } from './focus';
import { codeHighlight } from './code';
import { isCodeEditorHtml, looksLikeMarkdown, pasteHandling, pasteMarkdown, wordLists } from './paste';
import { mediaUploads, pendingUploads, uploadFiles, driveFileId, mediaSrc, nameOfSrc } from './media';
import { kindOfLink } from './media-kind';
import { decoded } from '$lib/utils/urlUtils';
import { svelteNodeView } from './nodeviews.svelte';
import { editLinkAtSelection, isRef, linkAt, linkMenu, links, openLink, openLinkBeside } from './links';
import { anchorAt, anchorRange, pinAt, placeOf, resolveRange } from './anchor';
import { linkEditor } from '$lib/stores/linkEditor.svelte';
import { toast } from 'svelte-sonner';
import { convertPastedMarkdown, type Applet } from '$lib/api/client';
import { aiPresence, clearAi, flashBlocks, flashedBlocks, setAiCaret, setAiTrail } from './presence';
import { contextGesture } from './gesture';
import { acceptAllProposals, acceptProposal, proposalAt, proposalIds, rejectProposal } from './suggestions';
import { alignColumn, columnAlign, tableActions } from './tables';
import { scrollerOf, treeHeadings, treeOutlineNav } from './outline';
import { treeStats } from './stats';
import { getTreeEditor, registerTreeEditor, unregisterTreeEditor } from './registry';
import { fakeProvider, htmlEditor, pageDoc, posIn, settle } from './test-utils';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const COMPONENTS = path.resolve(HERE, '../components/pages/nodes');
const MEDIA_KINDS = path.resolve(HERE, '../../../../../crates/virtues-document/tests/corpus/media-kinds.json');

/**
 * A page with every widget the editor draws, ending in a table: the shape
 * that tempts a plugin to write (a trailing paragraph, a table fix, ids).
 * Fictional people only.
 */
const FIXTURE = [
	'<h2>Trip to Lisbon</h2>',
	'<p>Lunch with <virtues-mention to="/person/person_1" label="Nick"></virtues-mention> and <virtues-mention to="/person/person_2" label="David Okafor"></virtues-mention>.</p>',
	'<ul data-type="taskList"><li data-type="taskItem" data-checked="false"><p>Book seats</p></li><li data-type="taskItem" data-checked="true"><p>Pack</p></li></ul>',
	'<aside data-tone="tip"><p>Bring a jacket.</p></aside>',
	'<pre data-language="rust"><code>fn main() {}</code></pre>',
	'<virtues-applet ref="sleep-week" height="200"></virtues-applet>',
	'<img src="https://images.example.com/photo-1?w=400" alt="Harbour">',
	'<h3>Costs</h3>',
	'<table><tbody><tr><th><p>Day</p></th><th align="right"><p>Cost</p></th></tr><tr><td><p>Fri</p></td><td align="right"><p>40</p></td></tr></tbody></table>',
].join('');

/** Every plugin `DocumentEditor` mounts, with hosts that do nothing. */
function pagePlugins() {
	return [
		triggers([
			{ char: '/', onOpen: () => {}, onQuery: () => {}, onClose: () => {} },
			{ char: '@', onOpen: () => {}, onQuery: () => {}, onClose: () => {} },
		]),
		find({ onOpen: () => {}, onClose: () => {} }),
		focusMode(true),
		codeHighlight(async (code) => [[{ content: code, color: 'var(--color-primary)', offset: 0 }]]),
		pasteHandling(async () => ({ html: '' })),
		mediaUploads(async () => ({ url: '/api/media/x', filename: 'x', mime_type: null })),
		links({ onHover: () => {}, onLeave: () => {} }),
		aiPresence(),
		contextGesture([]),
	];
}

const editors: Editor[] = [];
const cleanups: (() => void)[] = [];

function bind(doc: Y.Doc, opts: { plugins?: boolean } = {}) {
	const provider = fakeProvider(doc);
	cleanups.push(provider.destroy);
	const element = document.createElement('div');
	document.body.append(element);
	const editor = createPageEditor({
		element,
		doc: { ydoc: doc, provider },
		editable: true,
		placeholder: 'Write',
		user: { name: 'Computer', tint: 'orange' },
		plugins: opts.plugins === false ? [] : pagePlugins(),
	});
	editors.push(editor);
	return { editor, provider, element };
}

function track<T extends Editor>(editor: T): T {
	editors.push(editor);
	return editor;
}

beforeEach(() => {
	mounted.length = 0;
});

afterEach(() => {
	for (const e of editors.splice(0)) if (!e.isDestroyed) e.destroy();
	for (const c of cleanups.splice(0)) c();
	document.body.replaceChildren();
});

/** The tree a Yjs page holds, as JSON. */
function treeOf(doc: Y.Doc) {
	const editor = htmlEditor('<p>x</p>');
	track(editor);
	return yXmlFragmentToProseMirrorRootNode(doc.getXmlFragment(contract.fragment), editor.schema).toJSON();
}

/** Ids of every block that carries one, in document order. */
function ids(editor: Editor): string[] {
	const out: string[] = [];
	editor.state.doc.descendants((node) => {
		const id = node.attrs[contract.id.attr];
		if (typeof id === 'string') out.push(id);
		return true;
	});
	return out;
}

describe('opening a page writes nothing', () => {
	it('mounting, resizing and blurring an editor on a server-written page send no update', async () => {
		const { doc, writtenBy } = pageDoc(FIXTURE);
		const before = Y.encodeStateVector(doc);
		const writes: unknown[] = [];
		const { editor, provider, element } = (() => {
			const bound = bind(doc);
			return bound;
		})();
		doc.on('update', (_u: Uint8Array, origin: unknown) => {
			if (origin !== provider) writes.push(origin);
		});
		await settle(250);
		expect(writes, `writes after mount (page written by ${writtenBy})`).toEqual([]);

		element.style.width = '320px';
		window.dispatchEvent(new Event('resize'));
		await settle(50);
		expect(writes, 'writes after a resize').toEqual([]);

		editor.commands.focus('end');
		editor.commands.blur();
		await settle(50);
		expect(writes, 'writes after a blur').toEqual([]);

		// The socket reconnecting asks UniqueID for missing ids: there are none.
		provider.emit('synced', true);
		await settle(20);
		expect(writes, 'writes after a reconnect').toEqual([]);
		expect(Y.encodeStateVector(doc)).toEqual(before);

		// The widgets were drawn by their views, and a callout's chrome
		// written into its DOM is not read back as content.
		const names = mounted.map((m) => m.name);
		expect(names).toContain('MentionNode');
		expect(names).toContain('CalloutNode');
		expect(names).toContain('CodeBlockNode');
		expect(names).toContain('MediaNode');
		expect(editor.state.doc.textContent).not.toContain('CalloutNode');
	});

	// Autolink reads every range a transaction changes, and binding a page
	// changes all of it: a first block ending in a word that reads as a
	// domain would gain a link, written by the next transaction of any kind.
	for (const first of ['Notes kept at example.com', 'Run main.py', 'See README.md']) {
		it(`a first block ending in "${first}" is not linked by opening it`, async () => {
			const { doc, writtenBy } = pageDoc(`<p>${first}</p><p>Second.</p>`);
			const before = Y.encodeStateVector(doc);
			const writes: unknown[] = [];
			const { editor, provider } = bind(doc);
			doc.on('update', (_u: Uint8Array, origin: unknown) => {
				if (origin !== provider) writes.push(origin);
			});
			await settle(100);
			editor.view.dispatch(editor.state.tr.setMeta('noop', true));
			editor.commands.focus('end');
			editor.commands.blur();
			await settle(50);
			expect(writes, `writes (page written by ${writtenBy})`).toEqual([]);
			expect(Y.encodeStateVector(doc)).toEqual(before);
			expect(editor.getJSON().content?.[0].content?.[0].marks ?? []).toEqual([]);
		});
	}

	it('a domain typed and followed by a space is still linked', async () => {
		const { doc } = pageDoc('<p>Second.</p>');
		const { editor } = bind(doc);
		await settle(50);
		editor.commands.setTextSelection(1);
		editor.commands.insertContent('example.com ');
		await settle(20);
		const link = editor.state.doc.firstChild?.firstChild?.marks.find((m) => m.type.name === 'link');
		expect(link?.attrs.href).toBe('http://example.com');
	});

	it('the last block stays a table: nothing appends a paragraph after it', async () => {
		const { doc } = pageDoc(FIXTURE);
		const { editor } = bind(doc);
		await settle(50);
		expect(editor.state.doc.lastChild?.type.name).toBe('table');
	});
});

describe('a click below the last block', () => {
	/** Click 100 px under the page's last block, then type `x` as ProseMirror's keypress does. */
	function clickBelowAndType(html: string) {
		const { doc } = pageDoc(html);
		const { editor } = bind(doc);
		const last = editor.view.dom.lastElementChild!.getBoundingClientRect();
		const down = new MouseEvent('mousedown', { button: 0, clientX: 10, clientY: last.bottom + 100, bubbles: true, cancelable: true });
		editor.view.dom.dispatchEvent(down);
		const selection = editor.state.selection;
		editor.view.dispatch(editor.state.tr.insertText('x'));
		return { editor, handled: down.defaultPrevented, selection };
	}

	const shape = (editor: Editor) => JSON.stringify(editor.getJSON().content?.map(function strip(n: Record<string, unknown>): unknown {
		const content = n.content as Record<string, unknown>[] | undefined;
		return n.type === 'text' ? n.text : [n.type, ...(content ?? []).map(strip)];
	}));

	it('ends in the last text of a page that ends in text, however deep', () => {
		const { editor, handled, selection } = clickBelowAndType('<p>Intro</p><aside data-tone="tip"><ul><li><p>Pack</p></li></ul></aside>');
		expect(handled).toBe(true);
		expect(selection.empty).toBe(true);
		expect(shape(editor)).toBe('[["paragraph","Intro"],["callout",["bulletList",["listItem",["paragraph","Packx"]]]]]');
	});

	it('adds a paragraph after a widget that ends a callout, a quote or a list, which the next key never replaces', () => {
		for (const [html, typed] of [
			[
				'<p>Intro</p><aside data-tone="tip"><p>Harbour at dusk</p><img src="/api/media/harbour.png" alt="Harbour"></aside>',
				'[["paragraph","Intro"],["callout",["paragraph","Harbour at dusk"],["image"],["paragraph","x"]]]',
			],
			[
				'<blockquote><p>Quote</p><virtues-applet ref="sleep-week"></virtues-applet></blockquote>',
				'[["blockquote",["paragraph","Quote"],["applet"],["paragraph","x"]]]',
			],
			['<ul><li><p>Item</p><hr></li></ul>', '[["bulletList",["listItem",["paragraph","Item"],["horizontalRule"],["paragraph","x"]]]]'],
			['<p>Intro</p><img src="/api/media/harbour.png">', '[["paragraph","Intro"],["image"],["paragraph","x"]]'],
		]) {
			const { editor, selection } = clickBelowAndType(html);
			expect(selection instanceof TextSelection, html).toBe(true);
			expect(shape(editor), html).toBe(typed);
		}
	});
});

describe('moving a block while another device edits', () => {
	/** Two devices on one page, each with its editor; `exchange` carries each one's updates to the other. */
	async function twoDevices(html: string) {
		const { doc: laptopDoc } = pageDoc(html);
		const phoneDoc = new Y.Doc();
		Y.applyUpdate(phoneDoc, Y.encodeStateAsUpdate(laptopDoc));
		const laptop = bind(laptopDoc, { plugins: false }).editor;
		const phone = bind(phoneDoc, { plugins: false }).editor;
		await settle();
		const exchange = async () => {
			const toPhone = Y.encodeStateAsUpdate(laptopDoc, Y.encodeStateVector(phoneDoc));
			const toLaptop = Y.encodeStateAsUpdate(phoneDoc, Y.encodeStateVector(laptopDoc));
			Y.applyUpdate(phoneDoc, toPhone, 'wire');
			Y.applyUpdate(laptopDoc, toLaptop, 'wire');
			await settle();
		};
		return { laptop, phone, laptopDoc, exchange };
	}

	const texts = (editor: Editor) => {
		const out: string[] = [];
		editor.state.doc.descendants((n) => {
			if (n.isTextblock) out.push(n.textContent);
			return true;
		});
		return out;
	};

	const typeAtEnd = (editor: Editor, text: string, words: string) =>
		editor.view.dispatch(editor.state.tr.insertText(words, posIn(editor, text, text.length)));

	// Swapped in one edit, the binding rewrote each block's element with the
	// other's content: words typed into one block on another device landed
	// in the other.
	for (const [what, html] of [
		['a two-block page', '<p>One</p><p>Two</p>'],
		['a two-item list', '<ul><li><p>One</p></li><li><p>Two</p></li></ul><p>After</p>'],
	] as const) {
		it(`on ${what}, words typed into the neighbour stay in it, and the moved block's own never land in another`, async () => {
			const neighbour = await twoDevices(html);
			const kept = neighbour.laptopDoc.getXmlFragment(contract.fragment).toArray();
			neighbour.laptop.commands.setTextSelection(posIn(neighbour.laptop, 'Two', 1));
			typeAtEnd(neighbour.phone, 'One', ' from phone');
			expect(moveBlock(neighbour.laptop, -1)).toBe(true);
			await neighbour.exchange();
			for (const editor of [neighbour.laptop, neighbour.phone]) {
				expect(texts(editor).slice(0, 2)).toEqual(['Two', 'One from phone']);
			}
			if (what === 'a two-block page') {
				// The neighbour's element is the one it had: only the moved block's is new.
				expect(neighbour.laptopDoc.getXmlFragment(contract.fragment).toArray()[1]).toBe(kept[0]);
			}

			const moved = await twoDevices(html);
			moved.laptop.commands.setTextSelection(posIn(moved.laptop, 'Two', 1));
			typeAtEnd(moved.phone, 'Two', ' from phone');
			moveBlock(moved.laptop, -1);
			await moved.exchange();
			for (const editor of [moved.laptop, moved.phone]) {
				expect(texts(editor).slice(0, 2)[1]).toBe('One');
				expect(texts(editor).join('|')).not.toContain('One from phone');
				expect(texts(editor).join('|')).not.toContain(' from phoneOne');
			}
		});
	}

	it('a moved block keeps its id and the caret, and one undo puts it back', async () => {
		const { laptop } = await twoDevices('<p>One</p><p>Two</p><p>Three</p>');
		const id = laptop.state.doc.child(1).attrs[contract.id.attr];
		laptop.commands.setTextSelection(posIn(laptop, 'Two', 2));
		moveBlock(laptop, 1);
		expect(texts(laptop)).toEqual(['One', 'Three', 'Two']);
		expect(laptop.state.doc.child(2).attrs[contract.id.attr]).toBe(id);
		expect(laptop.state.selection.$from.parent.textContent).toBe('Two');
		expect(laptop.state.selection.$from.parentOffset).toBe(2);
		laptop.commands.undo();
		expect(texts(laptop)).toEqual(['One', 'Two', 'Three']);
	});
});

describe('another device writing while the person works', () => {
	/** This device's editor and the phone's on one page, each one's updates reaching the other at once. */
	async function devices(html: string) {
		const { doc } = pageDoc(html);
		const other = new Y.Doc();
		Y.applyUpdate(other, Y.encodeStateAsUpdate(doc));
		doc.on('update', (u: Uint8Array, origin: unknown) => {
			if (origin !== 'wire') Y.applyUpdate(other, u, 'wire');
		});
		other.on('update', (u: Uint8Array, origin: unknown) => {
			if (origin !== 'wire') Y.applyUpdate(doc, u, 'wire');
		});
		const laptop = bind(doc, { plugins: false }).editor;
		const phone = bind(other, { plugins: false }).editor;
		await settle();
		return { laptop, phone };
	}

	const TABLE = '<p>Intro</p><table><tbody><tr><td><p>a1</p></td><td><p>b1</p></td></tr><tr><td><p>a2</p></td><td><p>b2</p></td></tr></tbody></table>';

	/** The position before the cell holding `text`. */
	function cellBefore(editor: Editor, text: string): number {
		let found = -1;
		editor.state.doc.descendants((node, pos) => {
			if (node.type.spec.tableRole === 'cell' && node.textContent === text) found = pos;
			return found < 0;
		});
		return found;
	}

	const cells = (editor: Editor) => {
		const out: string[] = [];
		editor.state.doc.descendants((node) => {
			if (node.type.spec.tableRole === 'cell') out.push(node.textContent);
			return true;
		});
		return out;
	};

	// The binding places a selection again as text or a node only: the cells
	// became the words of one cell, and Backspace emptied that cell alone.
	it('selected table cells stay selected, and Backspace then empties every one', async () => {
		const { laptop, phone } = await devices(TABLE);
		laptop.view.dispatch(
			laptop.state.tr.setSelection(CellSelection.create(laptop.state.doc, cellBefore(laptop, 'b1'), cellBefore(laptop, 'a2'))),
		);
		phone.commands.insertContentAt(posIn(phone, 'Intro', 5), ' more');
		await settle();
		expect(laptop.state.doc.firstChild!.textContent).toBe('Intro more');
		expect(laptop.state.selection).toBeInstanceOf(CellSelection);
		expect(laptop.state.selection.ranges).toHaveLength(4);
		// Every cell of the table selected: Backspace takes it whole, as it does with nobody else writing.
		laptop.view.dom.dispatchEvent(new KeyboardEvent('keydown', { key: 'Backspace', bubbles: true, cancelable: true }));
		expect(cells(laptop)).toEqual([]);

		const row = await devices(TABLE);
		row.laptop.view.dispatch(
			row.laptop.state.tr.setSelection(CellSelection.create(row.laptop.state.doc, cellBefore(row.laptop, 'a1'), cellBefore(row.laptop, 'b1'))),
		);
		row.phone.commands.insertContentAt(posIn(row.phone, 'Intro', 5), ' more');
		await settle();
		expect(row.laptop.state.selection.ranges).toHaveLength(2);
		row.laptop.view.dom.dispatchEvent(new KeyboardEvent('keydown', { key: 'Backspace', bubbles: true, cancelable: true }));
		expect(cells(row.laptop)).toEqual(['', '', 'a2', 'b2']);
		expect(cells(row.phone)).toEqual(['', '', 'a2', 'b2']);
	});

	it('a gap cursor after a final table stays there, and typing makes a line after the table', async () => {
		const { laptop, phone } = await devices(TABLE);
		laptop.view.dispatch(laptop.state.tr.setSelection(new GapCursor(laptop.state.doc.resolve(laptop.state.doc.content.size))));
		phone.commands.insertContentAt(posIn(phone, 'Intro', 5), ' more');
		await settle();
		expect(laptop.state.selection).toBeInstanceOf(GapCursor);
		laptop.commands.insertContent('New line');
		expect(cells(laptop)).toEqual(['a1', 'b1', 'a2', 'b2']);
		expect(laptop.state.doc.lastChild!.type.name).toBe('paragraph');
		expect(laptop.state.doc.lastChild!.textContent).toBe('New line');
	});

	// The binding applies another device's edit as a step, and a step clears
	// the marks waiting to be typed: ⌘B, the phone wrote, and the words came out plain.
	for (const mark of ['bold', 'italic', 'code', 'strike', 'highlight'] as const) {
		it(`${mark} turned on at the caret is still on for the next words after the phone writes`, async () => {
			const { laptop, phone } = await devices('<p>Hello</p><p>Other</p>');
			laptop.commands.setTextSelection(posIn(laptop, 'Hello', 5));
			laptop.commands.toggleMark(mark);
			expect(laptop.state.storedMarks?.map((m) => m.type.name)).toEqual([mark]);
			phone.commands.insertContentAt(posIn(phone, 'Other', 5), '!');
			await settle();
			expect(laptop.state.doc.lastChild!.textContent).toBe('Other!');
			expect(laptop.state.storedMarks?.map((m) => m.type.name)).toEqual([mark]);
			typeIn(laptop, ' world');
			const hello = laptop.state.doc.firstChild!;
			expect(hello.textContent).toBe('Hello world');
			expect(hello.lastChild!.text).toBe(' world');
			expect(hello.lastChild!.marks.map((m) => m.type.name)).toEqual([mark]);
		});
	}
});

describe('a merge the server repairs', () => {
	// Each device deletes one of an item's two leading paragraphs; the merge
	// leaves the item a nested list with no paragraph, which the schema
	// refuses until the server's repair adds one. Tiptap's binding deleted
	// such a node from the shared document, the nested list with it.
	it('a device holding a merge it cannot draw draws it filled, and deletes nothing', async () => {
		const { doc: laptopDoc, writtenBy } = pageDoc(
			'<ul><li><p>p1</p><p>p2</p><ul><li><p>nested item the person wrote</p></li></ul></li></ul><p>after</p>',
		);
		const phoneDoc = new Y.Doc();
		Y.applyUpdate(phoneDoc, Y.encodeStateAsUpdate(laptopDoc));
		const laptop = bind(laptopDoc, { plugins: false }).editor;
		const phone = bind(phoneDoc, { plugins: false }).editor;
		await settle();
		const outbox = new Map<Y.Doc, Uint8Array[]>([
			[laptopDoc, []],
			[phoneDoc, []],
		]);
		for (const d of [laptopDoc, phoneDoc]) {
			d.on('update', (u: Uint8Array, origin: unknown) => {
				if (origin !== 'wire') outbox.get(d)!.push(u);
			});
		}
		const deleteParagraph = (editor: Editor, text: string) => {
			const at = posIn(editor, text) - 1;
			editor.view.dispatch(editor.state.tr.delete(at, at + editor.state.doc.nodeAt(at)!.nodeSize));
		};
		deleteParagraph(laptop, 'p1');
		deleteParagraph(phone, 'p2');
		// The socket carries each device's updates to the other, until neither sends more.
		for (let round = 0; round < 3; round++) {
			const fromLaptop = outbox.get(laptopDoc)!.splice(0);
			const fromPhone = outbox.get(phoneDoc)!.splice(0);
			for (const u of fromLaptop) Y.applyUpdate(phoneDoc, u, 'wire');
			for (const u of fromPhone) Y.applyUpdate(laptopDoc, u, 'wire');
			await settle();
		}
		for (const [name, d, editor] of [
			['laptop', laptopDoc, laptop],
			['phone', phoneDoc, phone],
		] as const) {
			expect(d.getXmlFragment(contract.fragment).toString(), `${name}'s shared document (page written by ${writtenBy})`).toContain(
				'nested item the person wrote',
			);
			expect(editor.state.doc.textContent, name).toContain('nested item the person wrote');
		}
	});
});

describe('block ids across two devices', () => {
	function linked() {
		const a = pageDoc('<h2>Plan</h2><p>First line</p>').doc;
		const b = new Y.Doc();
		Y.applyUpdate(b, Y.encodeStateAsUpdate(a));
		const fromB: unknown[] = [];
		a.on('update', (u: Uint8Array, origin: unknown) => {
			if (origin !== 'wire') Y.applyUpdate(b, u, 'wire');
		});
		b.on('update', (u: Uint8Array, origin: unknown) => {
			if (origin === 'wire') return;
			fromB.push(origin);
			Y.applyUpdate(a, u, 'wire');
		});
		return { a, b, fromB };
	}

	it('keep the ids one device gave, and settle with no further update', async () => {
		const { a, b, fromB } = linked();
		const one = bind(a, { plugins: false }).editor;
		const two = bind(b, { plugins: false }).editor;
		await settle();
		one.commands.insertContentAt(one.state.doc.content.size, '<p>Added on one</p>');
		await settle();
		expect(ids(one)).toHaveLength(3);
		expect(ids(two)).toEqual(ids(one));
		expect(fromB, 'the second device wrote ids of its own').toEqual([]);
		expect(treeOf(b)).toEqual(treeOf(a));
	});

	it('give a block split in two a fresh id', async () => {
		const { a, b } = linked();
		const one = bind(a, { plugins: false }).editor;
		const two = bind(b, { plugins: false }).editor;
		await settle();
		const before = ids(one);
		one.chain().setTextSelection(posIn(one, 'First line', 5)).splitBlock().run();
		await settle();
		const after = ids(one);
		expect(after).toHaveLength(3);
		expect(new Set(after).size).toBe(3);
		expect(after.slice(0, 2)).toEqual(before);
		expect(ids(two)).toEqual(after);
	});
});

describe('the insert menu', () => {
	const host: TreeCommandHost = { pickFiles: vi.fn(), pickApplet: vi.fn(), mention: vi.fn(), askAi: vi.fn() };
	const commands = treeCommands(host);

	/** Type `/query` in an empty paragraph and run the first command it matches. */
	function run(query: string, html = '<p></p>') {
		const editor = track(htmlEditor(html));
		editor.commands.setTextSelection(1);
		editor.commands.insertContent(`/${query}`);
		const cmd = filterCommands(commands, query)[0];
		expect(cmd, `/${query} matches nothing`).toBeDefined();
		cmd.run(editor, { from: 1, to: editor.state.selection.from });
		return { editor, cmd };
	}

	it('/task makes a to-do list', () => {
		const { editor, cmd } = run('task');
		expect(cmd.label).toBe('To-do list');
		expect(editor.state.doc.firstChild?.type.name).toBe('taskList');
		expect(editor.state.doc.textContent).toBe('');
	});

	it('/h2, /callout, /code and /table make their blocks', () => {
		expect(run('h2').editor.getJSON().content?.[0]).toMatchObject({ type: 'heading', attrs: { level: 2 } });
		expect(run('callout').editor.state.doc.firstChild?.type.name).toBe('callout');
		expect(run('tip').editor.state.doc.firstChild?.attrs.tone).toBe('tip');
		expect(run('code').editor.state.doc.firstChild?.type.name).toBe('codeBlock');
		const table = run('table').editor.state.doc.firstChild;
		expect(table?.type.name).toBe('table');
		expect(table?.childCount).toBe(3);
	});

	// A `/` in a path opens the menu, which matches nothing; it took Enter,
	// Tab and the arrows all the same, so the line could not be ended.
	it('with no command matching, the menu leaves Enter, Tab and the arrows to the editor', () => {
		const keys = ['Enter', 'Tab', 'ArrowUp', 'ArrowDown'];
		expect(keys.map((k) => menuKey(k, 0))).toEqual([null, null, null, null]);
		expect(keys.map((k) => menuKey(k, 3))).toEqual(['run', 'run', 'previous', 'next']);
		expect(menuKey('Escape', 0)).toBe('close');
		expect(menuKey('a', 3)).toBeNull();
		const slash = fs.readFileSync(path.resolve(HERE, '../components/SlashMenu.svelte'), 'utf8');
		expect(slash).toMatch(/const action = menuKey\(e\.key, flatCommands\.length\);\s+if \(!action\) return;\s+e\.preventDefault\(\);/);

		const editor = track(htmlEditor('<p></p>', [triggers([{ char: '/', onOpen: () => {}, onQuery: () => {}, onClose: () => {} }])]));
		editor.commands.setTextSelection(1);
		typeIn(editor, 'see /etc/hosts');
		const open = triggerState(editor.state, '/');
		expect(open).toMatchObject({ active: true, query: 'etc/hosts' });
		expect(filterCommands(commands, open.query)).toEqual([]);
		// Enter, left to the editor, ends the line, which closes the menu.
		editor.view.dom.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }));
		expect(editor.state.doc.childCount).toBe(2);
		expect(editor.state.doc.child(0).textContent).toBe('see /etc/hosts');
		expect(triggerState(editor.state, '/').active).toBe(false);
	});

	// Text came first, so `/` and Enter, and `/h` and Enter (`paragraph`
	// holds an h), made a paragraph a paragraph: on a markdown page they
	// make Heading 1.
	it('`/` and Enter, `/h` and `/t` and Enter, make Heading 1, as on a markdown page', () => {
		for (const query of ['', 'h', 't']) {
			const first = filterCommands(commands, query)[0]?.label;
			expect(first, `/${query}`).toBe('Heading 1');
			expect(first, `/${query}`).toBe(filterCommands(getDefaultSlashCommands(), query)[0]?.label);
		}
		expect(filterCommands(commands, 'text')[0]?.label).toBe('Text');
		expect(filterCommands(commands, 'paragraph')[0]?.label).toBe('Text');
	});

	it('shows headings 4 to 6 only once they are asked for', () => {
		const labels = filterCommands(commands, '').map((c) => c.label);
		expect(labels).not.toContain('Heading 4');
		expect(filterCommands(commands, 'h5').map((c) => c.label)).toEqual(['Heading 5']);
	});

	it('hands pickers to the host where the trigger was', () => {
		run('image');
		expect(host.pickFiles).toHaveBeenCalledWith('image', 1);
		run('mention');
		expect(host.mention).toHaveBeenCalledWith(1);
	});

	it('`@` and a person make a mention, then a space', () => {
		const editor = track(htmlEditor('<p>Lunch with</p>'));
		const at = posIn(editor, 'Lunch with', 'Lunch with'.length) + 1;
		editor.commands.insertContentAt(at - 1, ' @');
		insertEntity(editor, { from: at, to: at + 1 }, { name: 'Nick', url: '/person/person_1', entity_type: 'person' });
		const p = editor.getJSON().content?.[0];
		expect(p?.content?.[1]).toMatchObject({ type: 'mention', attrs: { to: '/person/person_1', label: 'Nick' } });
		expect(editor.state.doc.textContent).toBe('Lunch with  ');
		expect(editor.getHTML()).toContain('<virtues-mention to="/person/person_1" label="Nick"></virtues-mention>');
	});

	it('a Drive file of type audio/mpeg becomes an audio block named after it', () => {
		const editor = track(htmlEditor('<p>Notes</p><p></p>'));
		insertEntity(
			editor,
			{ from: posIn(editor, 'Notes', 5) + 2, to: posIn(editor, 'Notes', 5) + 2 },
			{ name: 'memo.mp3', url: '/drive/df_3', entity_type: 'file', mime_type: 'audio/mpeg' },
		);
		const audio = editor.getJSON().content?.find((n) => n.type === 'audio');
		expect(audio).toMatchObject({ attrs: { src: '/drive/df_3', name: 'memo.mp3' } });
	});

	it('a quote opened with [!tip] becomes a tip callout', () => {
		const { doc } = pageDoc('<p>x</p>');
		const { editor } = bind(doc, { plugins: false });
		editor.chain().setTextSelection(posIn(editor, 'x', 1)).insertContent('<blockquote><p></p></blockquote>').run();
		const inQuote = editor.state.selection.from;
		editor.view.dispatch(editor.state.tr.setSelection(TextSelection.create(editor.state.doc, inQuote)));
		for (const ch of '[!tip] ') {
			const { from, to } = editor.state.selection;
			const handled = editor.view.someProp('handleTextInput', (f) => f(editor.view, from, to, ch, () => editor.state.tr.insertText(ch, from, to)));
			if (!handled) editor.view.dispatch(editor.state.tr.insertText(ch, from, to));
		}
		const callout = editor.getJSON().content?.find((n) => n.type === 'callout');
		expect(callout).toMatchObject({ attrs: { tone: 'tip' } });
		expect(JSON.stringify(callout)).not.toContain('[!tip]');
	});
});

/** Type `text` a key at a time, as a keyboard does. */
function typeIn(editor: Editor, text: string) {
	for (const ch of text) {
		const { from, to } = editor.state.selection;
		const handled = editor.view.someProp('handleTextInput', (f) => f(editor.view, from, to, ch, () => editor.state.tr.insertText(ch, from, to)));
		if (!handled) editor.view.dispatch(editor.state.tr.insertText(ch, from, to));
	}
}

/** Send `text` in one input, as an input method, dictation or a paste of keys does. */
function sendAtOnce(editor: Editor, text: string) {
	const { from, to } = editor.state.selection;
	editor.view.someProp('handleTextInput', (f) => f(editor.view, from, to, text, () => editor.state.tr.insertText(text, from, to)));
}

describe('typed triggers', () => {

	it('opens on `/` at a block start, follows the query, and closes on a space', () => {
		const queries: string[] = [];
		const editor = track(htmlEditor('<p></p>', [triggers([{ char: '/', onOpen: () => {}, onQuery: (q) => queries.push(q), onClose: () => {} }])]));
		editor.commands.setTextSelection(1);
		typeIn(editor, '/ta');
		expect(triggerState(editor.state, '/')).toMatchObject({ active: true, from: 1, query: 'ta' });
		expect(queries).toEqual(['t', 'ta']);
		typeIn(editor, ' ');
		expect(triggerState(editor.state, '/').active).toBe(false);
	});

	it('does not open in the middle of a word, or while an input method composes', () => {
		const editor = track(htmlEditor('<p>and</p>', [triggers([{ char: '/', onOpen: () => {}, onQuery: () => {}, onClose: () => {} }])]));
		editor.commands.setTextSelection(4);
		typeIn(editor, '/');
		expect(triggerState(editor.state, '/').active).toBe(false);
		editor.commands.setTextSelection(1);
		Object.defineProperty(editor.view, 'composing', { value: true, configurable: true });
		typeIn(editor, '/');
		expect(triggerState(editor.state, '/').active).toBe(false);
	});

	it('a pick replaces the `@` and what was typed after it, typed or sent at once', () => {
		const david = { name: 'David Okafor', url: '/person/person_2', entity_type: 'person' };
		const at = () => [triggers([{ char: '@', onOpen: () => {}, onQuery: () => {}, onClose: () => {} }])];
		for (const send of [(e: Editor) => typeIn(e, ' @Da'), (e: Editor) => {
			const { from, to } = e.state.selection;
			e.view.someProp('handleTextInput', (f) => f(e.view, from, to, ' @Da', () => e.state.tr.insertText(' @Da', from, to)));
		}]) {
			const editor = track(htmlEditor('<p>Lunch with</p>', at()));
			editor.commands.setTextSelection(posIn(editor, 'Lunch with', 'Lunch with'.length));
			send(editor);
			expect(triggerState(editor.state, '@')).toMatchObject({ active: true, query: 'Da' });
			insertEntity(editor, pickedRange(editor.state, '@', { from: 0, to: 0 }), david);
			const p = editor.getJSON().content?.[0];
			expect(p?.content).toEqual([
				{ type: 'text', text: 'Lunch with ' },
				{ type: 'mention', attrs: { to: '/person/person_2', label: 'David Okafor' } },
				{ type: 'text', text: ' ' },
			]);
		}
	});

	// ProseMirror reads text an input method composed without asking the
	// plugin again, so a `@` an Android keyboard composed with the word after
	// it never opened the picker; the CodeMirror editor opens it.
	it('opens on a trigger an input method composed, once the composition is done', async () => {
		const compose = (e: Editor, text: string) => {
			e.view.dom.dispatchEvent(new CompositionEvent('compositionstart', { bubbles: true }));
			expect(e.view.composing).toBe(true);
			typeIn(e, text);
			e.view.dom.dispatchEvent(new CompositionEvent('compositionend', { bubbles: true, data: text }));
		};
		const queries: string[] = [];
		const at = () => [triggers([{ char: '@', onOpen: () => {}, onQuery: (q) => queries.push(q), onClose: () => {} }])];
		const editor = track(htmlEditor('<p>Ask me</p>', at()));
		editor.commands.setTextSelection(posIn(editor, 'Ask me', 6));
		typeIn(editor, ' ');
		compose(editor, '@Da');
		expect(triggerState(editor.state, '@').active).toBe(false);
		await settle();
		expect(triggerState(editor.state, '@')).toMatchObject({ active: true, query: 'Da' });
		expect(queries.at(-1)).toBe('Da');

		// Mid-word, or one the person closed before composing more: no menu.
		const word = track(htmlEditor('<p>and</p>', at()));
		word.commands.setTextSelection(4);
		compose(word, '@x');
		await settle();
		expect(triggerState(word.state, '@').active).toBe(false);
		const closed = track(htmlEditor('<p></p>', at()));
		closed.commands.setTextSelection(1);
		typeIn(closed, '@Da');
		closed.view.dom.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
		expect(triggerState(closed.state, '@').active).toBe(false);
		compose(closed, 've');
		await settle();
		expect(triggerState(closed.state, '@').active).toBe(false);
	});

	// The `@` picker's own search box takes the keyboard as it opens, so
	// what the person went on typing went there, and Escape or a click
	// outside threw it away: 'Ping @team about it' became 'Ping @'.
	it('closing the `@` picker with nothing picked puts what was typed in its box back after the `@`', () => {
		const at = () => [triggers([{ char: '@', onOpen: () => {}, onQuery: () => {}, onClose: () => {} }])];
		const editor = track(htmlEditor('<p>Ping</p>', at()));
		editor.commands.setTextSelection(posIn(editor, 'Ping', 4));
		typeIn(editor, ' @te');
		const open = triggerState(editor.state, '@');
		expect(open).toMatchObject({ active: true, query: 'te' });
		putBackQuery(editor.view, '@', 'team about it', 'te', { from: open.from, to: open.from + 1 });
		expect(editor.state.doc.textContent).toBe('Ping @team about it');
		expect(editor.state.selection.from).toBe(posIn(editor, 'Ping @team about it', 'Ping @team about it'.length));
		expect(triggerState(editor.state, '@').active).toBe(false);

		// Typed nothing more: the page is left as it is.
		const same = track(htmlEditor('<p>Ping</p>', at()));
		same.commands.setTextSelection(posIn(same, 'Ping', 4));
		typeIn(same, ' @te');
		const before = same.state.doc;
		putBackQuery(same.view, '@', 'te', 'te', { from: 0, to: 0 });
		expect(same.state.doc).toBe(before);

		// The `@` no longer open: after it, in place of what it held.
		const closed = track(htmlEditor('<p>Ping @te</p>', at()));
		const sign = posIn(closed, 'Ping @te', 5);
		putBackQuery(closed.view, '@', 'team', 'te', { from: sign, to: sign + 1 });
		expect(closed.state.doc.textContent).toBe('Ping @team');

		const picker = fs.readFileSync(path.resolve(HERE, '../components/RefPicker.svelte'), 'utf8');
		expect(picker).toMatch(/useEscapeKey\(\(\) => onClose\(query\), \(\) => true\);/);
		expect(picker).toMatch(/\(\) => \[pickerEl\],\s+\(\) => onClose\(query\),/);
		const page = fs.readFileSync(path.resolve(HERE, '../components/pages/DocumentEditor.svelte'), 'utf8');
		expect(page).toMatch(/putBackQuery\(editor\.view, "@", typed, pickerQuery, at\);/);
		expect(page).toMatch(/onClose=\{handlePickerClose\}/);
	});

	it('a pick with the `@` closed replaces the `@` alone, or nothing once it is gone', () => {
		const editor = track(htmlEditor('<p>Lunch with @</p>'));
		const at = posIn(editor, 'Lunch with @', 'Lunch with '.length);
		expect(pickedRange(editor.state, '@', { from: at, to: at + 1 })).toEqual({ from: at, to: at + 1 });
		expect(pickedRange(editor.state, '@', { from: at - 1, to: at })).toEqual({ from: at - 1, to: at - 1 });
	});

	it('is never opened by another device inserting the character', async () => {
		const { doc } = pageDoc('<p>Hello</p>');
		const other = new Y.Doc();
		Y.applyUpdate(other, Y.encodeStateAsUpdate(doc));
		other.on('update', (u: Uint8Array) => Y.applyUpdate(doc, u, 'wire'));
		const { editor } = bind(doc);
		await settle();
		const text = other.getXmlFragment(contract.fragment).get(0) as Y.XmlElement;
		(text.get(0) as Y.XmlText).insert(0, '/');
		expect(editor.state.doc.textContent).toBe('/Hello');
		expect(triggerState(editor.state, '/').active).toBe(false);
	});
});

describe('a link whose address does not decode', () => {
	// The contract takes any path, and `%` alone is no %-escape.
	const BAD = '<p>See <a href="/person/%">them</a>.</p>';

	it('is still a ref, read as written', () => {
		expect(parseRef('/person/%')).toEqual({ kind: 'person', id: '%' });
		expect(isRef('/person/%')).toBe(true);
		expect(parseRef('/person/person%5F1')).toEqual({ kind: 'person', id: 'person_1' });
	});

	it('opens in a page that holds it', () => {
		const { doc } = pageDoc(`<p>Keep this paragraph.</p>${BAD}`);
		const { editor, element } = bind(doc);
		expect(editor.getHTML()).toContain('href="/person/%"');
		expect(element.querySelector('.doc-ref-link')?.textContent).toBe('them');
	});

	it('another device writes it into an open page, and typing keeps it', async () => {
		const { doc } = pageDoc('<p>Keep this paragraph.</p><p>Second.</p>');
		const other = new Y.Doc();
		Y.applyUpdate(other, Y.encodeStateAsUpdate(doc));
		other.on('update', (u: Uint8Array) => Y.applyUpdate(doc, u, 'wire'));
		const { editor } = bind(doc);
		await settle();
		const writer = bind(other, { plugins: false }).editor;
		writer.commands.insertContentAt(writer.state.doc.content.size, BAD);
		await settle();
		expect(editor.getHTML()).toContain('href="/person/%"');

		editor.commands.insertContentAt(1, 'typed ');
		await settle();
		const stored = doc.getXmlFragment(contract.fragment).toString();
		expect(stored).toContain('typed Keep this paragraph.');
		expect(stored).toContain('/person/%');
	});
});

describe('a link the contract refuses, and a widget\'s own link', () => {
	// What reached a page past the server's check: a link mark written
	// straight into the tree.
	function withRefusedLink() {
		const editor = track(htmlEditor('<p>Before</p>', [links()]));
		const { schema } = editor.state;
		const link = schema.marks.link.create({ href: 'javascript:alert(document.domain)' });
		editor.view.dispatch(editor.state.tr.insert(editor.state.doc.content.size, schema.nodes.paragraph.create(null, schema.text('click me', [link]))));
		return editor;
	}

	it('is drawn with no address, and opens nothing', () => {
		const editor = withRefusedLink();
		expect(editor.getHTML()).not.toContain('javascript:');
		expect(editor.view.dom.innerHTML).not.toContain('javascript:');
		const open = vi.spyOn(window, 'open').mockImplementation(() => null);
		openLink('javascript:alert(document.domain)', 'click me');
		openLinkBeside('javascript:alert(document.domain)', 'click me');
		expect(open).not.toHaveBeenCalled();
		openLink('https://example.com/plan', 'plan');
		expect(open).toHaveBeenCalledOnce();
		open.mockRestore();
	});

	it("a click on a widget's own link is the widget's: its navigation is not cancelled", async () => {
		const { editor } = bind(pageDoc('<p>Report:</p><virtues-file src="/drive/df_1" data-name="report.pdf"></virtues-file>').doc);
		await settle();
		for (const editable of [true, false]) {
			editor.setEditable(editable);
			const widget = editor.view.dom.querySelector('.doc-widget')!;
			const card = document.createElement('a');
			card.className = 'doc-file-card';
			card.href = '/api/drive/files/df_1/download';
			card.target = '_blank';
			card.dataset.widgetControl = '';
			widget.append(card);
			const click = new MouseEvent('click', { bubbles: true, cancelable: true, button: 0 });
			// Read once the editor has seen the click, on its way up past it;
			// then cancelled here, so the test runner does not navigate.
			let cancelled: boolean | null = null;
			document.addEventListener(
				'click',
				(e) => {
					cancelled = e.defaultPrevented;
					e.preventDefault();
				},
				{ once: true },
			);
			card.dispatchEvent(click);
			expect(cancelled, `editable: ${editable}`).toBe(false);
			card.remove();
		}
	});
});

describe('find in page', () => {
	// As the CodeMirror editor's highlightSelectionMatches: the selected
	// words, case and all, echoed where else they occur.
	it('echoes the selected words where else they occur, and nothing for spaces, two blocks or a widget', () => {
		const editor = track(
			htmlEditor('<h2>Trip to Lisbon</h2><p>Flights to Lisbon, then lisbon trams.</p><p>Back from Lisbon.</p>', [
				find({ onOpen: () => {}, onClose: () => {} }),
			]),
		);
		const echoed = () => [...editor.view.dom.querySelectorAll('.doc-selection-match')].map((el) => el.textContent);
		expect(echoed()).toEqual([]);
		const from = posIn(editor, 'Lisbon');
		editor.commands.setTextSelection({ from, to: from + 'Lisbon'.length });
		expect(echoed()).toEqual(['Lisbon', 'Lisbon']);
		expect(selectionEchoes(editor.state).every((m) => m.from > from)).toBe(true);
		const space = posIn(editor, ' to Lisbon');
		editor.commands.setTextSelection({ from: space, to: space + 1 });
		expect(echoed()).toEqual([]);
		editor.commands.setTextSelection({ from, to: posIn(editor, 'Flights', 3) });
		expect(echoed()).toEqual([]);
		editor.commands.setTextSelection(from);
		expect(echoed()).toEqual([]);
	});

	it('counts matches, case ignored, and steps through them both ways', () => {
		const editor = track(htmlEditor('<p>Plan the plan.</p><h2>Planning</h2>', [find({ onOpen: () => {}, onClose: () => {} })]));
		expect(findMatches(editor.state.doc, 'plan')).toHaveLength(3);
		setFindQuery(editor.view, 'plan');
		expect(findState(editor.state)).toMatchObject({ query: 'plan', current: 0 });
		findNext(editor.view);
		expect(findState(editor.state).current).toBe(1);
		findPrevious(editor.view);
		findPrevious(editor.view);
		expect(findState(editor.state).current).toBe(2);
		const m = findState(editor.state).matches[2];
		expect(editor.state.doc.textBetween(m.from, m.to)).toBe('Plan');
		expect(editor.state.selection.from).toBe(m.from);
		clearFind(editor.view);
		expect(findState(editor.state).matches).toEqual([]);
	});

	it('keeps counting as the page changes', () => {
		const editor = track(htmlEditor('<p>one</p>', [find({ onOpen: () => {}, onClose: () => {} })]));
		setFindQuery(editor.view, 'one');
		editor.commands.insertContentAt(editor.state.doc.content.size, '<p>one more</p>');
		expect(findState(editor.state).matches).toHaveLength(2);
	});
});

describe('the outline, the counts and the registry', () => {
	it('lists h1 to h3 by block id, and scrolls the scroller to one', async () => {
		const { doc } = pageDoc(FIXTURE);
		const { editor } = bind(doc);
		const headings = treeHeadings(editor.state.doc);
		expect(headings.map((h) => [h.level, h.text])).toEqual([
			[2, 'Trip to Lisbon'],
			[3, 'Costs'],
		]);
		expect(headings.every((h) => typeof h.id === 'string' && h.id.length === 8)).toBe(true);

		const nav = treeOutlineNav(editor);
		const scrollTo = vi.fn();
		nav.scroller.scrollTo = scrollTo as unknown as typeof nav.scroller.scrollTo;
		nav.scrollTo(headings[1]);
		expect(scrollTo).toHaveBeenCalledTimes(1);
		expect(nav.topOf(headings[1])).not.toBeNull();
	});

	// A page that opens short does not overflow its scroller yet; it is the
	// scroller all the same once it grows, and the nav is bound at mount.
	it('binds the outline to the page\'s scroller while the page is still short', async () => {
		const { doc } = pageDoc('<h2>Plan</h2><p>Short.</p>');
		const scroller = document.createElement('div');
		scroller.style.overflowY = 'auto';
		scroller.style.height = '400px';
		document.body.append(scroller);
		const element = document.createElement('div');
		scroller.append(element);
		const provider = fakeProvider(doc);
		cleanups.push(provider.destroy);
		const editor = track(
			createPageEditor({
				element,
				doc: { ydoc: doc, provider },
				editable: true,
				placeholder: 'Write',
				user: { name: 'Computer', tint: 'orange' },
				plugins: [],
			}),
		);
		await settle(20);
		expect(scroller.scrollHeight > scroller.clientHeight).toBe(false);
		const nav = treeOutlineNav(editor);
		expect(nav.scroller).toBe(scroller);
		expect(scrollerOf(editor.view.dom)).toBe(scroller);
	});

	it('counts words, characters, links and media', () => {
		const editor = track(
			htmlEditor(
				'<p>Lunch with <virtues-mention to="/person/person_1" label="Nick"></virtues-mention> at <a href="https://example.com">the <strong>harbour</strong></a>.</p><img src="/api/media/a.png"><virtues-applet ref="sleep-week"></virtues-applet>',
			),
		);
		// "Lunch with Nick at the harbour."
		expect(treeStats(editor.state.doc)).toEqual({ wordCount: 6, charCount: 31, linkCount: 2, mediaCount: 2 });
	});

	it('finds an open editor by page until it closes', () => {
		const editor = track(htmlEditor('<p>x</p>'));
		registerTreeEditor('page_1', editor);
		expect(getTreeEditor('page_1')).toBe(editor);
		unregisterTreeEditor('page_1', editor);
		expect(getTreeEditor('page_1')).toBeNull();
	});
});

describe('node views', () => {
	it('a mention is drawn by MentionNode with what the page wrote, and MentionNode draws Ref', async () => {
		const { doc } = pageDoc(FIXTURE);
		bind(doc);
		const mentions = mounted.filter((m) => m.name === 'MentionNode').map((m) => m.props.view.attrs);
		expect(mentions).toEqual([
			expect.objectContaining({ to: '/person/person_1', label: 'Nick' }),
			expect.objectContaining({ to: '/person/person_2', label: 'David Okafor' }),
		]);
		const source = fs.readFileSync(path.join(COMPONENTS, 'MentionNode.svelte'), 'utf8');
		expect(source).toMatch(/<Ref displayName=\{label\} url=\{to\}/);
	});

	it('an applet is mounted only once it scrolls into view, and AppletNode draws FaceFrame', async () => {
		const observers: { cb: IntersectionObserverCallback; el?: Element }[] = [];
		vi.stubGlobal(
			'IntersectionObserver',
			class {
				private entry: { cb: IntersectionObserverCallback; el?: Element };
				constructor(cb: IntersectionObserverCallback) {
					this.entry = { cb };
					observers.push(this.entry);
				}
				observe(el: Element) {
					this.entry.el = el;
				}
				disconnect() {}
			},
		);
		try {
			const { doc } = pageDoc(FIXTURE);
			bind(doc);
			expect(mounted.some((m) => m.name === 'AppletNode')).toBe(false);
			const applet = observers.find((o) => o.el?.classList.contains('doc-widget-applet'));
			const placeholder = applet?.el?.querySelector('.doc-applet-placeholder');
			expect(placeholder?.textContent).toBe('sleep-week');
			// Named once its name is read, as the mounted applet is named, and
			// as it prints if it never mounts.
			await settle();
			expect(placeholder?.textContent).toBe('Sleep this week');
			applet!.cb([{ isIntersecting: true } as IntersectionObserverEntry], {} as IntersectionObserver);
			const view = mounted.find((m) => m.name === 'AppletNode')?.props.view;
			expect(view?.attrs).toMatchObject({ ref: 'sleep-week', height: 200 });
		} finally {
			vi.unstubAllGlobals();
		}
		const source = fs.readFileSync(path.join(COMPONENTS, 'AppletNode.svelte'), 'utf8');
		expect(source).toMatch(/<FaceFrame appletId=\{ref\}/);
	});

	it('media blocks go to MediaNode by type', () => {
		const editor = track(htmlEditor('<p>x</p>'));
		void editor;
		const { doc } = pageDoc('<audio src="/drive/df_3" data-name="memo.m4a"></audio><virtues-file src="/drive/df_5" data-name="notes.pdf"></virtues-file>');
		bind(doc);
		expect(mounted.filter((m) => m.name === 'MediaNode').map((m) => m.props.view.type)).toEqual(['audio', 'file']);
	});
});

describe('proposals', () => {
	const ONE = '<p>The <virtues-del proposal="p1">old</virtues-del><virtues-ins proposal="p1">new</virtues-ins> plan.</p>';
	const THREE =
		'<p>Keep <virtues-del proposal="p2">this end</virtues-del></p><p><virtues-del proposal="p2">all of this</virtues-del></p><p><virtues-del proposal="p2">and this start</virtues-del><virtues-ins proposal="p2">the new middle</virtues-ins> stays.</p>';

	it('accept takes what it puts in and drops what it takes out, in one block', () => {
		const editor = track(htmlEditor(ONE));
		expect(proposalAt(editor.state, posIn(editor, 'new', 1))).toEqual({ id: 'p1', deletes: true, inserts: true });
		expect(acceptProposal(editor, 'p1')).toBe(true);
		expect(editor.getHTML()).toBe('<p>The new plan.</p>');
		expect(proposalIds(editor.state)).toEqual([]);
	});

	it('reject keeps what it would take out and drops what it puts in', () => {
		const editor = track(htmlEditor(ONE));
		rejectProposal(editor, 'p1');
		expect(editor.getHTML()).toBe('<p>The old plan.</p>');
	});

	it('across three blocks, a block it empties goes whole', () => {
		const accepted = track(htmlEditor(THREE));
		acceptProposal(accepted, 'p2');
		expect(accepted.getHTML()).toBe('<p>Keep </p><p>the new middle stays.</p>');
		const rejected = track(htmlEditor(THREE));
		rejectProposal(rejected, 'p2');
		expect(rejected.getHTML()).toBe('<p>Keep this end</p><p>all of this</p><p>and this start stays.</p>');
	});

	it('all at once, as one undo step', () => {
		const editor = track(htmlEditor(`${ONE}<p><virtues-ins proposal="p3">Added.</virtues-ins></p>`));
		expect(proposalIds(editor.state)).toEqual(['p1', 'p3']);
		acceptAllProposals(editor);
		expect(editor.getHTML()).toBe('<p>The new plan.</p><p>Added.</p>');
	});

	it('a proposal copied and pasted is a second one: settling the first leaves the copy as it was', async () => {
		const { editor } = bind(pageDoc('<p>Keep <virtues-del proposal="p1">old</virtues-del><virtues-ins proposal="p1">new</virtues-ins> end</p><p>Other</p>').doc);
		await settle();
		const first = editor.state.doc.child(0);
		const { dom } = editor.view.serializeForClipboard(editor.state.doc.slice(1, 1 + first.content.size));
		expect(dom.innerHTML).toContain('proposal="p1"');
		editor.commands.setTextSelection(posIn(editor, 'Other', 'Other'.length));
		editor.view.pasteHTML(dom.innerHTML);
		const ids = proposalIds(editor.state);
		expect(ids).toHaveLength(2);
		expect(ids[0]).toBe('p1');
		expect(acceptProposal(editor, 'p1')).toBe(true);
		expect(editor.state.doc.child(0).textContent).toBe('Keep new end');
		expect(editor.state.doc.child(1).textContent).toBe('OtherKeep oldnew end');
		expect(proposalIds(editor.state)).toEqual([ids[1]]);
		expect(rejectProposal(editor, ids[1])).toBe(true);
		expect(editor.state.doc.child(1).textContent).toBe('OtherKeep old end');
	});

	// Typed or pasted in the middle of words a proposal would delete, the
	// person's words took the proposal's marks (a paste's under a new id,
	// a proposal nobody made), and Accept deleted them.
	it('words typed or pasted inside a proposal are the person’s own, whatever settles it', async () => {
		const PAGE = '<p>Keep <virtues-del proposal="p1">old words</virtues-del><virtues-ins proposal="p1">new</virtues-ins> end</p>';
		for (const [write, settleIt, left] of [
			[(e: Editor) => typeIn(e, 'MINE'), (e: Editor) => acceptProposal(e, 'p1'), 'Keep MINEnew end'],
			[(e: Editor) => typeIn(e, 'MINE'), (e: Editor) => rejectProposal(e, 'p1'), 'Keep oldMINE words end'],
			[(e: Editor) => e.view.pasteText('MINE'), (e: Editor) => acceptAllProposals(e), 'Keep MINEnew end'],
			[(e: Editor) => e.view.pasteText('MINE'), (e: Editor) => rejectProposal(e, 'p1'), 'Keep oldMINE words end'],
		] as const) {
			const { editor } = bind(pageDoc(PAGE).doc);
			await settle();
			editor.commands.setTextSelection(posIn(editor, 'old words', 3));
			write(editor);
			expect(editor.getHTML()).toContain('<virtues-del proposal="p1">old</virtues-del>MINE<virtues-del proposal="p1"> words</virtues-del>');
			expect(proposalIds(editor.state)).toEqual(['p1']);
			expect(settleIt(editor)).toBe(true);
			expect(editor.state.doc.child(0).textContent).toBe(left);
		}
		// Bold turned on in one is still bold, and still the person's.
		const { editor } = bind(pageDoc(PAGE).doc);
		await settle();
		editor.commands.setTextSelection(posIn(editor, 'old words', 3));
		editor.commands.toggleBold();
		typeIn(editor, 'MINE');
		expect(editor.getHTML()).toContain('<virtues-del proposal="p1">old</virtues-del><strong>MINE</strong><virtues-del proposal="p1"> words</virtues-del>');
	});
});

describe('paste', () => {
	function clipboard(data: Record<string, string>, files: File[] = []) {
		return {
			clipboardData: {
				getData: (type: string) => data[type] ?? '',
				files,
			},
			preventDefault() {},
		} as unknown as ClipboardEvent;
	}

	function paste(editor: Editor, event: ClipboardEvent): boolean {
		return !!editor.view.someProp('handlePaste', (f) => f(editor.view, event, editor.state.doc.slice(0, 0)));
	}

	it('converts markdown text through the server and puts it where the paste was', async () => {
		const convert = vi.fn(async () => ({ html: '<h2>Plan</h2><ul><li><p>one</p></li></ul>' }));
		const editor = track(htmlEditor('<p>Before</p><p></p>', [pasteHandling(convert)]));
		editor.commands.setTextSelection(editor.state.doc.content.size - 1);
		expect(paste(editor, clipboard({ 'text/plain': '## Plan\n\n- one' }))).toBe(true);
		// Something typed above while the server answers moves the landing spot.
		editor.commands.insertContentAt(1, 'Typed ');
		await settle();
		expect(convert).toHaveBeenCalledWith('## Plan\n\n- one');
		expect(editor.getHTML()).toBe('<p>Typed Before</p><h2>Plan</h2><ul><li><p>one</p></li></ul>');
	});

	it('pastes the text as it is when the server cannot convert it', async () => {
		const editor = track(htmlEditor('<p></p>', [pasteHandling(async () => Promise.reject(new Error('offline')))]));
		editor.commands.setTextSelection(1);
		paste(editor, clipboard({ 'text/plain': '- **bold** text' }));
		await settle();
		expect(editor.getHTML()).toBe('<p>- **bold** text</p>');
	});

	// A line of code holds `==`, `**` and `](` often enough, and the
	// converter reads `<Login/>` in it as a tag: only a block's sign makes
	// pasted text markdown, and the words of a line are the editor's paste
	// rules' to format.
	it('pastes code-like text as it is, never through the converter', () => {
		const convert = vi.fn(async () => ({ html: '<p>lost</p>' }));
		const editor = track(htmlEditor('<p></p>', [pasteHandling(convert)]));
		editor.commands.setTextSelection(1);
		for (const text of [
			'if (user == null) return <Login/>;',
			'Press <Enter> to confirm, then check that a == b',
			'if (total == 2*3*4) return;',
			'C:\\Users\\nick\\new_folder\\*.txt == 3 files',
			'Draft {--old--} == fine',
			'#include <stdio.h>\nint main() { return a == b; }',
			'let v = vec![1](x) == 2',
			'Run **this** first',
		]) {
			expect(looksLikeMarkdown(text), text).toBe(false);
			expect(paste(editor, clipboard({ 'text/plain': text })), text).toBe(false);
		}
		expect(convert).not.toHaveBeenCalled();
		for (const text of ['## Plan', '- one', '1. one', '> quoted', '```js\nx\n```', '| a | b |', '  - nested']) {
			expect(looksLikeMarkdown(text), text).toBe(true);
		}
	});

	/** Paste `data` through the browser's paste event, as a person does: paste rules run on it. */
	function pasteEvent(editor: Editor, data: Record<string, string>) {
		const transfer = new DataTransfer();
		for (const [type, value] of Object.entries(data)) transfer.setData(type, value);
		editor.view.dom.dispatchEvent(new ClipboardEvent('paste', { clipboardData: transfer, bubbles: true, cancelable: true }));
	}

	// The words of a line are formatted as CommonMark reads them, as the
	// server's converter does: a `*` or `==` with a space inside it is an
	// operator. Pasted HTML carries its own formatting, and inline code holds
	// its text as written.
	it("formats a pasted line's words as markdown reads them, and leaves operators, inline code and HTML's text as they are", () => {
		const editor = track(htmlEditor('<p></p>', [pasteHandling(async () => ({ html: '' })), mediaUploads(async () => ({ url: '/api/media/x', filename: 'x', mime_type: null }))]));
		const landed = (data: Record<string, string>, page = '<p></p>', at = 1) => {
			editor.commands.setContent(page);
			editor.commands.setTextSelection(at);
			pasteEvent(editor, data);
			return editor.getHTML();
		};
		for (const text of [
			'SELECT * FROM orders WHERE total = 2 * price',
			'if a == b or c == d:',
			'x = a ** b ** c',
			'rm ~~ and ~~ more',
			'the snake_case_name and an other_name_',
			'Keep `a == b` and `x **y** z` as code',
		]) {
			expect(landed({ 'text/plain': text }).replace(/<\/?code>/g, '`'), text).toBe(`<p>${text}</p>`);
		}
		expect(landed({ 'text/plain': 'Run **this** first, *then* ==that== and ~~not~~ `code`' })).toBe(
			'<p>Run <strong>this</strong> first, <em>then</em> <mark>that</mark> and <s>not</s> <code>code</code></p>',
		);
		expect(landed({ 'text/html': '<code>a == b || c == d</code>', 'text/plain': 'a == b || c == d' })).toBe(
			'<p><code>a == b || c == d</code></p>',
		);
		expect(landed({ 'text/html': '<p>Run <code>a ** b ** c</code> now</p>', 'text/plain': 'Run a ** b ** c now' })).toBe(
			'<p>Run <code>a ** b ** c</code> now</p>',
		);
		expect(landed({ 'text/html': '<p>Say **hi** and ==there==</p>', 'text/plain': 'Say **hi** and ==there==' })).toBe(
			'<p>Say **hi** and ==there==</p>',
		);
		// Plain text pasted into inline code takes the code mark: it stays as written.
		expect(landed({ 'text/plain': 'x ==y== z' }, '<p><code>ab</code></p>', 2)).toBe('<p><code>ax ==y== zb</code></p>');
		// Links too: a markdown link or an address in plain text is linked;
		// pasted HTML carries its own links, and inline code says what it says.
		expect(landed({ 'text/plain': 'See [docs](https://example.com) and https://example.com/notes' })).toBe(
			'<p>See <a href="https://example.com">docs</a> and <a href="https://example.com/notes">https://example.com/notes</a></p>',
		);
		expect(
			landed({
				'text/html': '<p>Write links as <code>[text](https://example.com)</code> in markdown</p>',
				'text/plain': 'Write links as [text](https://example.com) in markdown',
			}),
		).toBe('<p>Write links as <code>[text](https://example.com)</code> in markdown</p>');
		expect(landed({ 'text/plain': '[text](https://example.com)' }, '<p><code>ab</code></p>', 2)).toBe(
			'<p><code>a[text](https://example.com)b</code></p>',
		);
		expect(
			landed({ 'text/html': '<p>Say **hi** and [docs](https://example.com)</p>', 'text/plain': 'Say **hi** and [docs](https://example.com)' }),
		).toBe('<p>Say **hi** and [docs](https://example.com)</p>');
		expect(landed({ 'text/plain': 'Keep `[a](https://example.com)` as code' })).toBe(
			'<p>Keep <code>[a](https://example.com)</code> as code</p>',
		);
	});

	it("sends a paste to the server's converter as a paste, which keeps its text", async () => {
		vi.mocked(convertPastedMarkdown).mockClear();
		const editor = track(htmlEditor('<p></p>', [pasteHandling()]));
		editor.commands.setTextSelection(1);
		expect(paste(editor, clipboard({ 'text/plain': '- remember <placeholder> here' }))).toBe(true);
		await settle();
		expect(convertPastedMarkdown).toHaveBeenCalledWith('- remember <placeholder> here');
	});

	/** VS Code's clipboard HTML: its syntax colours as styled spans, a token's bold included. */
	const VSCODE = (lines: string[]) =>
		'<meta charset="utf-8"><div style="color: #d4d4d4;background-color: #1e1e1e;font-family: Menlo, monospace;font-weight: normal;font-size: 12px;line-height: 18px;white-space: pre;">'
		+ lines.map((l) => (l ? `<div><span style="color: #569cd6;font-weight: bold;">${l}</span></div>` : '<br>')).join('')
		+ '</div>';

	it("reads a code editor's HTML as the text it shows: markdown converted, code without its colours' bold", async () => {
		expect(isCodeEditorHtml(VSCODE(['## Plan']))).toBe(true);
		expect(isCodeEditorHtml('<div style="white-space: pre"><b>Bold</b></div>')).toBe(false);
		expect(isCodeEditorHtml('<p><span style="font-weight:700">Bold</span></p>')).toBe(false);

		const convert = vi.fn(async () => ({ html: '<h2>Plan</h2><ul><li><p>one</p></li></ul>' }));
		const editor = track(htmlEditor('<p></p>', [pasteHandling(convert)]));
		editor.commands.setTextSelection(1);
		expect(paste(editor, clipboard({ 'text/plain': '## Plan\n\n- one', 'text/html': VSCODE(['## Plan', '', '- one']) }))).toBe(true);
		await settle();
		expect(convert).toHaveBeenCalledWith('## Plan\n\n- one');
		expect(editor.getHTML()).toBe('<h2>Plan</h2><ul><li><p>one</p></li></ul>');

		const code = track(htmlEditor('<p></p>', [pasteHandling(convert)]));
		code.commands.setTextSelection(1);
		expect(paste(code, clipboard({ 'text/plain': 'const a = 1;\nconst b = 2;', 'text/html': VSCODE(['const a = 1;', 'const b = 2;']) }))).toBe(true);
		expect(code.getHTML()).toBe('<p>const a = 1;</p><p>const b = 2;</p>');
		expect(convert).toHaveBeenCalledTimes(1);
	});

	it("leaves VS Code's own data to the code block paste, unless it says markdown", async () => {
		const convert = vi.fn(async () => ({ html: '<h2>Plan</h2>' }));
		const editor = track(htmlEditor('<p></p>', [pasteHandling(convert)]));
		editor.commands.setTextSelection(1);
		const html = VSCODE(['# comment']);
		const python = JSON.stringify({ version: 1, mode: 'python' });
		const markdown = JSON.stringify({ version: 1, mode: 'markdown' });
		const handled = (data: Record<string, string>) =>
			!!editor.view.someProp('handlePaste', (f) => (f === undefined ? false : f(editor.view, clipboard(data), editor.state.doc.slice(0, 0))));
		// Tiptap's code block paste makes the Python a code block.
		expect(handled({ 'text/plain': '# comment', 'text/html': html, 'vscode-editor-data': python })).toBe(true);
		expect(convert).not.toHaveBeenCalled();
		expect(editor.state.doc.firstChild?.type.name).toBe('codeBlock');

		const md = track(htmlEditor('<p></p>', [pasteHandling(convert)]));
		md.commands.setTextSelection(1);
		expect(paste(md, clipboard({ 'text/plain': '## Plan', 'text/html': VSCODE(['## Plan']), 'vscode-editor-data': markdown }))).toBe(true);
		await settle();
		expect(md.getHTML()).toBe('<h2>Plan</h2>');
	});

	it('leaves plain words and HTML to the editor, and pastes raw into a code block', async () => {
		const convert = vi.fn(async () => ({ html: '' }));
		const editor = track(htmlEditor('<p></p><pre><code>x</code></pre>', [pasteHandling(convert)]));
		editor.commands.setTextSelection(1);
		expect(paste(editor, clipboard({ 'text/plain': 'just words' }))).toBe(false);
		expect(paste(editor, clipboard({ 'text/plain': '**b**', 'text/html': '<b>b</b>' }))).toBe(false);
		editor.commands.setTextSelection(posIn(editor, 'x', 1));
		expect(paste(editor, clipboard({ 'text/plain': '# not a heading' }))).toBe(true);
		expect(convert).not.toHaveBeenCalled();
		expect(editor.state.doc.lastChild?.textContent).toBe('x# not a heading');
	});

	// Windows puts CRLF line ends on its clipboard; the server's own writers
	// never put a CR in code, as ProseMirror's code paste never does.
	it('pastes Windows line ends into a code block as line ends', () => {
		const editor = track(htmlEditor('<pre data-language="python"><code>x = 1</code></pre>', [pasteHandling(async () => ({ html: '' }))]));
		editor.commands.setTextSelection(posIn(editor, 'x = 1', 5));
		expect(paste(editor, clipboard({ 'text/plain': '\r\ny = 2\r\nz = 3\rw = 4' }))).toBe(true);
		expect(editor.state.doc.firstChild?.textContent).toBe('x = 1\ny = 2\nz = 3\nw = 4');
	});

	/** Drop `data` at `pos`, as the browser delivers a drop from outside the page. */
	function dropAt(editor: Editor, pos: number, data: Record<string, string>) {
		const transfer = new DataTransfer();
		for (const [type, value] of Object.entries(data)) transfer.setData(type, value);
		editor.view.posAtCoords = () => ({ pos, inside: -1 });
		const event = new DragEvent('drop', { bubbles: true, cancelable: true, clientX: 10, clientY: 10 });
		Object.defineProperty(event, 'dataTransfer', { value: transfer });
		editor.view.dom.dispatchEvent(event);
	}

	// A drop is read as the same data pasted: markdown text dropped stayed
	// raw, and HTML dropped had the plain-text rules run over its words.
	it('reads dropped text as it reads the same text pasted: markdown converted, HTML’s words as written', async () => {
		const convert = vi.fn(async () => ({ html: '<h2>Plan</h2><ul><li><p>one</p></li></ul>' }));
		const editor = track(htmlEditor('<p>Start</p>', [pasteHandling(convert)]));
		dropAt(editor, posIn(editor, 'Start', 5), { 'text/plain': '## Plan\n\n- one' });
		await settle();
		expect(convert).toHaveBeenCalledWith('## Plan\n\n- one');
		expect(editor.getHTML()).toBe('<p>Start</p><h2>Plan</h2><ul><li><p>one</p></li></ul>');

		const html = track(htmlEditor('<p>Start</p>', [pasteHandling(convert)]));
		dropAt(html, posIn(html, 'Start', 5), { 'text/html': '<p>Say **hi** and ==there==</p>', 'text/plain': 'Say **hi** and ==there==' });
		await settle();
		expect(html.getHTML()).toBe('<p>StartSay **hi** and ==there==</p>');

		// Plain words dropped are formatted as plain words pasted are.
		const words = track(htmlEditor('<p>Start</p>', [pasteHandling(convert)]));
		dropAt(words, posIn(words, 'Start', 5), { 'text/plain': ' say **hi**' });
		await settle();
		expect(words.getHTML()).toBe('<p>Start say <strong>hi</strong></p>');
	});

	it("reads Google Docs' and Word's inline styles as marks", () => {
		const editor = track(htmlEditor('<p></p>', [pasteHandling()]));
		editor.commands.setTextSelection(1);
		editor.view.pasteHTML('<b style="font-weight:normal" id="docs-internal-guid-1"><span style="font-weight:700">Bold</span> <span style="font-style:italic">slanted</span> plain</b>');
		expect(editor.getHTML()).toBe('<p><strong>Bold</strong> <em>slanted</em> plain</p>');
	});

	// Word writes a list as paragraphs, each with its bullet or number as
	// text: pasted as they came they were paragraphs starting with `·` or
	// `1.` and a run of no-break spaces, and the markdown read them so.
	it("pastes Word's lists as lists, bulleted, numbered and nested, without their markers", () => {
		const marker = (mark: string, spaces: number) =>
			`<span style='font-family:Symbol'><span style='mso-list:Ignore'>${mark}<span style='font:7.0pt "Times New Roman"'>${'&nbsp;'.repeat(spaces)} </span></span></span>`;
		const item = (list: string, level: number, mark: string, words: string, spaces = 7) =>
			`<p class=MsoListParagraphCxSpMiddle style='margin-left:${level * 0.5}in;text-indent:-.25in;mso-list:${list} level${level} lfo1'><![if !supportLists]>${marker(mark, spaces)}<![endif]>${words}<o:p></o:p></p>`;
		const windows = [
			'<html xmlns:o="urn:schemas-microsoft-com:office:office"><head><meta charset="utf-8"></head><body lang=EN-US><!--StartFragment-->',
			item('l0', 1, '·', 'First <b>item</b>'),
			item('l0', 2, 'o', 'Nested item', 3),
			item('l0', 1, '·', 'Second item'),
			"<p class=MsoNormal>Between<o:p></o:p></p>",
			item('l1', 1, '3.', 'Numbered', 5),
			item('l1', 1, '4.', 'Next', 5),
			'<!--EndFragment--></body></html>',
		].join('\n');
		const expected =
			'<ul><li><p>First <strong>item</strong></p><ul><li><p>Nested item</p></li></ul></li><li><p>Second item</p></li></ul><p>Between</p><ol start="3"><li><p>Numbered</p></li><li><p>Next</p></li></ol>';
		const editor = track(htmlEditor('<p></p>', [pasteHandling()]));
		editor.commands.setTextSelection(1);
		editor.view.pasteHTML(windows);
		expect(editor.getHTML()).toBe(expected);
		expect(editor.getText()).not.toMatch(/[·\u00a0]/);

		// Word for Mac and Outlook write the conditionals as real comments.
		const mac = windows.replaceAll('<![if !supportLists]>', '<!--[if !supportLists]-->').replaceAll('<![endif]>', '<!--[endif]-->');
		const other = track(htmlEditor('<p></p>', [pasteHandling()]));
		other.commands.setTextSelection(1);
		other.view.pasteHTML(mac);
		expect(other.getHTML()).toBe(expected);

		// Anything else is read as it came.
		const plain = '<p style="mso-margin-top-alt:auto">Not a list</p>';
		expect(wordLists(plain)).toBe(plain);
	});

	it('a paste whose spot is gone lands at the caret, and says so', async () => {
		vi.mocked(toast).mockClear();
		let answer: (v: { html: string }) => void = () => {};
		const editor = track(htmlEditor('<p>Before</p><p>Draft line.</p><p></p>', [pasteHandling(async () => ({ html: '' }))]));
		const done = pasteMarkdown(editor.view, '- one', posIn(editor, 'Draft line.', 5), () => new Promise((r) => (answer = r)));
		const from = posIn(editor, 'Draft line.') - 1;
		editor.commands.deleteRange({ from, to: from + editor.state.doc.child(1).nodeSize });
		editor.commands.setTextSelection(editor.state.doc.content.size - 1);
		answer({ html: '<ul><li><p>one</p></li></ul>' });
		await done;
		expect(editor.getHTML()).toBe('<p>Before</p><ul><li><p>one</p></li></ul>');
		expect(toast).toHaveBeenCalledOnce();
	});

	/** A converter whose answers the test gives, one per call, in the order asked. */
	function heldConverter() {
		const answers: ((v: { html: string }) => void)[] = [];
		return {
			convert: () => new Promise<{ html: string }>((r) => answers.push(r)),
			answer: (i: number, html: string) => answers[i]({ html }),
		};
	}

	for (const order of [
		[0, 1],
		[1, 0],
	]) {
		it(`two pastes waiting at one caret land in the order made, the server answering ${order.join(' then ')}`, async () => {
			for (const shape of ['plain', 'shared'] as const) {
				const held = heldConverter();
				const editor =
					shape === 'plain'
						? track(htmlEditor('<p>Start</p><p>End</p>', [pasteHandling(held.convert)]))
						: bind(pageDoc('<p>Start</p><p>End</p>').doc).editor;
				editor.commands.setTextSelection(posIn(editor, 'Start', 5));
				const first = pasteMarkdown(editor.view, '## First\n\n- a', posIn(editor, 'Start', 5), held.convert);
				const second = pasteMarkdown(editor.view, '## Second\n\n- b', posIn(editor, 'Start', 5), held.convert);
				const html = ['<h2>First</h2><ul><li><p>a</p></li></ul>', '<h2>Second</h2><ul><li><p>b</p></li></ul>'];
				for (const i of order) {
					held.answer(i, html[i]);
					await settle();
				}
				await Promise.all([first, second]);
				const out: string[] = [];
				editor.state.doc.forEach((n) => out.push(`${n.type.name}:${n.textContent}`));
				expect(out, shape).toEqual(['paragraph:Start', 'heading:First', 'bulletList:a', 'heading:Second', 'bulletList:b', 'paragraph:End']);
			}
		});
	}

	it('two runs of words pasted at one caret join the text in the order made', async () => {
		const held = heldConverter();
		const editor = track(htmlEditor('<p>Start</p>', [pasteHandling(held.convert)]));
		const one = pasteMarkdown(editor.view, '**one**', posIn(editor, 'Start', 5), held.convert);
		const two = pasteMarkdown(editor.view, '**two**', posIn(editor, 'Start', 5), held.convert);
		held.answer(0, '<p>ONE</p>');
		await settle();
		held.answer(1, '<p>TWO</p>');
		await Promise.all([one, two]);
		expect(editor.getHTML()).toBe('<p>StartONETWO</p>');
	});

	it('a pending paste is not a document node', async () => {
		let answer: (v: { html: string }) => void = () => {};
		const editor = track(htmlEditor('<p></p>', [pasteHandling(() => new Promise((r) => (answer = r)))]));
		editor.commands.setTextSelection(1);
		const done = pasteMarkdown(editor.view, '- one', 1, () => new Promise((r) => (answer = r)));
		expect(editor.getHTML()).toBe('<p></p>');
		answer({ html: '<ul><li><p>one</p></li></ul>' });
		await done;
		expect(editor.getHTML()).toBe('<ul><li><p>one</p></li></ul>');
	});

	describe('blocks arrive whole, an inline run joins the text', () => {
		const HEADING_AND_LIST = '<h2>Pasted heading</h2><ul><li><p><strong>one</strong></p></li><li><p>two</p></li></ul>';
		const LIST = '<ul><li><p>one</p></li><li><p>two</p></li></ul>';

		/**
		 * Paste markdown the server converts to `html`, the caret `offset` into
		 * the block holding `text`, or at the position `text` is.
		 */
		async function pasteInto(page: string, text: string | number, offset: number, html: string) {
			const editor = track(htmlEditor(page, [pasteHandling(async () => ({ html }))]));
			const at = typeof text === 'number' ? text : posIn(editor, text, offset);
			editor.commands.setTextSelection(at);
			await pasteMarkdown(editor.view, 'markdown', at, async () => ({ html }));
			return editor;
		}

		it('at the end of a paragraph, after it', async () => {
			const editor = await pasteInto('<p>Before</p><p>After</p>', 'Before', 6, HEADING_AND_LIST);
			expect(editor.getHTML()).toBe(`<p>Before</p>${HEADING_AND_LIST}<p>After</p>`);
			// The caret ends after what was pasted, as a paste leaves it.
			expect(editor.state.selection.from).toBe(posIn(editor, 'two', 3));
		});

		it('in the middle of a paragraph, between its halves', async () => {
			const editor = await pasteInto('<p>Before after</p>', 'Before', 7, HEADING_AND_LIST);
			expect(editor.getHTML()).toBe(`<p>Before </p>${HEADING_AND_LIST}<p>after</p>`);
		});

		it('at the start of a paragraph, before it', async () => {
			const editor = await pasteInto('<p>Before</p>', 'Before', 0, HEADING_AND_LIST);
			expect(editor.getHTML()).toBe(`${HEADING_AND_LIST}<p>Before</p>`);
		});

		it('in an empty paragraph, in its place', async () => {
			// 9: inside the empty paragraph, after `<p>Before</p>` (8).
			const editor = await pasteInto('<p>Before</p><p></p><p>After</p>', 9, 0, HEADING_AND_LIST);
			expect(editor.getHTML()).toBe(`<p>Before</p>${HEADING_AND_LIST}<p>After</p>`);
		});

		it('at the end of a list item inside a callout, after the list, still in the callout', async () => {
			const page = '<aside data-tone="tip"><ul><li><p>first</p></li><li><p>inside</p></li></ul></aside>';
			const editor = await pasteInto(page, 'inside', 6, HEADING_AND_LIST);
			expect(editor.getHTML()).toBe(
				`<aside data-tone="tip"><ul><li><p>first</p></li><li><p>inside</p></li></ul>${HEADING_AND_LIST}</aside>`,
			);
		});

		it('in the middle of a list, which it splits; a pasted list joins the list it meets', async () => {
			const page = '<ul><li><p>alpha</p></li><li><p>beta</p></li><li><p>gamma</p></li></ul>';
			const split = await pasteInto(page, 'beta', 4, HEADING_AND_LIST);
			expect(split.getHTML()).toBe(
				'<ul><li><p>alpha</p></li><li><p>beta</p></li></ul><h2>Pasted heading</h2>'
					+ '<ul><li><p><strong>one</strong></p></li><li><p>two</p></li><li><p>gamma</p></li></ul>',
			);
			const joined = await pasteInto(page, 'beta', 4, LIST);
			expect(joined.getHTML()).toBe(
				'<ul><li><p>alpha</p></li><li><p>beta</p></li><li><p>one</p></li><li><p>two</p></li><li><p>gamma</p></li></ul>',
			);
		});

		// The second half of a numbered list a paste split copied its start:
		// the items after the paste counted from it again, a second 3 written
		// to the page and its markdown.
		it('in the middle of a numbered list, whose second half goes on counting', async () => {
			const page = '<ol start="3"><li><p>a</p></li><li><p>b</p></li><li><p>c</p></li></ol>';
			const after = await pasteInto(page, 'a', 1, '<h2>Mid</h2>');
			expect(after.getHTML()).toBe(
				'<ol start="3"><li><p>a</p></li></ol><h2>Mid</h2><ol start="4"><li><p>b</p></li><li><p>c</p></li></ol>',
			);
			const inside = await pasteInto('<ol><li><p>alpha</p></li><li><p>beta</p></li><li><p>gamma</p></li></ol>', 'beta', 2, '<ul><li><p>x</p></li></ul>');
			expect(inside.getHTML()).toBe(
				'<ol><li><p>alpha</p></li><li><p>be</p></li></ol><ul><li><p>x</p></li></ul><ol start="3"><li><p>ta</p></li><li><p>gamma</p></li></ol>',
			);
		});

		it('in an empty list item, in its place', async () => {
			// 12: inside the second item's empty paragraph.
			const editor = await pasteInto('<ul><li><p>alpha</p></li><li><p></p></li></ul>', 12, 0, LIST);
			expect(editor.getHTML()).toBe('<ul><li><p>alpha</p></li><li><p>one</p></li><li><p>two</p></li></ul>');
			const lone = await pasteInto('<p>Before</p><ul><li><p></p></li></ul>', 11, 0, HEADING_AND_LIST);
			expect(lone.getHTML()).toBe(`<p>Before</p>${HEADING_AND_LIST}`);
			// 12: inside the middle item's empty paragraph.
			const middle = await pasteInto('<ul><li><p>alpha</p></li><li><p></p></li><li><p>gamma</p></li></ul>', 12, 0, LIST);
			expect(middle.getHTML()).toBe(
				'<ul><li><p>alpha</p></li><li><p>one</p></li><li><p>two</p></li><li><p>gamma</p></li></ul>',
			);
		});

		it('in the middle of a nested item, among the items of the one it is nested in', async () => {
			const page = '<ul><li><p>alpha</p><ul><li><p>beta</p></li></ul></li></ul>';
			const editor = await pasteInto(page, 'beta', 2, '<h2>Pasted heading</h2>');
			expect(editor.getHTML()).toBe(
				'<ul><li><p>alpha</p><ul><li><p>be</p></li></ul><h2>Pasted heading</h2><ul><li><p>ta</p></li></ul></li></ul>',
			);
		});

		it('at the end of an item with items under it, after the item and those', async () => {
			const page = '<ul><li><p>alpha</p><ul><li><p>under</p></li></ul></li><li><p>beta</p></li></ul>';
			const editor = await pasteInto(page, 'alpha', 5, '<h2>Pasted heading</h2>');
			expect(editor.getHTML()).toBe(
				'<ul><li><p>alpha</p><ul><li><p>under</p></li></ul></li></ul><h2>Pasted heading</h2><ul><li><p>beta</p></li></ul>',
			);
		});

		it('inside a callout, after the paragraph, in the callout', async () => {
			const editor = await pasteInto('<aside data-tone="tip"><p>Bring a jacket.</p></aside>', 'jacket.', 7, HEADING_AND_LIST);
			expect(editor.getHTML()).toBe(`<aside data-tone="tip"><p>Bring a jacket.</p>${HEADING_AND_LIST}</aside>`);
		});

		it('inside a table cell, in the cell', async () => {
			const page = '<table><tbody><tr><td><p>Fri</p></td><td><p>40</p></td></tr></tbody></table>';
			const editor = await pasteInto(page, 'Fri', 3, HEADING_AND_LIST);
			const cell = editor.state.doc.firstChild!.firstChild!.firstChild!;
			expect(cell.content.toJSON().map((n: { type: string }) => n.type)).toEqual(['paragraph', 'heading', 'bulletList']);
			expect(cell.textContent).toBe('FriPasted headingonetwo');
		});

		it('one line of markdown stays inline, in the middle of a paragraph', async () => {
			const editor = await pasteInto('<p>Total: .</p>', 'Total', 7, '<p><strong>40</strong> euros</p>');
			expect(editor.getHTML()).toBe('<p>Total: <strong>40</strong> euros.</p>');
			expect(editor.state.selection.from).toBe(posIn(editor, '.', 0));
		});

		it('one line of markdown stays inline at the end of a list item', async () => {
			const editor = await pasteInto('<ul><li><p>pack</p></li></ul>', 'pack', 4, '<p>, <strong>boots</strong></p>');
			expect(editor.getHTML()).toBe('<ul><li><p>pack, <strong>boots</strong></p></li></ul>');
		});

		it('a heading alone is a block, even mid-paragraph', async () => {
			const editor = await pasteInto('<p>Before after</p>', 'Before', 7, '<h2>Title</h2>');
			expect(editor.getHTML()).toBe('<p>Before </p><h2>Title</h2><p>after</p>');
		});

		it('nothing converted pastes the text as it is', async () => {
			const editor = await pasteInto('<p>Before</p>', 'Before', 6, '');
			expect(editor.getHTML()).toBe('<p>Beforemarkdown</p>');
		});
	});
});

describe('media', () => {
	it('an audio file becomes an audio block, and nothing is in the page while it uploads', async () => {
		let finish: (v: { url: string; filename: string; mime_type: string | null }) => void = () => {};
		const upload = () => new Promise<{ url: string; filename: string; mime_type: string | null }>((r) => (finish = r));
		const editor = track(htmlEditor('<p>Notes</p>', [mediaUploads(upload)]));
		const before = editor.getJSON();
		const file = new File(['abc'], 'memo.m4a', { type: 'audio/mp4' });
		const done = uploadFiles(editor.view, [file], editor.state.doc.content.size - 1, upload);
		expect(editor.getJSON()).toEqual(before);
		expect(pendingUploads(editor.view)).toBe(1);
		finish({ url: '/api/media/memo.m4a', filename: 'memo.m4a', mime_type: 'audio/mp4' });
		await done;
		expect(pendingUploads(editor.view)).toBe(0);
		expect(editor.getJSON().content?.[1]).toMatchObject({ type: 'audio', attrs: { src: '/api/media/memo.m4a', name: 'memo.m4a' } });
	});

	/** Uploads that finish when the test says, by file name. */
	function heldUploads() {
		const waiting = new Map<string, (v: { url: string; filename: string; mime_type: string | null }) => void>();
		const failing = new Map<string, (e: Error) => void>();
		const upload = vi.fn(
			(file: File) =>
				new Promise<{ url: string; filename: string; mime_type: string | null }>((resolve, reject) => {
					waiting.set(file.name, resolve);
					failing.set(file.name, reject);
				}),
		);
		return {
			upload,
			finish: (name: string) => waiting.get(name)!({ url: `/api/media/${name}`, filename: name, mime_type: 'image/png' }),
			fail: (name: string) => failing.get(name)!(new Error('disk full')),
		};
	}

	const photos = (...names: string[]) => names.map((n) => new File(['x'], n, { type: 'image/png' }));

	/** Each top-level block: its type, and its text or its address. */
	function blocks(editor: Editor): string[] {
		const out: string[] = [];
		editor.state.doc.forEach((n) => out.push(n.isAtom ? `${n.type.name}:${n.attrs.src}` : `${n.type.name}:${n.textContent}`));
		return out;
	}

	it('several files on an empty line all land there, in the order given, whichever finishes first', async () => {
		const held = heldUploads();
		const editor = track(htmlEditor('<p>Trip</p><p></p><p>After</p>', [mediaUploads(held.upload)]));
		// The Image command's flow: the trigger deleted, the place anchored while the dialog is open.
		const at = anchorAt(editor.state, posIn(editor, 'Trip') + 6);
		const done = uploadFiles(editor.view, photos('harbour.png', 'tram.png', 'castle.png'), at, held.upload);
		expect(pendingUploads(editor.view)).toBe(1);
		held.finish('castle.png');
		held.finish('harbour.png');
		await settle();
		held.finish('tram.png');
		await done;
		expect(blocks(editor)).toEqual([
			'paragraph:Trip',
			'image:/api/media/harbour.png',
			'image:/api/media/tram.png',
			'image:/api/media/castle.png',
			'paragraph:After',
		]);
		expect(pendingUploads(editor.view)).toBe(0);
		expect(held.upload).toHaveBeenCalledTimes(3);
	});

	it('several files at the end of a line land after it in order, and in the middle of one, between its halves', async () => {
		const end = heldUploads();
		const atEnd = track(htmlEditor('<p>Notes</p><p>After</p>', [mediaUploads(end.upload)]));
		const done = uploadFiles(atEnd.view, photos('one.png', 'two.png', 'three.png'), posIn(atEnd, 'Notes', 5), end.upload);
		for (const name of ['one.png', 'two.png', 'three.png']) {
			end.finish(name);
			await settle();
		}
		await done;
		expect(blocks(atEnd)).toEqual(['paragraph:Notes', 'image:/api/media/one.png', 'image:/api/media/two.png', 'image:/api/media/three.png', 'paragraph:After']);

		const mid = heldUploads();
		const middle = track(htmlEditor('<p>Notes More</p>', [mediaUploads(mid.upload)]));
		const split = uploadFiles(middle.view, photos('one.png', 'two.png'), posIn(middle, 'More'), mid.upload);
		mid.finish('two.png');
		mid.finish('one.png');
		await split;
		expect(blocks(middle)).toEqual(['paragraph:Notes ', 'image:/api/media/one.png', 'image:/api/media/two.png', 'paragraph:More']);
	});

	// Placing the first file splits the paragraph, and UniqueID re-marks the
	// second half with an id of its own: a step that drops a widget at that
	// half's start, which is where the next file goes.
	it('on a shared page, several files in the middle of a line land between its halves, in order', async () => {
		const { doc, writtenBy } = pageDoc('<p>Notes More</p><p>After</p>');
		const held = heldUploads();
		const { editor } = bind(doc, { plugins: false });
		editor.registerPlugin(mediaUploads(held.upload).config.addProseMirrorPlugins!.call({ editor } as never)[0]);
		const done = uploadFiles(editor.view, photos('one.png', 'two.png', 'three.png'), posIn(editor, 'More'), held.upload);
		for (const name of ['one.png', 'two.png', 'three.png']) {
			held.finish(name);
			await settle();
		}
		await done;
		expect(blocks(editor), `page written by ${writtenBy}`).toEqual([
			'paragraph:Notes ',
			'image:/api/media/one.png',
			'image:/api/media/two.png',
			'image:/api/media/three.png',
			'paragraph:More',
			'paragraph:After',
		]);
		const ids = editor.state.doc.content.content.map((n) => n.attrs[contract.id.attr]);
		expect(new Set(ids).size).toBe(ids.length);
	});

	/** A paste of `files` alone, as a copied picture is. */
	function pasteFiles(editor: Editor, files: File[]): boolean {
		const event = { clipboardData: { getData: () => '', files }, preventDefault() {} } as unknown as ClipboardEvent;
		return !!editor.view.someProp('handlePaste', (f) => f(editor.view, event, editor.state.doc.slice(0, 0)));
	}

	it('a file pasted over words leaves them until it lands, and a failed upload leaves them be', async () => {
		vi.mocked(toast.error).mockClear();
		const { doc } = pageDoc('<p>Keep these words</p>');
		const held = heldUploads();
		const { editor } = bind(doc, { plugins: false });
		editor.registerPlugin(mediaUploads(held.upload).config.addProseMirrorPlugins!.call({ editor } as never)[0]);
		const from = posIn(editor, 'these');
		editor.commands.setTextSelection({ from, to: from + 'these'.length });
		expect(pasteFiles(editor, photos('new.png'))).toBe(true);
		expect(blocks(editor)).toEqual(['paragraph:Keep these words']);
		held.fail('new.png');
		await settle();
		expect(blocks(editor)).toEqual(['paragraph:Keep these words']);
		expect(toast.error).toHaveBeenCalledOnce();
		expect(pendingUploads(editor.view)).toBe(0);
	});

	it('a file pasted over a selected image leaves the image when the upload fails', async () => {
		const { doc } = pageDoc('<p>A</p><img src="/api/media/old.png" alt="Old"><p>B</p>');
		const held = heldUploads();
		const { editor } = bind(doc, { plugins: false });
		editor.registerPlugin(mediaUploads(held.upload).config.addProseMirrorPlugins!.call({ editor } as never)[0]);
		editor.view.dispatch(editor.state.tr.setSelection(NodeSelection.create(editor.state.doc, editor.state.doc.child(0).nodeSize)));
		expect(pasteFiles(editor, photos('new.png'))).toBe(true);
		held.fail('new.png');
		await settle();
		expect(blocks(editor)).toEqual(['paragraph:A', 'image:/api/media/old.png', 'paragraph:B']);
	});

	it('a file pasted over words replaces them as it lands, one undo putting them back however long it took', async () => {
		const { doc } = pageDoc('<p>Before</p><p>Keep these words</p>');
		const held = heldUploads();
		const { editor } = bind(doc, { plugins: false });
		editor.registerPlugin(mediaUploads(held.upload).config.addProseMirrorPlugins!.call({ editor } as never)[0]);
		const from = posIn(editor, 'these');
		editor.commands.setTextSelection({ from, to: from + 'these'.length });
		expect(pasteFiles(editor, photos('new.png'))).toBe(true);
		await settle(700);
		expect(blocks(editor)).toEqual(['paragraph:Before', 'paragraph:Keep these words']);
		held.finish('new.png');
		await settle();
		expect(blocks(editor)).toEqual(['paragraph:Before', 'paragraph:Keep ', 'image:/api/media/new.png', 'paragraph: words']);
		editor.commands.undo();
		expect(blocks(editor)).toEqual(['paragraph:Before', 'paragraph:Keep these words']);
	});

	it('on a shared page, several files picked onto an empty line all land, and a failed one is said', async () => {
		vi.mocked(toast.error).mockClear();
		const { doc } = pageDoc('<p>Trip</p><p></p><p>After</p>');
		const held = heldUploads();
		const { editor } = bind(doc, { plugins: false });
		editor.registerPlugin(mediaUploads(held.upload).config.addProseMirrorPlugins!.call({ editor } as never)[0]);
		const at = anchorAt(editor.state, posIn(editor, 'Trip') + 6);
		const done = uploadFiles(editor.view, photos('harbour.png', 'tram.png', 'castle.png'), at, held.upload);
		held.finish('harbour.png');
		await settle();
		held.fail('tram.png');
		held.finish('castle.png');
		await done;
		expect(blocks(editor)).toEqual(['paragraph:Trip', 'image:/api/media/harbour.png', 'image:/api/media/castle.png', 'paragraph:After']);
		expect(toast.error).toHaveBeenCalledOnce();
		expect(pendingUploads(editor.view)).toBe(0);
	});

	it('files whose place is gone land at the caret, and say so', async () => {
		vi.mocked(toast).mockClear();
		const held = heldUploads();
		const editor = track(htmlEditor('<p>Before</p><p>Draft line.</p><p>End</p>', [mediaUploads(held.upload)]));
		const done = uploadFiles(editor.view, photos('one.png', 'two.png'), posIn(editor, 'Draft', 2), held.upload);
		const from = posIn(editor, 'Draft line.') - 1;
		editor.commands.deleteRange({ from, to: from + editor.state.doc.child(1).nodeSize });
		editor.commands.setTextSelection(posIn(editor, 'End', 3));
		held.finish('one.png');
		held.finish('two.png');
		await done;
		expect(blocks(editor)).toEqual(['paragraph:Before', 'paragraph:End', 'image:/api/media/one.png', 'image:/api/media/two.png']);
		expect(toast).toHaveBeenCalledOnce();
	});

	// Word, Excel and PowerPoint copy a picture of the range beside its HTML.
	it("pastes Office's HTML, not the picture of it beside it; a copied picture is uploaded", async () => {
		const upload = vi.fn(async () => ({ url: '/api/media/x.png', filename: 'image.png', mime_type: 'image/png' }));
		const editor = track(htmlEditor('<p>Notes</p>', [pasteHandling(async () => ({ html: '' })), mediaUploads(upload)]));
		editor.commands.setTextSelection(posIn(editor, 'Notes', 5));
		const office = new DataTransfer();
		office.setData('text/html', '<table><tr><td>Q1</td><td>40</td></tr></table>');
		office.setData('text/plain', 'Q1\t40');
		office.items.add(new File(['x'], 'image.png', { type: 'image/png' }));
		editor.view.dom.dispatchEvent(new ClipboardEvent('paste', { clipboardData: office, bubbles: true, cancelable: true }));
		await settle();
		expect(upload).not.toHaveBeenCalled();
		expect(blocks(editor)).toEqual(['paragraph:Notes', 'table:Q140']);

		const picture = new DataTransfer();
		picture.setData('text/html', '<img src="https://images.example.com/photo-1">');
		picture.items.add(new File(['x'], 'image.png', { type: 'image/png' }));
		editor.view.dom.dispatchEvent(new ClipboardEvent('paste', { clipboardData: picture, bubbles: true, cancelable: true }));
		await settle();
		expect(upload).toHaveBeenCalledOnce();
	});

	// A Word range holding a picture: its HTML carries the picture as a file
	// on the copying computer, and the picture of the range comes beside it.
	it("pastes the words of a Word range that holds a picture, not the picture of the range", async () => {
		const upload = vi.fn(async () => ({ url: '/api/media/range.png', filename: 'image.png', mime_type: 'image/png' }));
		const editor = track(htmlEditor('<p>Notes</p>', [pasteHandling(async () => ({ html: '' })), mediaUploads(upload)]));
		editor.commands.setTextSelection(posIn(editor, 'Notes', 5));
		const word = new DataTransfer();
		word.setData(
			'text/html',
			[
				'<html><body><!--StartFragment-->',
				'<p class=MsoNormal>Quarterly results were strong.</p>',
				'<p class=MsoNormal><![if !vml]><img width=320 height=200 src="file:///C:/Users/nick/AppData/Local/Temp/msohtmlclip1/01/clip_image001.png" v:shapes="Picture_x0020_1"><![endif]></p>',
				'<p class=MsoNormal>Next steps follow.</p>',
				'<!--EndFragment--></body></html>',
			].join(''),
		);
		word.setData('text/plain', 'Quarterly results were strong.\n\nNext steps follow.');
		word.items.add(new File(['x'], 'image.png', { type: 'image/png' }));
		editor.view.dom.dispatchEvent(new ClipboardEvent('paste', { clipboardData: word, bubbles: true, cancelable: true }));
		await settle();
		expect(upload).not.toHaveBeenCalled();
		const text = blocks(editor).join(' ');
		expect(text).toContain('Quarterly results were strong.');
		expect(text).toContain('Next steps follow.');
	});

	it('a failed upload leaves the page as it was', async () => {
		const editor = track(htmlEditor('<p>Notes</p>', [mediaUploads()]));
		const before = editor.getJSON();
		await uploadFiles(editor.view, [new File(['x'], 'a.png', { type: 'image/png' })], 1, async () => {
			throw new Error('disk full');
		});
		expect(editor.getJSON()).toEqual(before);
		expect(pendingUploads(editor.view)).toBe(0);
	});

	it('classifies addresses as the converter does, and loads Drive files from their download address', () => {
		// The converter's own cases (`media_kind`'s test reads the same file).
		const cases = JSON.parse(fs.readFileSync(MEDIA_KINDS, 'utf8')) as { src: string; name: string; kind: string }[];
		expect(cases.length).toBeGreaterThan(10);
		for (const c of cases) expect(kindOfLink(c.src, c.name), `${c.src} ${c.name}`).toBe(c.kind);
		expect(kindOfLink('https://example.com/report.pdf', '')).toBe('file');
		expect(driveFileId('/drive/df_5')).toBe('df_5');
		expect(mediaSrc('/drive/df_5')).toBe('/api/drive/files/df_5/download');
		expect(mediaSrc('https://example.com/a.png')).toBe('https://example.com/a.png');
	});

	it("reads a file's kind by kindOfLink wherever the app draws one, with no list of its own", () => {
		// A list of extensions per view drifts from the converter's: a `.tif`
		// the page drew as an image opened as a download, and its ref showed
		// a file icon.
		const LIB = path.resolve(HERE, '..');
		// TODO(2026-10-10): migrate components/RefCard.svelte's extension list
		// onto kindOfLink and add it here; another edit to that file is in
		// flight.
		for (const file of ['components/tabs/views/AssetView.svelte', 'utils/refRoutes.ts']) {
			const source = fs.readFileSync(path.join(LIB, file), 'utf8');
			expect(source, file).toMatch(/kindOfLink\(/);
			expect(source, file).not.toMatch(/jpe\?g|\bjpeg\b|\bmp3\b|\bwebm\b|\bheic\b/);
		}
		expect(fileIcon({ filename: 'scan.TIF' })).toBe('ri:image-line');
		expect(fileIcon({ filename: 'memo.opus' })).toBe('ri:music-2-line');
		expect(fileIcon({ filename: 'clip.m4v' })).toBe('ri:movie-line');
		expect(fileIcon({ filename: 'Q3 report.PDF' })).toBe('ri:file-pdf-line');
		expect(fileIcon({ filename: 'notes' })).toBe('ri:file-line');
		expect(fileIcon({ mimeType: 'audio/mp4', filename: 'notes' })).toBe('ri:music-2-line');
	});

	// The contract takes any path, and a block's address can hold a % that
	// is no escape: the block is still drawn, by the name it can read.
	it('reads an address whose %-escapes do not decode as written', () => {
		expect(driveFileId('/drive/%E0%A4%A')).toBe('%E0%A4%A');
		expect(mediaSrc('/drive/%E0%A4%A')).toBe('/api/drive/files/%25E0%25A4%25A/download');
		expect(nameOfSrc('https://example.com/files/50%.pdf')).toBe('50%.pdf');
		expect(nameOfSrc('/drive/report%zz.pdf')).toBe('report%zz.pdf');
		expect(nameOfSrc('/drive/Trip%20notes.pdf?x=1')).toBe('Trip notes.pdf');
		expect(decoded('a%2Fb')).toBe('a/b');
	});

	it('a widget that cannot draw its block leaves the page open, the block saying so', () => {
		const element = document.createElement('div');
		document.body.append(element);
		const throwing = () => {
			throw new URIError('URI malformed');
		};
		const error = vi.spyOn(console, 'error').mockImplementation(() => {});
		const editor = track(
			new Editor({
				element,
				content: '<p>Before.</p><virtues-file src="/drive/%E0%A4%A"></virtues-file><p>After.</p>',
				extensions: contractExtensions({
					nodes: { file: { addNodeView: () => svelteNodeView(throwing as never, 'div') } },
				}),
			}),
		);
		error.mockRestore();
		expect(editor.state.doc.childCount).toBe(3);
		expect(element.textContent).toContain('Before.');
		expect(element.textContent).toContain('After.');
		expect(element.querySelector('.doc-widget-failed')?.textContent).toBe("Couldn't show this block. The rest of the page works as usual.");
	});
});

describe('tables', () => {
	it('aligns every cell of the caret’s column, and clears it again', () => {
		const editor = track(
			htmlEditor('<table><tbody><tr><th><p>Day</p></th><th><p>Cost</p></th></tr><tr><td><p>Fri</p></td><td><p>40</p></td></tr></tbody></table>'),
		);
		editor.commands.setTextSelection(posIn(editor, '40'));
		expect(tableActions(editor).map((a) => a.id)).toContain('delete-table');
		alignColumn(editor, 'right');
		expect(columnAlign(editor.state)).toBe('right');
		expect(editor.getHTML()).toContain('<th align="right"><p>Cost</p></th>');
		expect(editor.getHTML()).toContain('<td align="right"><p>40</p></td>');
		expect(editor.getHTML()).toContain('<th><p>Day</p></th>');
		alignColumn(editor, null);
		expect(columnAlign(editor.state)).toBeNull();
	});

	// A markdown page draws a typed row of cells and the delimiter row under
	// it as a table; on a block page they stayed two lines of pipes.
	it('a row of cells typed, then its delimiter row, makes a table, the caret in its first cell', async () => {
		const enter = (e: Editor) =>
			e.view.dom.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }));
		const { editor } = bind(pageDoc('<p>Costs</p><p></p>').doc);
		await settle();
		editor.commands.setTextSelection(editor.state.doc.content.size - 1);
		typeIn(editor, '| Day | Cost | Note |');
		enter(editor);
		typeIn(editor, '| :-- | ---: | --- |');
		const table = editor.state.doc.child(1);
		expect(table.type.name).toBe('table');
		const cells = (row: number) => {
			const out: string[] = [];
			table.child(row).forEach((cell) => out.push(`${cell.type.name}:${cell.attrs.align}:${cell.textContent}`));
			return out;
		};
		expect(cells(0)).toEqual(['tableHeader:left:Day', 'tableHeader:right:Cost', 'tableHeader:null:Note']);
		expect(cells(1)).toEqual(['tableCell:left:', 'tableCell:right:', 'tableCell:null:']);
		expect(editor.state.doc.childCount).toBe(2);
		const { $from } = editor.state.selection;
		expect($from.node(-1)).toBe(table.child(1).child(0));
		typeIn(editor, 'Fri');
		expect(editor.state.doc.child(1).child(1).child(0).textContent).toBe('Fri');

		// A delimiter row whose cells the row above does not match is text.
		const { editor: other } = bind(pageDoc('<p></p>').doc);
		await settle();
		other.commands.setTextSelection(1);
		typeIn(other, '| a | b | c |');
		enter(other);
		typeIn(other, '| --- | --- |');
		expect(other.state.doc.childCount).toBe(2);
		expect(other.state.doc.child(1).textContent).toBe('| --- | --- |');
	});
});

describe('tables, pasted into', () => {
	// The paste plugin takes a paste before tableEditing does: a markdown
	// paste over selected cells would replace only the head cell.
	it('markdown text pasted over selected cells fills every cell, as any text does', async () => {
		const convert = vi.fn(async () => ({ html: '<blockquote><p>q</p></blockquote>' }));
		const { doc } = pageDoc(
			'<table><tbody><tr><th><p>H1</p></th><th><p>H2</p></th></tr><tr><td><p>c1</p></td><td><p>c2</p></td></tr></tbody></table>',
		);
		const provider = fakeProvider(doc);
		cleanups.push(provider.destroy);
		const element = document.createElement('div');
		document.body.append(element);
		const editor = track(
			createPageEditor({ element, doc: { ydoc: doc, provider }, editable: true, placeholder: 'Write', plugins: [pasteHandling(convert)] }),
		);
		const cell = (text: string) => editor.state.doc.resolve(posIn(editor, text) - 2);
		editor.view.dispatch(editor.state.tr.setSelection(new CellSelection(cell('H1'), cell('c2'))));
		const data = new DataTransfer();
		data.setData('text/plain', '> q');
		editor.view.dom.dispatchEvent(new ClipboardEvent('paste', { clipboardData: data, bubbles: true, cancelable: true }));
		await settle();
		expect(convert).not.toHaveBeenCalled();
		const cells: string[] = [];
		editor.state.doc.descendants((n) => {
			if (n.type.name === 'tableCell' || n.type.name === 'tableHeader') cells.push(n.textContent);
			return true;
		});
		expect(cells).toEqual(['> q', '> q', '> q', '> q']);
	});

	// A file over selected cells replaces them as a selection is replaced:
	// every cell goes, not the words of the one the drag ended in.
	it('files pasted over selected cells empty every one of them and land in the first', async () => {
		const table =
			'<p>Before</p><table><tbody><tr><th><p>H1</p></th><th><p>H2</p></th></tr><tr><td><p>c1</p></td><td><p>c2</p></td></tr></tbody></table><p>After</p>';
		for (const [anchor, head, want] of [
			['H1', 'c2', ['image', '', '', '']],
			['c2', 'H1', ['image', '', '', '']],
			['H2', 'c2', ['H1', 'image', 'c1', '']],
			['H1', 'H2', ['image', '', 'c1', 'c2']],
		] as const) {
			const { doc } = pageDoc(table);
			const provider = fakeProvider(doc);
			cleanups.push(provider.destroy);
			const element = document.createElement('div');
			document.body.append(element);
			let finish = () => {};
			const upload = vi.fn(
				() =>
					new Promise<{ url: string; filename: string; mime_type: string | null }>((resolve) => {
						finish = () => resolve({ url: '/api/media/shot.png', filename: 'shot.png', mime_type: 'image/png' });
					}),
			);
			const editor = track(
				createPageEditor({
					element,
					doc: { ydoc: doc, provider },
					editable: true,
					placeholder: 'Write',
					plugins: [pasteHandling(async () => ({ html: '' })), mediaUploads(upload)],
				}),
			);
			const cell = (text: string) => editor.state.doc.resolve(posIn(editor, text) - 2);
			const cells = () => {
				const out: string[] = [];
				editor.state.doc.descendants((n) => {
					if (n.type.name !== 'tableCell' && n.type.name !== 'tableHeader') return true;
					out.push(n.childCount === 1 && n.firstChild?.type.name === 'image' ? 'image' : n.textContent);
					return false;
				});
				return out;
			};
			editor.view.dispatch(editor.state.tr.setSelection(new CellSelection(cell(anchor), cell(head))));
			const data = new DataTransfer();
			data.items.add(new File(['x'], 'shot.png', { type: 'image/png' }));
			editor.view.dom.dispatchEvent(new ClipboardEvent('paste', { clipboardData: data, bubbles: true, cancelable: true }));
			await settle();
			const going = [...element.querySelectorAll('.doc-paste-replaced')].map((el) => el.textContent).sort();
			expect(going, `${anchor}..${head} drawn as going`).toEqual(
				['H1', 'H2', 'c1', 'c2'].filter((t, i) => (want[i] as string) !== t),
			);
			finish();
			await settle();
			expect(cells(), `${anchor}..${head}`).toEqual(want);
			expect(editor.state.doc.firstChild?.textContent).toBe('Before');
			expect(editor.state.doc.lastChild?.textContent).toBe('After');
			editor.commands.undo();
			expect(cells(), `${anchor}..${head} undone`).toEqual(['H1', 'H2', 'c1', 'c2']);
		}
	});
});

describe('code highlighting', () => {
	// A token's colour is its theme's: a block kept from one theme is never
	// drawn in another, on a page opened after the change or one open
	// through it.
	it("draws every block in the page's theme, after the theme changes too", async () => {
		let theme = 'github-light';
		const colours: Record<string, string> = { 'github-light': 'rgb(1, 2, 3)', 'github-dark': 'rgb(200, 201, 202)' };
		const highlight = vi.fn(async (code: string) => [[{ content: code, color: colours[theme], offset: 0 }]]);
		const html = '<pre data-language="rust"><code>fn theme_test() {}</code></pre>';
		const drawn = (editor: Editor) => editor.view.dom.querySelector('[style*="color"]')?.getAttribute('style')?.replace(/;$/, '');

		const first = track(htmlEditor(html, [codeHighlight(highlight, () => theme)]));
		await settle(250);
		expect(drawn(first)).toBe('color: rgb(1, 2, 3)');
		first.destroy();

		theme = 'github-dark';
		const second = track(htmlEditor(html, [codeHighlight(highlight, () => theme)]));
		await settle(250);
		expect(drawn(second)).toBe('color: rgb(200, 201, 202)');

		theme = 'github-light';
		document.documentElement.setAttribute('data-theme', 'light');
		await settle(250);
		expect(drawn(second)).toBe('color: rgb(1, 2, 3)');
		document.documentElement.removeAttribute('data-theme');
	});

	it('is drawn again after another device changes the page', async () => {
		const { doc } = pageDoc(FIXTURE);
		const { element } = bind(doc);
		const coloured = () => element.querySelectorAll('[style*="color: var(--color-primary)"]').length;
		await settle(250);
		expect(coloured(), 'highlighted once mounted').toBeGreaterThan(0);

		// Another device types in the heading. The binding rebuilds the whole
		// document for a remote change, which maps every decoration away.
		const other = new Y.Doc();
		Y.applyUpdate(other, Y.encodeStateAsUpdate(doc));
		const heading = other.getXmlFragment(contract.fragment).get(0) as Y.XmlElement;
		const text = heading.get(0) as Y.XmlText;
		const before = Y.encodeStateVector(other);
		text.insert(0, 'Our ');
		Y.applyUpdate(doc, Y.encodeStateAsUpdate(other, before), 'wire');
		await settle(250);
		expect(coloured(), 'highlighted after the remote change').toBeGreaterThan(0);
	});
});

describe('the assistant on the page', () => {
	it('flashes the blocks a chat edit wrote, by id, then lets them go', async () => {
		vi.useFakeTimers();
		try {
			const editor = track(htmlEditor('<p data-id="aaaa0001">One</p><p data-id="aaaa0002">Two</p>', [aiPresence()]));
			flashBlocks(editor, ['aaaa0002']);
			expect(flashedBlocks(editor.state)).toEqual(['aaaa0002']);
			expect(editor.view.dom.querySelectorAll('.doc-flash')).toHaveLength(1);
			vi.advanceTimersByTime(2000);
			expect(flashedBlocks(editor.state)).toEqual([]);
		} finally {
			vi.useRealTimers();
		}
	});

	it('draws a caret and a trail that follow edits, and clears them', () => {
		const editor = track(htmlEditor('<p>Hello</p>', [aiPresence()]));
		setAiCaret(editor, { pos: 6, phase: 'active' });
		setAiTrail(editor, { from: 1, to: 6 });
		editor.commands.insertContentAt(1, 'Oh ');
		expect(editor.view.dom.querySelectorAll('.doc-ai-trail')).not.toHaveLength(0);
		clearAi(editor);
		expect(editor.view.dom.querySelectorAll('.doc-ai-trail')).toHaveLength(0);
	});
});

describe('the page moving under a panel, a menu or a picker', () => {
	/**
	 * This device's editor and a second device on the same page, their
	 * updates crossing as the socket carries them. The second device's edits
	 * reach this editor as Tiptap's binding applies any remote change: one
	 * step that rewrites the whole document.
	 */
	function twoDevices(html: string) {
		const { doc } = pageDoc(html);
		const other = new Y.Doc();
		Y.applyUpdate(other, Y.encodeStateAsUpdate(doc));
		doc.on('update', (u: Uint8Array, origin: unknown) => {
			if (origin !== 'wire') Y.applyUpdate(other, u, 'wire');
		});
		other.on('update', (u: Uint8Array, origin: unknown) => {
			if (origin !== 'wire') Y.applyUpdate(doc, u, 'wire');
		});
		const { editor, element } = bind(doc);
		const phone = bind(other, { plugins: false }).editor;
		return { editor, phone, element };
	}

	/** The phone types at the start of the block holding `text`, then adds a paragraph at the top. */
	async function phoneEditsAbove(phone: Editor, text: string) {
		phone.commands.insertContentAt(posIn(phone, text), 'Moved: ');
		phone.commands.insertContentAt(0, '<p>Added on the phone first.</p>');
		await settle();
	}

	function blocks(editor: Editor): string[] {
		const out: string[] = [];
		editor.state.doc.forEach((n) => out.push(`${n.type.name}:${n.textContent}`));
		return out;
	}

	const panel = () => vi.mocked(linkEditor.show).mock.calls.at(-1)![1];

	beforeEach(() => {
		vi.mocked(linkEditor.show).mockClear();
		vi.mocked(toast.error).mockClear();
	});

	it('the link panel links the words it opened on, wherever they have moved', async () => {
		const { editor, phone } = twoDevices('<p>Lunch with Nick on Friday.</p><p>Read the budget notes before lunch.</p>');
		await settle();
		const from = posIn(editor, 'budget notes');
		editor.commands.setTextSelection({ from, to: from + 'budget notes'.length });
		expect(editLinkAtSelection(editor)).toBe(true);
		await phoneEditsAbove(phone, 'Read the budget');

		panel()({ label: 'budget notes', href: '/page/page_1' });
		expect(blocks(editor)).toEqual([
			'paragraph:Added on the phone first.',
			'paragraph:Lunch with Nick on Friday.',
			'paragraph:Moved: Read the budget notes before lunch.',
		]);
		const link = linkAt(editor.state, posIn(editor, 'budget notes', 1));
		expect(link).toMatchObject({ text: 'budget notes', href: '/page/page_1' });
		await settle();
		expect(blocks(phone)).toEqual(blocks(editor));
		expect(toast.error).not.toHaveBeenCalled();
	});

	it('the link panel leaves words someone changed while it was open, and says so', async () => {
		const { editor, phone } = twoDevices('<p>Read the budget notes before lunch.</p>');
		await settle();
		const from = posIn(editor, 'budget notes');
		editor.commands.setTextSelection({ from, to: from + 'budget notes'.length });
		editLinkAtSelection(editor);
		const at = posIn(phone, 'notes');
		phone.commands.insertContentAt({ from: at, to: at + 'notes'.length }, 'figures');
		await settle();
		const before = editor.getJSON();

		panel()({ label: 'budget notes', href: '/page/page_1' });
		expect(editor.getJSON()).toEqual(before);
		expect(toast.error).toHaveBeenCalledOnce();
	});

	it('editing a link changes it where it is now, and leaves one someone else changed', async () => {
		const { editor, phone } = twoDevices('<p>Call notes.</p><p>See <a href="/page/page_1">the plan</a> for details.</p>');
		await settle();
		editor.commands.setTextSelection(posIn(editor, 'the plan', 2));
		editLinkAtSelection(editor);
		await phoneEditsAbove(phone, 'See the plan');
		panel()({ label: 'the new plan', href: '/page/page_2' });
		expect(blocks(editor)).toEqual([
			'paragraph:Added on the phone first.',
			'paragraph:Call notes.',
			'paragraph:Moved: See the new plan for details.',
		]);
		expect(linkAt(editor.state, posIn(editor, 'the new plan', 1))).toMatchObject({ href: '/page/page_2' });

		editor.commands.setTextSelection(posIn(editor, 'the new plan', 2));
		editLinkAtSelection(editor);
		const at = posIn(phone, 'new plan');
		phone.commands.insertContentAt({ from: at, to: at + 3 }, 'old');
		await settle();
		const before = editor.getJSON();
		panel()({ label: 'the plan', href: '/page/page_3' });
		expect(editor.getJSON()).toEqual(before);
		expect(toast.error).toHaveBeenCalledOnce();
	});

	it('the link menu turns into an embed or unlinks the link it opened on', async () => {
		const { editor, phone } = twoDevices(
			'<p>Call notes.</p><p>Ask <a href="/person/person_2">David Okafor</a> about <a href="/page/page_1">lunch</a>.</p>',
		);
		await settle();
		const david = linkAt(editor.state, posIn(editor, 'David', 1))!;
		const lunch = linkAt(editor.state, posIn(editor, 'lunch', 1))!;
		const embed = linkMenu(editor, david, { x: 0, y: 0 }).find((i) => i.id === 'embed')!;
		const remove = linkMenu(editor, lunch, { x: 0, y: 0 }).find((i) => i.id === 'remove')!;
		await phoneEditsAbove(phone, 'Call notes');

		remove.action!();
		embed.action!();
		expect(blocks(editor)).toEqual([
			'paragraph:Added on the phone first.',
			'paragraph:Moved: Call notes.',
			'paragraph:Ask  about lunch.',
		]);
		const ask = editor.getJSON().content?.[2].content ?? [];
		expect(ask[1]).toMatchObject({ type: 'mention', attrs: { to: '/person/person_2', label: 'David Okafor' } });
		expect(ask.some((n) => n.marks?.some((m) => m.type === 'link'))).toBe(false);
		expect(toast.error).not.toHaveBeenCalled();

	});

	it('the link menu leaves a link someone changed while it was open, and says so', async () => {
		const { editor, phone } = twoDevices('<p>See <a href="/page/page_1">the plan</a> and <a href="/page/page_2">the budget</a>.</p>');
		await settle();
		const plan = linkAt(editor.state, posIn(editor, 'the plan', 1))!;
		const remove = linkMenu(editor, plan, { x: 0, y: 0 }).find((i) => i.id === 'remove')!;
		// The phone rewrites the link's words; the budget link now sits where the plan's began.
		const at = posIn(phone, 'the plan');
		phone.commands.insertContentAt({ from: at, to: at + 'the plan and '.length }, 'and ');
		await settle();
		const before = editor.getJSON();
		remove.action!();
		expect(editor.getJSON()).toEqual(before);
		expect(toast.error).toHaveBeenCalledOnce();
	});

	it('an applet picked from the list lands where the Applet command ran', async () => {
		const { editor, phone } = twoDevices('<h2>Trip to Lisbon</h2><p>Lunch with Nick on Friday.</p>');
		await settle();
		const end = posIn(editor, 'Lunch with Nick on Friday.', 'Lunch with Nick on Friday.'.length);
		const [sleep] = appletCommands(
			[{ id: 'sleep-week', name: 'Sleep', description: null, has_face: true } as unknown as Applet],
			anchorAt(editor.state, end),
		);
		await phoneEditsAbove(phone, 'Trip to Lisbon');
		sleep.run(editor, { from: 0, to: 0 });
		expect(blocks(editor)).toEqual([
			'paragraph:Added on the phone first.',
			'heading:Moved: Trip to Lisbon',
			'paragraph:Lunch with Nick on Friday.',
			'applet:',
		]);
	});

	it('a picked file uploads where the command ran, its placeholder moving with the page', async () => {
		const { editor, phone, element } = twoDevices('<h2>Trip to Lisbon</h2><p>Lunch with Nick on Friday.</p>');
		await settle();
		const end = posIn(editor, 'Lunch with Nick on Friday.', 'Lunch with Nick on Friday.'.length);
		const at = anchorAt(editor.state, end);
		// The file dialog is open while the phone writes.
		await phoneEditsAbove(phone, 'Trip to Lisbon');
		let finish: (v: { url: string; filename: string; mime_type: string | null }) => void = () => {};
		const upload = () => new Promise<{ url: string; filename: string; mime_type: string | null }>((r) => (finish = r));
		const done = uploadFiles(editor.view, [new File(['abc'], 'memo.m4a', { type: 'audio/mp4' })], at, upload);
		phone.commands.insertContentAt(0, '<p>And again, while it uploads.</p>');
		await settle();
		expect(pendingUploads(editor.view)).toBe(1);
		expect(element.querySelectorAll('.doc-upload')).toHaveLength(1);

		finish({ url: '/api/media/memo.m4a', filename: 'memo.m4a', mime_type: 'audio/mp4' });
		await done;
		expect(pendingUploads(editor.view)).toBe(0);
		expect(blocks(editor)).toEqual([
			'paragraph:And again, while it uploads.',
			'paragraph:Added on the phone first.',
			'heading:Moved: Trip to Lisbon',
			'paragraph:Lunch with Nick on Friday.',
			'audio:',
		]);
	});

	it('a paste waiting on the server lands where it was pasted while the phone writes', async () => {
		const { editor, phone, element } = twoDevices('<p>Plan</p><p></p><p>Lunch with Nick.</p>');
		await settle();
		let answer: (v: { html: string }) => void = () => {};
		const at = editor.state.doc.child(0).nodeSize + 1;
		const done = pasteMarkdown(editor.view, '- passport\n- tickets', at, () => new Promise((r) => (answer = r)));
		phone.commands.insertContentAt(posIn(phone, 'Plan', 'Plan'.length), ' for Friday');
		await phoneEditsAbove(phone, 'Lunch with Nick');
		expect(element.querySelectorAll('.doc-paste-pending')).toHaveLength(1);

		answer({ html: '<ul><li><p>passport</p></li><li><p>tickets</p></li></ul>' });
		await done;
		expect(blocks(editor)).toEqual([
			'paragraph:Added on the phone first.',
			'paragraph:Plan for Friday',
			'bulletList:passporttickets',
			'paragraph:Moved: Lunch with Nick.',
		]);
		expect(element.querySelectorAll('.doc-paste-pending')).toHaveLength(0);
		expect(toast).not.toHaveBeenCalled();
		await settle();
		expect(blocks(phone)).toEqual(blocks(editor));
	});

	// Menus over the page are placed once, by screen coordinates: scrolled,
	// the page moved under Accept and Reject, the `/` menu, the `@` picker
	// and the prompt, which stayed over other words.
	it('a menu over the page goes where the text it opened at is now, scrolled or written above', async () => {
		const { editor, phone } = twoDevices('<p>Plan</p><p>Lunch with Nick.</p>');
		await settle();
		let scrolled = 0;
		const { view } = editor;
		view.coordsAtPos = ((pos: number) => ({
			left: 10 + pos,
			right: 10 + pos,
			top: 100 + pos * 20 - scrolled,
			bottom: 118 + pos * 20 - scrolled,
		})) as typeof view.coordsAtPos;
		const at = posIn(editor, 'Nick');
		const pin = pinAt(view, at, { x: 10 + at, y: 122 + at * 20 });
		scrolled = 300;
		expect(placeOf(view, pin)).toEqual({ x: 10 + at, y: 122 + at * 20 - 300 });
		await phoneEditsAbove(phone, 'Lunch with');
		const now = posIn(editor, 'Nick');
		expect(now).toBeGreaterThan(at);
		expect(placeOf(view, pin)).toEqual({ x: 10 + now, y: 122 + now * 20 - 300 });

		const page = fs.readFileSync(path.resolve(HERE, '../components/pages/DocumentEditor.svelte'), 'utf8');
		expect(page).toMatch(/const onScroll = \(\) => \{\s+updateTable\(e\);\s+updateSuggestion\(e\);\s+if \(toolbarOpen\) updateToolbar\(e\);\s+placeMenus\(e\);/);
		for (const pinned of ['slashPin', 'pickerPin', 'aiPin']) {
			expect(page, pinned).toMatch(new RegExp(`${pinned} = (editor \\? )?pinAt\\(`));
			expect(page, pinned).toMatch(new RegExp(`Pos = placeOf\\(e\\.view, ${pinned}\\)`));
		}
	});

	/** Paste or upload over the selection, the server or the upload held; returns what lets it land. */
	function waitOver(editor: Editor, via: 'paste' | 'upload'): () => Promise<void> {
		const { selection } = editor.state;
		if (via === 'paste') {
			let answer: (v: { html: string }) => void = () => {};
			const done = pasteMarkdown(editor.view, '## Plan', selection.from, () => new Promise((r) => (answer = r)), selection);
			return async () => {
				answer({ html: '<h2>Plan</h2>' });
				await done;
			};
		}
		let finish = () => {};
		const upload = () =>
			new Promise<{ url: string; filename: string; mime_type: string | null }>((r) => {
				finish = () => r({ url: '/api/media/plan.png', filename: 'plan.png', mime_type: 'image/png' });
			});
		const done = uploadFiles(editor.view, [new File(['x'], 'plan.png', { type: 'image/png' })], selection.from, upload, selection);
		return async () => {
			finish();
			await done;
		};
	}

	// The start of a held selection held to the character before it, so the
	// phone's words typed at it fell inside it; and nothing checked the
	// selection still held what was chosen. The landing deleted the phone's
	// words, and nobody on the phone was told.
	it('a paste or an upload waiting over a selection leaves the words the phone typed into it, and those it typed beside it', async () => {
		for (const via of ['paste', 'upload'] as const) {
			const landed = via === 'paste' ? 'heading:Plan' : 'image:';
			for (const [phoneTypes, want, told] of [
				[(p: Editor) => p.commands.insertContentAt(posIn(p, 'Hello world', 5), ' there'), ['paragraph:Say ', landed, 'paragraph:Hello there world'], true],
				[(p: Editor) => p.commands.insertContentAt(posIn(p, 'Hello world'), 'Oh, '), ['paragraph:Say Oh, ', landed], false],
				[(p: Editor) => p.commands.insertContentAt(posIn(p, 'Hello world', 'Hello world'.length), '!'), ['paragraph:Say ', landed, 'paragraph:!'], false],
			] as const) {
				const { editor, phone } = twoDevices('<p>Say Hello world</p><p>Next</p>');
				await settle();
				vi.mocked(toast).mockClear();
				const from = posIn(editor, 'Hello world');
				editor.commands.setTextSelection({ from, to: from + 'Hello world'.length });
				const land = waitOver(editor, via);
				phoneTypes(phone);
				await settle();
				await land();
				expect(blocks(editor), `${via}: ${want.join(' | ')}`).toEqual([...want, 'paragraph:Next']);
				expect(vi.mocked(toast).mock.calls.length, `${via} told`).toBe(told ? 1 : 0);
				await settle();
				expect(blocks(phone)).toEqual(blocks(editor));
				for (const e of [editor, phone]) e.destroy();
			}
			// Words this device writes into it meanwhile are its own choice: they go with it.
			const { editor, phone } = twoDevices('<p>Say Hello world</p><p>Next</p>');
			await settle();
			vi.mocked(toast).mockClear();
			const from = posIn(editor, 'Hello world');
			editor.commands.setTextSelection({ from, to: from + 'Hello world'.length });
			const land = waitOver(editor, via);
			editor.view.dispatch(editor.state.tr.insertText(' there', from + 5));
			await land();
			expect(blocks(editor), `${via}: typed here`).toEqual(['paragraph:Say ', landed, 'paragraph:Next']);
			expect(toast).not.toHaveBeenCalled();
			await settle();
			expect(blocks(phone)).toEqual(blocks(editor));
		}
	});

	it('a paste that splits a paragraph keeps its id on the words before it, and gives every block its own', async () => {
		const { editor, phone } = twoDevices('<p>Plan</p><p>Before after</p>');
		await settle();
		const id = editor.state.doc.child(1).attrs[contract.id.attr];
		const at = posIn(editor, 'Before after', 7);
		editor.commands.setTextSelection(at);
		await pasteMarkdown(editor.view, '## Pasted', at, async () => ({ html: '<h2>Pasted</h2><ul><li><p>one</p></li></ul>' }));
		await settle();
		expect(blocks(editor)).toEqual(['paragraph:Plan', 'paragraph:Before ', 'heading:Pasted', 'bulletList:one', 'paragraph:after']);
		expect(editor.state.doc.child(1).attrs[contract.id.attr]).toBe(id);
		const all = ids(editor);
		expect(all.every(Boolean)).toBe(true);
		expect(new Set(all).size).toBe(all.length);
		expect(blocks(phone)).toEqual(blocks(editor));
	});

	it('a paste whose paragraph the phone deleted lands where the paragraph was', async () => {
		const { editor, phone } = twoDevices('<p>Plan</p><p>Draft line.</p><p>Lunch with Nick.</p>');
		await settle();
		vi.mocked(toast).mockClear();
		let answer: (v: { html: string }) => void = () => {};
		const done = pasteMarkdown(editor.view, '- passport', posIn(editor, 'Draft line.', 5), () => new Promise((r) => (answer = r)));
		const from = posIn(phone, 'Draft line.') - 1;
		phone.commands.deleteRange({ from, to: from + phone.state.doc.child(1).nodeSize });
		await settle();

		answer({ html: '<ul><li><p>passport</p></li></ul>' });
		await done;
		expect(blocks(editor)).toEqual(['paragraph:Plan', 'bulletList:passport', 'paragraph:Lunch with Nick.']);
		expect(toast).not.toHaveBeenCalled();
	});

	it('an open `/` menu stays open, and follows its query, while the phone writes', async () => {
		const { editor, phone } = twoDevices('<h2>Plan</h2><p>Lunch with Nick.</p><p></p>');
		await settle();
		editor.commands.setTextSelection(editor.state.doc.content.size - 1);
		typeIn(editor, '/ta');
		await settle();
		expect(triggerState(editor.state, '/')).toMatchObject({ active: true, query: 'ta' });
		await phoneEditsAbove(phone, 'Plan');
		phone.commands.insertContentAt(posIn(phone, 'Lunch with Nick.', 'Lunch with Nick.'.length), ' Book seats.');
		await settle();
		const open = triggerState(editor.state, '/');
		expect(open).toMatchObject({ active: true, query: 'ta' });
		expect(editor.state.doc.textBetween(open.from, open.from + 3)).toBe('/ta');
		typeIn(editor, 'b');
		expect(triggerState(editor.state, '/')).toMatchObject({ active: true, query: 'tab' });
	});

	it('a mention picked after the phone wrote replaces the `@` and the query typed after it', async () => {
		const { editor, phone } = twoDevices('<h2>Plan for the week</h2><p>Lunch with</p>');
		await settle();
		editor.commands.setTextSelection(posIn(editor, 'Lunch with', 'Lunch with'.length));
		sendAtOnce(editor, ' @Da');
		await settle();
		const at = triggerState(editor.state, '@').from;
		const picked = anchorRange(editor.state, at, at + 1);
		await phoneEditsAbove(phone, 'Plan for the week');
		expect(triggerState(editor.state, '@')).toMatchObject({ active: true, query: 'Da' });
		insertEntity(
			editor,
			pickedRange(editor.state, '@', resolveRange(picked)!),
			{ name: 'David Okafor', url: '/person/person_2', entity_type: 'person' },
		);
		expect(blocks(editor)).toEqual([
			'paragraph:Added on the phone first.',
			'heading:Moved: Plan for the week',
			'paragraph:Lunch with  ',
		]);
		expect(editor.getJSON().content?.[2].content?.[1]).toMatchObject({ type: 'mention', attrs: { label: 'David Okafor' } });
	});

	it('typing after the phone wrote earlier in the same paragraph lands where the caret was', async () => {
		const { editor, phone } = twoDevices('<p>Intro</p><p>Hello world</p>');
		await settle();
		editor.commands.setTextSelection(posIn(editor, 'Hello world', 'Hello world'.length));
		phone.commands.insertContentAt(posIn(phone, 'Hello world'), 'Oh ');
		await settle();
		typeIn(editor, '!');
		expect(blocks(editor)).toEqual(['paragraph:Intro', 'paragraph:Oh Hello world!']);
		// Elsewhere on the page, as before.
		phone.commands.insertContentAt(posIn(phone, 'Intro'), 'An ');
		await settle();
		typeIn(editor, '?');
		expect(blocks(editor)).toEqual(['paragraph:An Intro', 'paragraph:Oh Hello world!?']);
	});

	it('an open `/` stays open when the phone writes earlier in its own paragraph', async () => {
		const { editor, phone } = twoDevices('<p>Intro</p><p>Hello world</p>');
		await settle();
		editor.commands.setTextSelection(posIn(editor, 'Hello world', 'Hello world'.length));
		typeIn(editor, ' /ta');
		await settle();
		expect(triggerState(editor.state, '/')).toMatchObject({ active: true, query: 'ta' });
		phone.commands.insertContentAt(posIn(phone, 'Hello world'), 'Oh ');
		await settle();
		expect(triggerState(editor.state, '/')).toMatchObject({ active: true, query: 'ta' });
		typeIn(editor, 'b');
		expect(blocks(editor)).toEqual(['paragraph:Intro', 'paragraph:Oh Hello world /tab']);
		expect(triggerState(editor.state, '/')).toMatchObject({ active: true, query: 'tab' });
	});

	it('a mention picked after the `@` closed replaces the `@` where it is now', async () => {
		const { editor, phone } = twoDevices('<h2>Plan for the week</h2><p>Lunch with @</p>');
		await settle();
		const at = posIn(editor, 'Lunch with @', 'Lunch with '.length);
		const picked = anchorRange(editor.state, at, at + 1);
		await phoneEditsAbove(phone, 'Plan for the week');
		insertEntity(
			editor,
			pickedRange(editor.state, '@', resolveRange(picked)!),
			{ name: 'Nick', url: '/person/person_1', entity_type: 'person' },
		);
		expect(blocks(editor)).toEqual([
			'paragraph:Added on the phone first.',
			'heading:Moved: Plan for the week',
			'paragraph:Lunch with  ',
		]);
		expect(editor.getJSON().content?.[2].content?.[1]).toMatchObject({ type: 'mention', attrs: { label: 'Nick' } });
	});
});

describe('a drag from one page to another', () => {
	const gallery = ['<p>Gallery</p>', ...Array.from({ length: 14 }, (_, i) => `<img src="https://images.example.com/${i + 1}.png" alt="${i + 1}.png">`), '<p>End</p>'].join('');

	function blocksOf(editor: Editor): string[] {
		const out: string[] = [];
		editor.state.doc.forEach((n) => out.push(n.type.name === 'image' ? String(n.attrs.alt) : n.textContent));
		return out;
	}

	/** Drag the block `index` of `from` (its DOM) and drop it at the end of `to`. */
	async function dragBlock(from: Editor, index: number, to: Editor) {
		const at = (() => {
			let pos = 0;
			for (let i = 0; i < index; i++) pos += from.state.doc.child(i).nodeSize;
			return pos;
		})();
		const dom = from.view.nodeDOM(at) as HTMLElement;
		from.view.posAtCoords = () => null;
		const end = to.state.doc.content.size;
		to.view.posAtCoords = () => ({ pos: end, inside: -1 });
		const data = new DataTransfer();
		const event = (type: string) => {
			const e = new DragEvent(type, { bubbles: true, cancelable: true });
			Object.defineProperty(e, 'dataTransfer', { value: data });
			return e;
		};
		dom.dispatchEvent(event('dragstart'));
		to.view.dom.dispatchEvent(event('drop'));
		dom.dispatchEvent(event('dragend'));
		await settle(60);
	}

	it('moves the image dragged, and only it, out of the page it came from', async () => {
		const a = bind(pageDoc(gallery).doc).editor;
		const b = bind(pageDoc('<p>Elsewhere</p>').doc).editor;
		await settle();
		a.view.dispatch(a.state.tr.setSelection(NodeSelection.create(a.state.doc, a.state.doc.child(0).nodeSize + a.state.doc.child(1).nodeSize)));
		expect((a.state.selection as NodeSelection).node.attrs.alt).toBe('2.png');
		await dragBlock(a, 2, b);
		const left = blocksOf(a);
		expect(left).toHaveLength(15);
		expect(left).not.toContain('2.png');
		expect(left).toContain('3.png');
		expect(blocksOf(b)).toEqual(['Elsewhere', '2.png']);
	});

	// Tiptap ran the paste rules on a drop from another editor as on text
	// pasted: a moved block gained a link nobody made and lost its `==`.
	it('a block dragged to another page keeps its words as they were: no link added, no marks made', async () => {
		const a = bind(pageDoc('<p>Edit main.py and say ==hi== today</p><p>Other</p>').doc).editor;
		const b = bind(pageDoc('<p>Elsewhere</p>').doc).editor;
		await settle();
		a.commands.setTextSelection({ from: posIn(a, 'Edit'), to: posIn(a, 'today', 'today'.length) });
		await dragBlock(a, 0, b);
		expect(blocksOf(b)).toEqual(['Elsewhere', 'Edit main.py and say ==hi== today']);
		const marks: string[] = [];
		b.state.doc.descendants((n) => {
			marks.push(...n.marks.map((m) => m.type.name));
			return true;
		});
		expect(marks).toEqual([]);
	});

	it('an image dragged without being selected leaves the words the page had selected', async () => {
		const a = bind(pageDoc(gallery).doc).editor;
		const b = bind(pageDoc('<p>Elsewhere</p>').doc).editor;
		await settle();
		a.commands.setTextSelection({ from: posIn(a, 'Gallery'), to: posIn(a, 'Gallery', 'Gallery'.length) });
		await dragBlock(a, 5, b);
		const left = blocksOf(a);
		expect(left[0]).toBe('Gallery');
		expect(left).toHaveLength(15);
		expect(left).not.toContain('5.png');
		expect(blocksOf(b)).toEqual(['Elsewhere', '5.png']);
	});
});

describe('undo', () => {
	function blocks(editor: Editor): string[] {
		const out: string[] = [];
		editor.state.doc.forEach((n) => out.push(`${n.type.name}:${n.textContent}`));
		return out;
	}

	/** The editor shows what the shared document holds. */
	function agree(editor: Editor, doc: Y.Doc) {
		expect(editor.state.doc.toJSON()).toEqual(treeOf(doc));
	}

	it('undoing typing made across pauses puts back what was there, the editor and the page agreeing', async () => {
		const { doc } = pageDoc('<p>Before</p><p>Select me</p>');
		const { editor } = bind(doc);
		await settle();
		const from = posIn(editor, 'Select me');
		editor.commands.setTextSelection({ from, to: from + 'Select me'.length });
		typeIn(editor, 'A');
		await settle(600);
		editor.commands.enter();
		typeIn(editor, 'second line');
		editor.commands.enter();
		typeIn(editor, 'third line');
		await settle(600);
		expect(() => editor.commands.undo()).not.toThrow();
		agree(editor, doc);
		expect(blocks(editor)).toEqual(['paragraph:Before', 'paragraph:A']);
		expect(() => editor.commands.undo()).not.toThrow();
		agree(editor, doc);
		expect(blocks(editor)).toEqual(['paragraph:Before', 'paragraph:Select me']);
		// What the person types next is written to the page as they see it.
		typeIn(editor, 'x');
		agree(editor, doc);
	});

	// A paste or an upload lands when the server answers, not when the person
	// acted: within y-undo's capture window of typing elsewhere it would join
	// that typing, and undoing the typing would take it too.
	it('a paste landing while the person types elsewhere is an undo step of its own', async () => {
		const { doc } = pageDoc('<p>Notes</p><p>Draft</p>');
		const { editor } = bind(doc);
		await settle();
		let answer: (v: { html: string }) => void = () => {};
		const done = pasteMarkdown(editor.view, '## Plan', posIn(editor, 'Notes', 5), () => new Promise((r) => (answer = r)));
		await settle(700);
		editor.commands.setTextSelection(posIn(editor, 'Draft', 5));
		typeIn(editor, ' typed');
		answer({ html: '<h2>Plan</h2>' });
		await done;
		expect(blocks(editor)).toEqual(['paragraph:Notes', 'heading:Plan', 'paragraph:Draft typed']);
		editor.commands.undo();
		expect(blocks(editor)).toEqual(['paragraph:Notes', 'paragraph:Draft typed']);
		editor.commands.undo();
		expect(blocks(editor)).toEqual(['paragraph:Notes', 'paragraph:Draft']);
		agree(editor, doc);
	});

	it('an upload landing while the person types elsewhere is an undo step of its own', async () => {
		const { doc } = pageDoc('<p>Notes</p><p>Draft</p>');
		const { editor } = bind(doc);
		await settle();
		let finish: (v: { url: string; filename: string; mime_type: string | null }) => void = () => {};
		const upload = () => new Promise<{ url: string; filename: string; mime_type: string | null }>((r) => (finish = r));
		const done = uploadFiles(editor.view, [new File(['x'], 'a.png', { type: 'image/png' })], posIn(editor, 'Notes', 5), upload);
		await settle(700);
		editor.commands.setTextSelection(posIn(editor, 'Draft', 5));
		typeIn(editor, ' typed');
		finish({ url: '/api/media/a.png', filename: 'a.png', mime_type: 'image/png' });
		await done;
		expect(blocks(editor)).toEqual(['paragraph:Notes', 'image:', 'paragraph:Draft typed']);
		editor.commands.undo();
		expect(blocks(editor)).toEqual(['paragraph:Notes', 'paragraph:Draft typed']);
		agree(editor, doc);
	});

	/** ⌘Z (Ctrl-Z off Apple keyboards), as the browser delivers it. */
	function pressUndo(editor: Editor) {
		const apple = /Mac|iP(hone|[oa]d)/.test(navigator.platform);
		editor.view.dom.dispatchEvent(
			new KeyboardEvent('keydown', { key: 'z', code: 'KeyZ', metaKey: apple, ctrlKey: !apple, bubbles: true, cancelable: true }),
		);
	}

	// What waits on the server is no edit yet: an undo that passed over it
	// took the person's own writing, and the landing, a new edit, then put
	// that writing beyond redo while the paste landed anyway.
	it('Undo while a paste waits calls it off; the writing before it stays, to undo and redo', async () => {
		const { doc } = pageDoc('<p>Notes</p>');
		const { editor } = bind(doc);
		await settle();
		editor.commands.setTextSelection(posIn(editor, 'Notes', 5));
		typeIn(editor, ' I wrote this sentence.');
		await settle(600);
		let answer: (v: { html: string }) => void = () => {};
		const done = pasteMarkdown(editor.view, '## Plan\n\n- one', editor.state.selection.from, () => new Promise((r) => (answer = r)));
		vi.mocked(toast).mockClear();
		pressUndo(editor);
		expect(blocks(editor)).toEqual(['paragraph:Notes I wrote this sentence.']);
		expect(editor.view.dom.querySelector('.doc-paste-pending')).toBeNull();
		expect(toast).toHaveBeenCalledWith('Undo stopped the paste');
		answer({ html: '<h2>Plan</h2><ul><li><p>one</p></li></ul>' });
		await done;
		expect(blocks(editor)).toEqual(['paragraph:Notes I wrote this sentence.']);
		pressUndo(editor);
		expect(blocks(editor)).toEqual(['paragraph:Notes']);
		expect(editor.commands.redo()).toBe(true);
		expect(blocks(editor)).toEqual(['paragraph:Notes I wrote this sentence.']);
		agree(editor, doc);
	});

	it('Undo while a file uploads calls it off, by key or by the platform’s undo; the writing before it stays, to undo and redo', async () => {
		for (const how of ['key', 'platform'] as const) {
			const { doc } = pageDoc('<p>Notes</p>');
			const { editor } = bind(doc);
			await settle();
			editor.commands.setTextSelection(posIn(editor, 'Notes', 5));
			typeIn(editor, ' and a paragraph I wrote.');
			await settle(600);
			let finish: (v: { url: string; filename: string; mime_type: string | null }) => void = () => {};
			const upload = () => new Promise<{ url: string; filename: string; mime_type: string | null }>((r) => (finish = r));
			const done = uploadFiles(editor.view, [new File(['x'], 'wrong.png', { type: 'image/png' })], editor.state.selection.from, upload);
			expect(pendingUploads(editor.view)).toBe(1);
			vi.mocked(toast).mockClear();
			if (how === 'key') pressUndo(editor);
			else editor.view.dom.dispatchEvent(new InputEvent('beforeinput', { inputType: 'historyUndo', bubbles: true, cancelable: true }));
			expect(pendingUploads(editor.view), how).toBe(0);
			expect(blocks(editor), how).toEqual(['paragraph:Notes and a paragraph I wrote.']);
			expect(toast).toHaveBeenCalledWith('Undo stopped uploading wrong.png');
			finish({ url: '/api/media/wrong.png', filename: 'wrong.png', mime_type: 'image/png' });
			await done;
			expect(blocks(editor), how).toEqual(['paragraph:Notes and a paragraph I wrote.']);
			pressUndo(editor);
			expect(blocks(editor), how).toEqual(['paragraph:Notes']);
			expect(editor.commands.redo(), how).toBe(true);
			expect(blocks(editor), how).toEqual(['paragraph:Notes and a paragraph I wrote.']);
			agree(editor, doc);
		}
	});

	it('Undo calls off the newest of a paste and an upload first, then undoes edits', async () => {
		const { doc } = pageDoc('<p>Notes</p><p>Draft</p>');
		const { editor } = bind(doc);
		await settle();
		let answer: (v: { html: string }) => void = () => {};
		const pasted = pasteMarkdown(editor.view, '## Plan', posIn(editor, 'Notes', 5), () => new Promise((r) => (answer = r)));
		let finish: (v: { url: string; filename: string; mime_type: string | null }) => void = () => {};
		const upload = () => new Promise<{ url: string; filename: string; mime_type: string | null }>((r) => (finish = r));
		const uploaded = uploadFiles(editor.view, [new File(['x'], 'a.png', { type: 'image/png' })], posIn(editor, 'Draft', 5), upload);
		pressUndo(editor);
		expect(pendingUploads(editor.view)).toBe(0);
		expect(editor.view.dom.querySelector('.doc-paste-pending')).not.toBeNull();
		finish({ url: '/api/media/a.png', filename: 'a.png', mime_type: 'image/png' });
		await uploaded;
		answer({ html: '<h2>Plan</h2>' });
		await pasted;
		expect(blocks(editor)).toEqual(['paragraph:Notes', 'heading:Plan', 'paragraph:Draft']);
		pressUndo(editor);
		expect(blocks(editor)).toEqual(['paragraph:Notes', 'paragraph:Draft']);
	});

	it('a markdown paste over a selection is one undo step, however long the server takes', async () => {
		const { doc } = pageDoc('<p>Before</p><p>Select me</p>');
		let answer: (v: { html: string }) => void = () => {};
		const convert = vi.fn(() => new Promise<{ html: string }>((r) => (answer = r)));
		const provider = fakeProvider(doc);
		cleanups.push(provider.destroy);
		const element = document.createElement('div');
		document.body.append(element);
		const editor = track(
			createPageEditor({ element, doc: { ydoc: doc, provider }, editable: true, placeholder: 'Write', plugins: [pasteHandling(convert)] }),
		);
		await settle();
		const from = posIn(editor, 'Select me');
		editor.commands.setTextSelection({ from, to: from + 'Select me'.length });
		const event = {
			clipboardData: { getData: (type: string) => (type === 'text/plain' ? '## Plan\n\n- one' : ''), files: [] },
			preventDefault() {},
		} as unknown as ClipboardEvent;
		expect(editor.view.someProp('handlePaste', (f) => f(editor.view, event, editor.state.doc.slice(0, 0)))).toBe(true);
		// The selection stays while the server converts.
		expect(blocks(editor)).toEqual(['paragraph:Before', 'paragraph:Select me']);
		await settle(700);
		answer({ html: '<h2>Plan</h2><ul><li><p>one</p></li></ul>' });
		await settle();
		expect(blocks(editor)).toEqual(['paragraph:Before', 'heading:Plan', 'bulletList:one']);
		agree(editor, doc);
		expect(() => editor.commands.undo()).not.toThrow();
		expect(blocks(editor)).toEqual(['paragraph:Before', 'paragraph:Select me']);
		agree(editor, doc);
		expect(() => editor.commands.undo()).not.toThrow();
		agree(editor, doc);
	});
});
