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
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';

/** Whether a page names each file, as the server answers (`getNamedUploads`). */
const server = vi.hoisted(() => ({ names: (_id: string): boolean => true }));

/** Files sent to the server, held until the test settles each (`sendPageFile`). */
const transfers = vi.hoisted(
	() =>
		[] as {
			id: string;
			file: File;
			done: () => void;
			fail: (error: Error) => void;
			progress?: (sent: number, total: number) => void;
		}[],
);

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
vi.mock('$lib/components/pages/nodes/FootnoteNode.svelte', () => ({ default: standIn('FootnoteNode') }));
vi.mock('$lib/components/pages/nodes/FigureNode.svelte', () => ({ default: standIn('FigureNode') }));
vi.mock('$lib/stores/contextMenu.svelte', () => ({ contextMenu: { show: vi.fn(), reachFirstRow: vi.fn() } }));
vi.mock('$lib/stores/linkEditor.svelte', () => ({ linkEditor: { show: vi.fn() } }));
vi.mock('$lib/stores/window-shell.svelte', () => ({
	windowShellStore: { openRouteBeside: vi.fn(), openTabFromRoute: vi.fn() },
}));
vi.mock('svelte-sonner', () => ({ toast: Object.assign(vi.fn(), { error: vi.fn() }) }));
vi.mock('$lib/api/client', async (orig) => ({
	...(await orig<typeof import('$lib/api/client')>()),
	getApplet: vi.fn(async (id: string) => ({ id, name: 'Sleep this week' })),
	sendPageFile: vi.fn(
		(id: string, file: File, progress?: (sent: number, total: number) => void) =>
			new Promise((resolve, reject) =>
				transfers.push({
					id,
					file,
					progress,
					done: () => resolve({ id, url: `/api/drive/files/${id}/download`, filename: file.name, mime_type: file.type }),
					fail: reject,
				}),
			),
	),
	getMediaUploads: vi.fn(async () => ({})),
	announcePageFiles: vi.fn(async () => undefined),
	getNamedUploads: vi.fn(async (ids: string[]) => Object.fromEntries(ids.map((id) => [id, server.names(id)]))),
}));

import { Editor } from '@tiptap/core';
import { NodeSelection, TextSelection } from '@tiptap/pm/state';
import { CellSelection } from '@tiptap/pm/tables';
import { GapCursor } from '@tiptap/pm/gapcursor';
import { yXmlFragmentToProseMirrorRootNode } from '@tiptap/y-tiptap';
import * as Y from 'yjs';
import { filterCommands, menuKey } from '$lib/components/menuCommand';
import { createPageEditor } from './editor';
import { contract, contractExtensions, ITEM_NUMBER, itemNumbers } from './schema';
import { appletCommands, insertEntity, moveBlock, moveUnitTo, treeCommands, unitAround, type TreeCommandHost } from './commands';
import { pickedRange, putBackQuery, triggers, triggerState } from './triggers';
import { fileIcon, parseRef } from '$lib/utils/refRoutes';
import { clearFind, find, findMatches, findNext, findPrevious, findState, selectionEchoes, setFindQuery } from './find';
import { focusMode } from './focus';
import { codeHighlight } from './code';
import { isCodeEditorHtml, looksLikeMarkdown, MAX_DEPTH, MAX_NODES, nodeCount, nodeDepth, pasteHandling, pasteMarkdown, wordLists, type Convert } from './paste';
import { addFiles, driveFileId, imagesOnTheirWay, kindOfFile, MAX_UPLOAD_BYTES, mediaSrc, mediaUploads, nameForBlock, nameOfSrc, type Send } from './media';
import { CHECK_AFTER_MS, resetUploads, retryUpload, TRY_AGAIN_EVERY_MS, uploadsAtStake, watchUpload, type LocalUpload } from './uploads';
import { convertNow, type ConvertResult } from './converter';
import { kindOfLink } from './media-kind';
import { decoded } from '$lib/utils/urlUtils';
import { svelteNodeView } from './nodeviews.svelte';
import { editLinkAtSelection, isRef, linkAt, linkMenu, links, openLink, openLinkBeside } from './links';
import { anchorAt, anchorRange, pinAt, placeOf, resolveRange } from './anchor';
import { linkEditor } from '$lib/stores/linkEditor.svelte';
import { toast } from 'svelte-sonner';
import { sendPageFile, UploadError, type Applet } from '$lib/api/client';
import { aiPresence, clearAi, flashBlocks, flashedBlocks, setAiCaret, setAiTrail } from './presence';
import { contextGesture } from './gesture';
import { acceptAllProposals, acceptProposal, proposalAt, proposalIds, rejectProposal } from './suggestions';
import { alignColumn, columnAlign, tableActions } from './tables';
import { scrollerOf, treeHeadings, treeOutlineNav } from './outline';
import { treeStats } from './stats';
import { getTreeEditor, registerTreeEditor, unregisterTreeEditor } from './registry';
import { fakeProvider, htmlEditor, pageDoc, posIn, settle, useRealConverter } from './test-utils';
import { get } from 'svelte/store';

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
		pasteHandling(),
		mediaUploads(sends),
		links({ onHover: () => {}, onLeave: () => {} }),
		aiPresence(),
		contextGesture([]),
	];
}

/** Files the page's editors were given, recorded instead of sent: tests that send them use the queue itself. */
const sends = vi.fn<Send>();

/** What the converter answers for markdown it makes `html` of. */
function converted(html: string): ConvertResult {
	return { kind: 'converted', html, notes: [] };
}

/** A converter that answers `answer` for anything. */
function answering(answer: ConvertResult): Convert & ReturnType<typeof vi.fn> {
	return vi.fn(() => answer);
}

/** The page's own converter, watched. */
function realConvert() {
	return vi.fn((markdown: string) => convertNow(markdown, 'paste'));
}

/** A file's block address: `/drive/` and a Drive id made in the browser. */
const UPLOAD_SRC = /^\/drive\/file_[0-9a-f]{16}$/;

const editors: Editor[] = [];
const cleanups: (() => void)[] = [];

beforeAll(() => useRealConverter());

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
	sends.mockClear();
	transfers.length = 0;
	server.names = () => true;
	resetUploads();
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

	// An article: its footnotes, a figure of each kind and veils, as the
	// server writes them when it moves a day page to blocks. Opening it
	// writes nothing, and each atom is drawn by its own view.
	it('opening an article holding footnotes, figures and veils writes nothing', async () => {
		const article = [
			'<p>A slow Saturday with <a href="/person/person_n"><virtues-veil>Nick</virtues-veil></a>.</p>',
			'<h2>Morning on the porch<virtues-footnote label="cx-1" note="8:40 AM–10:05 AM"></virtues-footnote></h2>',
			'<p>You read while it rained.<virtues-footnote label="ev-1" note="Recording · 8:40 AM · data_communication_transcription:tr_1"></virtues-footnote> ',
			'<strong><virtues-veil>Nick</virtues-veil></strong> was late.<virtues-footnote label="ev-2" note="Message · 9:12 AM · data_communication_message:msg_3"></virtues-footnote>',
			'<virtues-footnote label="ev-3"></virtues-footnote></p>',
			'<virtues-figure kind="quote" text="Save me a seat." who="Nick" ref="data_communication_message:msg_3" time="9:12 AM"></virtues-figure>',
			'<virtues-figure kind="thread" refs="data_communication_message:msg_8, data_communication_message:msg_9"></virtues-figure>',
			'<virtues-figure kind="route"></virtues-figure>',
			'<virtues-figure kind="picture" src="/api/drive/files/f_1/download" caption="Rain at ⟦the Blue Kettle⟧." data-style="watercolor" time="10:40 AM"></virtues-figure>',
			'<table><tbody><tr><th><p>Where</p></th></tr><tr><td><p><virtues-veil>the Blue Kettle</virtues-veil><virtues-footnote label="ev-4" note="Location · 7:15 PM"></virtues-footnote></p></td></tr></tbody></table>',
		].join('');
		const { doc, writtenBy } = pageDoc(article);
		const before = Y.encodeStateVector(doc);
		const writes: unknown[] = [];
		const { editor, provider } = bind(doc);
		doc.on('update', (_u: Uint8Array, origin: unknown) => {
			if (origin !== provider) writes.push(origin);
		});
		await settle(250);
		editor.commands.focus('end');
		editor.commands.blur();
		provider.emit('synced', true);
		await settle(50);
		expect(writes, `writes (page written by ${writtenBy})`).toEqual([]);
		expect(Y.encodeStateVector(doc)).toEqual(before);
		const names = mounted.map((m) => m.name);
		expect(names.filter((n) => n === 'FootnoteNode')).toHaveLength(5);
		expect(names.filter((n) => n === 'FigureNode')).toHaveLength(4);
		let veiled = 0;
		editor.state.doc.descendants((n) => {
			if (n.isText && n.marks.some((m) => m.type.name === 'veil')) veiled++;
			return true;
		});
		expect(veiled).toBe(3);
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

	// The page scrolls a box the editor sits in, with padding under it and
	// blank space under a short page: a click there blurred the editor.
	it('below the editor, on the blank page around it, ends in the last text too; a control there keeps its click', () => {
		const { doc } = pageDoc('<p>Intro</p><p>Last line</p>');
		const scroller = document.createElement('div');
		scroller.style.overflowY = 'auto';
		const page = document.createElement('div');
		const title = document.createElement('button');
		const element = document.createElement('div');
		page.append(title, element);
		scroller.append(page);
		document.body.append(scroller);
		const provider = fakeProvider(doc);
		cleanups.push(provider.destroy);
		const editor = track(createPageEditor({ element, doc: { ydoc: doc, provider }, editable: true, placeholder: 'Write' }));
		const last = editor.view.dom.lastElementChild!.getBoundingClientRect();
		const click = (on: Element) => {
			const down = new MouseEvent('mousedown', { button: 0, clientX: 10, clientY: last.bottom + 100, bubbles: true, cancelable: true });
			on.dispatchEvent(down);
			return down.defaultPrevented;
		};
		editor.commands.setTextSelection(1);
		expect(click(title), 'a control').toBe(false);
		expect(editor.state.selection.from).toBe(1);
		expect(click(page), 'the page under the editor').toBe(true);
		expect(editor.state.selection.from).toBe(posIn(editor, 'Last line', 9));
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

	describe('dropped from the drag handle', () => {
		const shape = (e: Editor) => {
			const out: string[] = [];
			e.state.doc.forEach((n) => out.push(n.type.name));
			return out;
		};

		it('lands where it is dropped with its id, words typed into a neighbour meanwhile staying in it, and one undo puts it back', async () => {
			const d = await twoDevices('<p>One</p><p>Two</p><p>Three</p>');
			const one = d.laptop.state.doc.child(0);
			const id = one.attrs[contract.id.attr];
			typeAtEnd(d.phone, 'Two', ' from phone');
			expect(moveUnitTo(d.laptop.view, 0, one.nodeSize, d.laptop.state.doc.content.size)).toBe(true);
			await d.exchange();
			for (const editor of [d.laptop, d.phone]) expect(texts(editor)).toEqual(['Two from phone', 'Three', 'One']);
			expect(d.laptop.state.doc.child(2).attrs[contract.id.attr]).toBe(id);
			expect(d.laptop.state.selection).toBeInstanceOf(NodeSelection);
			d.laptop.commands.undo();
			expect(texts(d.laptop)).toEqual(['One', 'Two from phone', 'Three']);
		});

		it('moves nothing when dropped inside itself or at either of its ends', async () => {
			const { laptop } = await twoDevices('<p>One</p><p>Two</p>');
			const size = laptop.state.doc.child(0).nodeSize;
			const before = laptop.state.doc;
			for (const target of [0, 2, size]) expect(moveUnitTo(laptop.view, 0, size, target)).toBe(false);
			expect(laptop.state.doc).toBe(before);
		});

		it('joins the two lists a block leaves side by side, which would export as two', async () => {
			const { laptop } = await twoDevices('<ul><li><p>A</p></li></ul><p>Gap</p><ul><li><p>B</p></li></ul><p>End</p>');
			const from = laptop.state.doc.child(0).nodeSize;
			moveUnitTo(laptop.view, from, from + laptop.state.doc.child(1).nodeSize, laptop.state.doc.content.size);
			expect(shape(laptop)).toEqual(['bulletList', 'paragraph', 'paragraph']);
			expect(laptop.state.doc.child(0).childCount).toBe(2);
			expect(texts(laptop)).toEqual(['A', 'B', 'End', 'Gap']);
		});

		it('puts a list item dropped outside its list in a list of its own, and joins it to a list it lands beside', async () => {
			const { laptop } = await twoDevices('<ul><li><p>A</p></li><li><p>B</p></li></ul><p>End</p>');
			const item = laptop.state.doc.child(0).child(0).nodeSize;
			moveUnitTo(laptop.view, 1, 1 + item, laptop.state.doc.content.size);
			expect(shape(laptop)).toEqual(['bulletList', 'paragraph', 'bulletList']);
			expect(texts(laptop)).toEqual(['B', 'End', 'A']);

			const { laptop: beside } = await twoDevices('<ul><li><p>A</p></li><li><p>B</p></li></ul><p>End</p>');
			const listEnd = beside.state.doc.child(0).nodeSize;
			moveUnitTo(beside.view, 1, 1 + item, listEnd);
			expect(shape(beside)).toEqual(['bulletList', 'paragraph']);
			expect(texts(beside)).toEqual(['B', 'A', 'End']);
		});

		it('takes away a list whose only item it moves', async () => {
			const { laptop } = await twoDevices('<ul><li><p>A</p></li></ul><p>B</p>');
			const item = laptop.state.doc.child(0).child(0).nodeSize;
			moveUnitTo(laptop.view, 1, 1 + item, laptop.state.doc.content.size);
			expect(shape(laptop)).toEqual(['paragraph', 'bulletList']);
			expect(texts(laptop)).toEqual(['B', 'A']);
		});

		it('takes the innermost list item under the pointer, or else the block at the top of the page', async () => {
			const { laptop } = await twoDevices('<p>Lead</p><ul><li><p>A</p><ul><li><p>B</p></li></ul></li></ul>');
			const { doc } = laptop.state;
			const nested = unitAround(doc, posIn(laptop, 'B', 0));
			expect(nested?.node.type.name).toBe('listItem');
			expect(nested?.node.textContent).toBe('B');
			expect(unitAround(doc, posIn(laptop, 'A', 0))?.node.textContent).toBe('AB');
			expect(unitAround(doc, posIn(laptop, 'Lead', 1))).toMatchObject({ from: 0, node: { type: { name: 'paragraph' } } });
		});
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

	// Text first would make `/` and Enter, and `/h` and Enter (`paragraph`
	// holds an h), turn a paragraph into a paragraph: they make Heading 1.
	it('`/` and Enter, `/h` and `/t` and Enter, make Heading 1', () => {
		for (const query of ['', 'h', 't']) {
			expect(filterCommands(commands, query)[0]?.label, `/${query}`).toBe('Heading 1');
		}
		expect(filterCommands(commands, 'text')[0]?.label).toBe('Text');
		expect(filterCommands(commands, 'paragraph')[0]?.label).toBe('Text');
	});

	it('matches letters typed in order, after every name that holds the query', () => {
		expect(filterCommands(commands, 'nmbr')[0]?.label).toBe('Numbered list');
		expect(filterCommands(commands, 'bul li')[0]?.label).toBe('Bulleted list');
		expect(filterCommands(commands, 'cdblk')[0]?.label).toBe('Code block');
		// A word's middle starts no match: `umb` is in no name's word start.
		expect(filterCommands(commands, 'umb').map((c) => c.label)).toEqual(['Numbered list']);
		expect(filterCommands(commands, 'mbr')).toEqual([]);
		// What holds the query comes first.
		expect(filterCommands(commands, 'li')[0]?.label).toBe('Bulleted list');
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

describe('typed emphasis', () => {
	/** The first paragraph's text runs, each with its mark names. */
	const runs = (editor: Editor) =>
		editor.state.doc.child(0).content.content.map((n) => [n.text, n.marks.map((m) => m.type.name)]);

	// Tiptap's own rules take any two delimiters, so `2 * 3 *` turned ` 3 `
	// italic. A delimiter with a space on its inner side is text, as the
	// paste rules and the converter read it.
	it.each(['2 * 3 * 4', 'a == b == c', 'x ** y **'])('leaves %s as text', (text) => {
		const editor = track(htmlEditor('<p></p>'));
		editor.commands.setTextSelection(1);
		typeIn(editor, text);
		expect(runs(editor)).toEqual([[text, []]]);
	});

	it.each([
		['*it*', 'it', 'italic'],
		['**b**', 'b', 'bold'],
		['~~s~~', 's', 'strike'],
		['==h==', 'h', 'highlight'],
	])('formats %s', (typed, text, mark) => {
		const editor = track(htmlEditor('<p></p>'));
		editor.commands.setTextSelection(1);
		typeIn(editor, typed);
		expect(runs(editor)).toEqual([[text, [mark]]]);
	});
});

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

describe('a list numbered its own way', () => {
	const YEARS =
		'<ol start="2019"><li><p>Moved</p></li><li value="2021"><p>Boat</p></li><li value="2024"><p>Sold</p></li></ol>';
	const enter = (e: Editor) =>
		e.view.dom.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }));
	/** Each item of the page's first block, a list: its number, its text. */
	const items = (e: Editor) => {
		const list = e.state.doc.child(0);
		const numbers = itemNumbers(list);
		const out: string[] = [];
		list.forEach((item, _, i) => out.push(`${numbers[i]}. ${item.textContent}`));
		return out;
	};
	/** The own numbers the shared document holds for the first list's items. */
	const shared = (doc: Y.Doc) =>
		(treeOf(doc).content![0].content ?? []).map((item: { attrs?: Record<string, unknown> }) => item.attrs?.[ITEM_NUMBER] ?? null);

	// A page whose list was numbered 2019, 2021, 2024 moved to blocks
	// numbered 2019, 2020, 2021. Each item keeps its number now, and Enter
	// starts an item that counts on from the one it is typed in.
	it('Enter at an item\'s end starts one counting on from it, every other item keeping its number', async () => {
		const { doc } = pageDoc(YEARS);
		const { editor } = bind(doc);
		await settle();
		editor.commands.setTextSelection(posIn(editor, 'Boat', 4));
		enter(editor);
		typeIn(editor, 'Fixed');
		expect(items(editor)).toEqual(['2019. Moved', '2021. Boat', '2022. Fixed', '2024. Sold']);
		expect(shared(doc)).toEqual([null, 2021, null, 2024]);
	});

	it('Enter at an item\'s start leaves it its number, the item made before it counting on from the one above', async () => {
		const { doc } = pageDoc(YEARS);
		const { editor } = bind(doc);
		await settle();
		editor.commands.setTextSelection(posIn(editor, 'Boat'));
		enter(editor);
		expect(items(editor)).toEqual(['2019. Moved', '2020. ', '2021. Boat', '2024. Sold']);
		expect(shared(doc)).toEqual([null, null, 2021, 2024]);
	});

	it('a number typed under it goes on with it only when it is the next one', () => {
		const next = track(htmlEditor(`${YEARS}<p></p>`));
		next.commands.setTextSelection(next.state.doc.content.size - 1);
		typeIn(next, '2025. Bought another');
		expect(next.state.doc.childCount).toBe(1);
		expect(items(next)).toEqual(['2019. Moved', '2021. Boat', '2024. Sold', '2025. Bought another']);
		const other = track(htmlEditor(`${YEARS}<p></p>`));
		other.commands.setTextSelection(other.state.doc.content.size - 1);
		typeIn(other, '2022. ');
		expect(other.state.doc.childCount).toBe(2);
		expect(other.state.doc.child(1).attrs.start).toBe(2022);
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

	it('converts markdown text in the page, in the page when the paste returns, where the paste was', () => {
		const convert = realConvert();
		const editor = track(htmlEditor('<p>Before</p><p></p>', [pasteHandling(convert)]));
		editor.commands.setTextSelection(editor.state.doc.content.size - 1);
		expect(paste(editor, clipboard({ 'text/plain': '## Plan\n\n- one' }))).toBe(true);
		expect(convert).toHaveBeenCalledWith('## Plan\n\n- one');
		expect(editor.getHTML()).toBe('<p>Before</p><h2>Plan</h2><ul><li><p>one</p></li></ul>');
	});

	it('pastes the text as it is when the converter cannot run, and says so once', () => {
		vi.mocked(toast).mockClear();
		const editor = track(htmlEditor('<p></p>', [pasteHandling(answering({ kind: 'unavailable' }))]));
		editor.commands.setTextSelection(1);
		paste(editor, clipboard({ 'text/plain': '- **bold** text' }));
		expect(editor.getHTML()).toBe('<p>- **bold** text</p>');
		paste(editor, clipboard({ 'text/plain': '- again' }));
		expect(editor.getHTML()).toBe('<p>- **bold** text- again</p>');
		expect(toast).toHaveBeenCalledOnce();
		expect(vi.mocked(toast).mock.calls[0][0]).toBe('Pasted as text');
	});

	it('pastes the text as it is when the converter refuses it, saying so when it was too much', () => {
		vi.mocked(toast).mockClear();
		const refused = track(htmlEditor('<p></p>', [pasteHandling(answering({ kind: 'refused', problems: [] }))]));
		refused.commands.setTextSelection(1);
		paste(refused, clipboard({ 'text/plain': '> deep' }));
		expect(refused.getHTML()).toBe('<p>&gt; deep</p>');
		expect(toast).not.toHaveBeenCalled();
		const large = track(htmlEditor('<p></p>', [pasteHandling(answering({ kind: 'too-large' }))]));
		large.commands.setTextSelection(1);
		paste(large, clipboard({ 'text/plain': '# Long\nlines' }));
		expect(large.getHTML()).toBe('<p># Long</p><p>lines</p>');
		expect(vi.mocked(toast).mock.calls.map((c) => c[0])).toEqual(['Pasted as text']);
		// The page's converter says so of more than one conversion takes, before reading any of it.
		expect(convertNow('x'.repeat(256 * 1024 + 1), 'paste')).toEqual({ kind: 'too-large' });
	});

	// A line of code holds `==`, `**` and `](` often enough, and the
	// converter reads `<Login/>` in it as a tag: only a block's sign makes
	// pasted text markdown, and the words of a line are the editor's paste
	// rules' to format.
	it('pastes code-like text as it is, never through the converter', () => {
		const convert = answering(converted('<p>lost</p>'));
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
			// A Python or shell comment's `# ` is a heading's sign.
			"# load the config\ndef __init__(self, *args, **kwargs):\n    self.path = base_dir + '/' + file_name\n",
			'# install\npip install foo_bar\nexport PATH=$HOME/.local/bin:$PATH  # *not* sudo',
			'# build\n$ cargo build --release\n- then run it',
			'- step\nconst total = items.length;',
			'# ship\ngit commit -m "Fix the date."',
			'# deploy\nsteps:\n  - run: |\n      npm ci',
			// Python: its names in two underscores, and its blocks.
			'# entry point\nif __name__ == "__main__":\n    main()',
			'# set up\nobj.__init__(x)',
			'# inspect\nprint(obj.__dict__)',
			'# each\nfor item in items:\n    print(item)',
			'# fall back\ntry:\n    run()\nexcept ValueError:\n    pass',
			// A diff's removed and added lines read as bullets of one kind.
			'@@ -1,3 +1,3 @@\n context line\n- old line\n+ new line',
			'--- a/notes.md\n+++ b/notes.md\n- old line\n+ new line',
			' keep this\n- old line\n+ new line',
			// A shell, YAML or Dockerfile comment, and C's directives.
			'# back up the photos\ntar -czf photos.tgz ~/Pictures\nrsync -av photos.tgz nas:/backups/',
			'# Clean the build\nrm -rf dist\nmake',
			'# Connect\nssh pi@host\ncd ~/app',
			'# CI\non: push\njobs:\n  build:\n    steps:\n      - run: make',
			'# Build stage\nFROM node:20 AS build\nRUN npm ci',
			'# Logs\n*.log\nnode_modules/',
			'#define MAX 10\n# define MIN 0',
			'# users who signed in\nSELECT name FROM users WHERE active = true',
			'# list them\nGet-ChildItem -Path C:\\Users',
			'# the path\n$env:PATH = "C:\\bin"',
			'# load it\nlibrary(ggplot2)\nx <- c(1, 2, 3)',
			// A terminal's prompt is no quote.
			'> npm start\n\nserver listening on 3000',
		]) {
			expect(looksLikeMarkdown(text), text).toBe(false);
			expect(paste(editor, clipboard({ 'text/plain': text })), text).toBe(false);
		}
		expect(convert).not.toHaveBeenCalled();
		for (const text of [
			'## Plan',
			'- one',
			'1. one',
			'> quoted',
			'```js\nx\n```',
			'| a | b |',
			'  - nested',
			'## Setup\n\n```sh\nexport PATH=$HOME/bin:$PATH\nnpm install\n```\n\n- Paris (France)\n- Rome',
			"# Title\n\nHere's how:\n\n1. Open it",
			// Chat's answers: a fence nested in a list item, or under item 10
			// or later, is indented four spaces or more; code indented inside
			// an item is the item's; prose may open with a command's word.
			'## Setup\n\n- Install the tools:\n  - Node:\n\n    ```sh\n    npm install\n    ```\n\nThen **restart** the app.',
			'10. Install the deps:\n\n    ```sh\n    pnpm install\n    ```',
			'1. Run it:\n\n        npm install\n\n2. Open the **app**',
			'## Why\n\ngit keeps every version, so **nothing** is lost.',
			'## Result\n\nresult = good, so **ship** it.',
			// Bold in underscores, a dunder in inline code, a sentence ending in a colon.
			'## Note\n\nUse __bold__ here.',
			'## API\n\nCall `obj.__init__()` once, and `x == "__main__"` never.',
			'## Steps\n\nfor example:\n  - one\n  - two',
			// Notes the code check must leave alone.
			'# Shopping\nmilk\neggs\nbread',
			'# Notes\nCall Nick at 5',
			'Shopping:\n  - milk: 2 liters\n  - eggs',
			'> git keeps every version, so nothing is lost.',
			'## Links\nhttps://example.com/x',
			'- milk\n- eggs\n\nMake sure to buy bread',
			'---\ntitle: Trip\ntags:\n  place: Lisbon\n---\n\n# Trip',
		]) {
			expect(looksLikeMarkdown(text), text).toBe(true);
		}
	});

	// Code copied as plain text came through the converter for one `# `
	// comment line: its lines joined into one, and `__init__` bold.
	it('pastes code with a comment line as its lines, as written', () => {
		const editor = track(htmlEditor('<p></p>', [pasteHandling(realConvert())]));
		const lines = (text: string) => {
			editor.commands.setContent('<p></p>');
			editor.commands.setTextSelection(1);
			pasteEvent(editor, { 'text/plain': text });
			const out: string[] = [];
			editor.state.doc.forEach((n) => out.push(`${n.type.name}:${n.textContent}`));
			return out;
		};
		expect(lines("# load the config\ndef __init__(self, *args, **kwargs):\n    self.path = base_dir")).toEqual([
			'paragraph:# load the config',
			'paragraph:def __init__(self, *args, **kwargs):',
			'paragraph:    self.path = base_dir',
		]);
		// Python's idiom came in as a heading and `if name == "main":`, bold.
		expect(lines('# entry point\nif __name__ == "__main__":\n    main()')).toEqual([
			'paragraph:# entry point',
			'paragraph:if __name__ == "__main__":',
			'paragraph:    main()',
		]);
		expect(lines('# set up\nobj.__init__(x)')).toEqual(['paragraph:# set up', 'paragraph:obj.__init__(x)']);
		// A diff's removed and added lines came in as two bullets alike.
		expect(lines('@@ -1,3 +1,3 @@\n context line\n- old line\n+ new line')).toEqual([
			'paragraph:@@ -1,3 +1,3 @@',
			'paragraph: context line',
			'paragraph:- old line',
			'paragraph:+ new line',
		]);
		// YAML lost its indents, and its comment became a heading.
		expect(lines('# CI\njobs:\n  build:\n    steps:')).toEqual(['paragraph:# CI', 'paragraph:jobs:', 'paragraph:  build:', 'paragraph:    steps:']);
	});

	// Text misread as markdown could only be undone: no paste put it in as
	// written. Shift with the paste (Paste and Match Style) does.
	it('pastes text as written, never converted nor formatted, with Shift held', () => {
		const convert = realConvert();
		const editor = track(htmlEditor('<p></p>', [pasteHandling(convert)]));
		const pasted = (text: string, shift: boolean) => {
			editor.commands.setContent('<p></p>');
			editor.commands.setTextSelection(1);
			// What ProseMirror reads of the keys (`view.input`), as a keydown sets it.
			editor.view.dom.dispatchEvent(new KeyboardEvent('keydown', { key: shift ? 'Shift' : 'v', keyCode: shift ? 16 : 86, shiftKey: shift, bubbles: true }));
			pasteEvent(editor, { 'text/plain': text });
			return editor.getHTML();
		};
		expect(pasted('## Plan\n- milk **now**', true)).toBe('<p>## Plan</p><p>- milk **now**</p>');
		expect(convert).not.toHaveBeenCalled();
		expect(pasted('Run **this** first', true)).toBe('<p>Run **this** first</p>');
		expect(pasted('## Plan\n- milk **now**', false)).toBe('<h2>Plan</h2><ul><li><p>milk <strong>now</strong></p></li></ul>');
		expect(convert).toHaveBeenCalledOnce();
	});

	// A note with one bullet in it came through as one item holding every
	// line after it, run together.
	it('pastes markdown with each line kept a line, never run on into an item', () => {
		const editor = track(htmlEditor('<p></p>', [pasteHandling(realConvert())]));
		editor.commands.setTextSelection(1);
		expect(paste(editor, clipboard({ 'text/plain': 'Shopping\n- milk\nCall Nick at 5\nBring receipts' }))).toBe(true);
		expect(editor.getHTML()).toBe('<p>Shopping</p><ul><li><p>milk</p></li></ul><p>Call Nick at 5<br>Bring receipts</p>');
	});

	// A page filled to MAX_NODES takes about a second to build here and over
	// five on a shared CI runner, past vitest's default of five.
	const FULL_PAGE_MS = 30_000;

	// A paste too large to convert went in as a paragraph a line, past what
	// a page holds: the server refused the edit, and the page stopped saving.
	it('refuses a paste that would take the page past what it holds, however it would have landed', () => {
		vi.mocked(toast).mockClear();
		const lines = Array.from({ length: MAX_NODES / 2 + 10 }, (_, i) => `- ${i}`).join('\n');
		const large = track(htmlEditor('<p>Keep</p>', [pasteHandling(answering({ kind: 'too-large' }))]));
		large.commands.setTextSelection(5);
		expect(paste(large, clipboard({ 'text/plain': lines }))).toBe(true);
		expect(large.getHTML()).toBe('<p>Keep</p>');
		expect(vi.mocked(toast).mock.calls.map((c) => c[0])).toEqual(['Not pasted']);

		// ProseMirror's own paste of plain text, a paragraph a line.
		vi.mocked(toast).mockClear();
		const words = Array.from({ length: MAX_NODES / 2 + 10 }, (_, i) => `line ${i}`).join('\n');
		const plain = track(htmlEditor('<p>Keep</p>', [pasteHandling()]));
		plain.commands.setTextSelection(5);
		pasteEvent(plain, { 'text/plain': words });
		expect(plain.getHTML()).toBe('<p>Keep</p>');
		expect(vi.mocked(toast).mock.calls.map((c) => c[0])).toEqual(['Not pasted']);

		// What fits still lands.
		pasteEvent(plain, { 'text/plain': 'one\ntwo' });
		expect(plain.getHTML()).toBe('<p>Keepone</p><p>two</p>');
		expect(nodeCount(plain.state.doc)).toBe(4);
	}, FULL_PAGE_MS);

	// Files went in past what a page holds, with nothing said, and their
	// upload began: the server refused the edit, and the page stopped saving.
	it('refuses files pasted, dropped or picked that would take the page past what it holds', () => {
		const photo = () => new File(['x'], 'harbour.png', { type: 'image/png' });
		const editor = track(htmlEditor('<p>x</p>'.repeat(MAX_NODES / 2), [pasteHandling(), mediaUploads(sends)]));
		expect(nodeCount(editor.state.doc)).toBe(MAX_NODES);
		const before = editor.state.doc;
		editor.view.posAtCoords = () => ({ pos: 1, inside: -1 });
		const dropped = {
			dataTransfer: { files: [photo()], getData: () => '' },
			clientX: 0,
			clientY: 0,
			preventDefault() {},
		} as unknown as DragEvent;
		const cases: [string, () => void][] = [
			['pasted', () => void paste(editor, clipboard({}, [photo()]))],
			['dropped', () => void editor.view.someProp('handleDrop', (f) => f(editor.view, dropped, editor.state.doc.slice(0, 0), false))],
			['picked', () => addFiles(editor.view, [photo()], 1, { send: sends })],
		];
		for (const [how, give] of cases) {
			vi.mocked(toast).mockClear();
			editor.commands.setTextSelection(1);
			give();
			expect(editor.state.doc.eq(before), how).toBe(true);
			expect(sends, how).not.toHaveBeenCalled();
			expect(vi.mocked(toast).mock.calls.map((c) => c[0]), how).toEqual(['Not added']);
		}

		// What fits still goes in.
		const room = track(htmlEditor('<p>x</p>', [pasteHandling(), mediaUploads(sends)]));
		addFiles(room.view, [photo()], 1, { send: sends });
		expect(sends).toHaveBeenCalledOnce();
	}, FULL_PAGE_MS);

	// A paste nesting lists past the levels a page holds went in, the server
	// refused the edit, and the page stopped saving with nothing said.
	it('refuses a paste or a drop that would nest the page deeper than it holds', () => {
		vi.mocked(toast).mockClear();
		const deep = (lists: number) => '<ul><li><p>x</p>'.repeat(lists) + '</li></ul>'.repeat(lists);
		const editor = track(htmlEditor('<p>hello</p>', [pasteHandling(), mediaUploads(sends)]));
		editor.commands.setTextSelection(6);
		pasteEvent(editor, { 'text/html': deep(60), 'text/plain': 'x' });
		expect(editor.getHTML()).toBe('<p>hello</p>');
		expect(vi.mocked(toast).mock.calls.map((c) => c[0])).toEqual(['Not pasted']);
		expect(nodeDepth(editor.state.doc)).toBe(1);

		// What the server takes still lands: a list is two levels, its text one more.
		vi.mocked(toast).mockClear();
		pasteEvent(editor, { 'text/html': deep(49), 'text/plain': 'x' });
		expect(nodeDepth(editor.state.doc)).toBeGreaterThan(90);
		expect(nodeDepth(editor.state.doc)).toBeLessThanOrEqual(MAX_DEPTH);
		expect(toast).not.toHaveBeenCalled();
		expect(nodeDepth(track(htmlEditor('<p>Lunch with <virtues-mention to="/person/person_1" label="Nick"></virtues-mention></p>')).state.doc)).toBe(2);
	});

	it("counts a page's depth against the crate's own limit", () => {
		const model = fs.readFileSync(path.resolve(HERE, '../../../../../crates/virtues-document/src/model.rs'), 'utf8');
		expect(model).toContain(`pub const MAX_DEPTH: usize = ${MAX_DEPTH};`);
	});

	it("counts a page's nodes against the crate's own limit", () => {
		const model = fs.readFileSync(path.resolve(HERE, '../../../../../crates/virtues-document/src/model.rs'), 'utf8');
		expect(model).toContain(`pub const MAX_NODES: usize = ${MAX_NODES.toLocaleString('en').replace(/,/g, '_')};`);
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
		const editor = track(htmlEditor('<p></p>', [pasteHandling(), mediaUploads(sends)]));
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
			// Python's names in two underscores are no bold.
			'print(obj.__dict__)',
			'obj.__class__.__name__',
			'if __name__ == "__main__":',
		]) {
			expect(landed({ 'text/plain': text }).replace(/<\/?code>/g, '`'), text).toBe(`<p>${text}</p>`);
		}
		expect(landed({ 'text/plain': 'Run **this** first, *then* ==that== and ~~not~~ `code`' })).toBe(
			'<p>Run <strong>this</strong> first, <em>then</em> <mark>that</mark> and <s>not</s> <code>code</code></p>',
		);
		expect(landed({ 'text/plain': 'Use __bold__ here' })).toBe('<p>Use <strong>bold</strong> here</p>');
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

	it('reads a paste as a paste, which keeps its text', () => {
		const editor = track(htmlEditor('<p></p>', [pasteHandling()]));
		editor.commands.setTextSelection(1);
		expect(paste(editor, clipboard({ 'text/plain': '- remember <placeholder> here' }))).toBe(true);
		expect(editor.getHTML()).toBe('<ul><li><p>remember &lt;placeholder&gt; here</p></li></ul>');
	});

	/** VS Code's clipboard HTML: its syntax colours as styled spans, a token's bold included. */
	const VSCODE = (lines: string[]) =>
		'<meta charset="utf-8"><div style="color: #d4d4d4;background-color: #1e1e1e;font-family: Menlo, monospace;font-weight: normal;font-size: 12px;line-height: 18px;white-space: pre;">'
		+ lines.map((l) => (l ? `<div><span style="color: #569cd6;font-weight: bold;">${l}</span></div>` : '<br>')).join('')
		+ '</div>';

	it("reads a code editor's HTML as the text it shows: markdown converted, code without its colours' bold", () => {
		expect(isCodeEditorHtml(VSCODE(['## Plan']))).toBe(true);
		expect(isCodeEditorHtml('<div style="white-space: pre"><b>Bold</b></div>')).toBe(false);
		expect(isCodeEditorHtml('<p><span style="font-weight:700">Bold</span></p>')).toBe(false);

		const convert = realConvert();
		const editor = track(htmlEditor('<p></p>', [pasteHandling(convert)]));
		editor.commands.setTextSelection(1);
		expect(paste(editor, clipboard({ 'text/plain': '## Plan\n\n- one', 'text/html': VSCODE(['## Plan', '', '- one']) }))).toBe(true);
		expect(convert).toHaveBeenCalledWith('## Plan\n\n- one');
		expect(editor.getHTML()).toBe('<h2>Plan</h2><ul><li><p>one</p></li></ul>');

		const code = track(htmlEditor('<p></p>', [pasteHandling(convert)]));
		code.commands.setTextSelection(1);
		expect(paste(code, clipboard({ 'text/plain': 'const a = 1;\nconst b = 2;', 'text/html': VSCODE(['const a = 1;', 'const b = 2;']) }))).toBe(true);
		expect(code.getHTML()).toBe('<p>const a = 1;</p><p>const b = 2;</p>');
		expect(convert).toHaveBeenCalledTimes(1);
	});

	it("leaves VS Code's own data to the code block paste, unless it says markdown", () => {
		const convert = realConvert();
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
		expect(md.getHTML()).toBe('<h2>Plan</h2>');

		// A markdown file's prose has no block's sign, and the code block
		// paste made a markdown code block of it.
		for (const [text, html] of [
			['Read the **setup** guide first.', '<p>Read the <strong>setup</strong> guide first.</p>'],
			['First one.\n\nThen a [link](https://example.com).', '<p>First one.</p><p>Then a <a href="https://example.com">link</a>.</p>'],
		]) {
			const prose = track(htmlEditor('<p></p>', [pasteHandling(convert)]));
			prose.commands.setTextSelection(1);
			const pasted = clipboard({ 'text/plain': text, 'text/html': VSCODE(text.split('\n')), 'vscode-editor-data': markdown });
			expect(handledBy(prose, pasted), text).toBe(true);
			expect(prose.getHTML(), text).toBe(html);
			expect(prose.state.doc.firstChild?.type.name, text).toBe('paragraph');
		}

		// Code copied out of a fence in a README: VS Code names the file's
		// language, not the fence's. It reads as code, so it stays as it is,
		// a code block: converted, `__init__` turned bold, every `# ` a
		// heading and each line's indent gone.
		for (const text of [
			'class Config:\n    def __init__(self, *args, **kwargs):\n        # keep the defaults\n        self.items = [*args]',
			'# install the tools\nnpm install --save-dev vitest\n# then run\nnpx vitest run',
		]) {
			const code = track(htmlEditor('<p></p>', [pasteHandling(convert)]));
			code.commands.setTextSelection(1);
			const pasted = clipboard({ 'text/plain': text, 'text/html': VSCODE(text.split('\n')), 'vscode-editor-data': markdown });
			expect(handledBy(code, pasted), text).toBe(true);
			expect(code.state.doc.firstChild?.type.name, text).toBe('codeBlock');
			expect(code.state.doc.firstChild?.textContent, text).toBe(text);
		}
		expect(convert).toHaveBeenCalledTimes(3);
	});

	/** Whether any paste handler takes `event`, as ProseMirror asks each in turn: Tiptap's code block paste after the page's. */
	function handledBy(editor: Editor, event: ClipboardEvent): boolean {
		return !!editor.view.someProp('handlePaste', (f) => (f === undefined ? false : f(editor.view, event, editor.state.doc.slice(0, 0))));
	}

	it('leaves plain words and HTML to the editor, and pastes raw into a code block', () => {
		const convert = realConvert();
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
		const editor = track(htmlEditor('<pre data-language="python"><code>x = 1</code></pre>', [pasteHandling()]));
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
		const convert = realConvert();
		const editor = track(htmlEditor('<p>Start</p>', [pasteHandling(convert)]));
		dropAt(editor, posIn(editor, 'Start', 5), { 'text/plain': '## Plan\n\n- one' });
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

	it('two pastes at one caret go in in the order made', async () => {
		for (const shape of ['plain', 'shared'] as const) {
			const editor =
				shape === 'plain' ? track(htmlEditor('<p>Start</p><p>End</p>', [pasteHandling()])) : bind(pageDoc('<p>Start</p><p>End</p>').doc).editor;
			await settle();
			editor.commands.setTextSelection(posIn(editor, 'Start', 5));
			paste(editor, clipboard({ 'text/plain': '## First\n\n- a' }));
			paste(editor, clipboard({ 'text/plain': '## Second\n\n- b' }));
			const out: string[] = [];
			editor.state.doc.forEach((n) => out.push(`${n.type.name}:${n.textContent}`));
			expect(out, shape).toEqual(['paragraph:Start', 'heading:First', 'bulletList:a', 'heading:Second', 'bulletList:b', 'paragraph:End']);
		}
	});

	it('two runs of words pasted at one caret join the text in the order made', () => {
		const editor = track(htmlEditor('<p>Start</p>', [pasteHandling()]));
		editor.commands.setTextSelection(posIn(editor, 'Start', 5));
		pasteMarkdown(editor.view, '**one**', editor.state.selection.from);
		pasteMarkdown(editor.view, '*two*', editor.state.selection.from);
		expect(editor.getHTML()).toBe('<p>Start<strong>one</strong><em>two</em></p>');
	});

	describe('blocks arrive whole, an inline run joins the text', () => {
		const HEADING_AND_LIST = '<h2>Pasted heading</h2><ul><li><p><strong>one</strong></p></li><li><p>two</p></li></ul>';
		const LIST = '<ul><li><p>one</p></li><li><p>two</p></li></ul>';

		/**
		 * Paste markdown the server converts to `html`, the caret `offset` into
		 * the block holding `text`, or at the position `text` is.
		 */
		async function pasteInto(page: string, text: string | number, offset: number, html: string) {
			const editor = track(htmlEditor(page, [pasteHandling(answering(converted(html)))]));
			const at = typeof text === 'number' ? text : posIn(editor, text, offset);
			editor.commands.setTextSelection(at);
			pasteMarkdown(editor.view, 'markdown', at, { convert: answering(converted(html)) });
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

		// Items numbered their own way keep their numbers either side of the
		// paste: the second list starts at its first item's, which keeps its
		// own, so a list pasted between that joins both still leaves it its
		// number; the second half of a split item counts on from the first.
		it('in a list numbered its own way, every item keeping its number', async () => {
			const years = '<ol start="2019"><li><p>a</p></li><li value="2021"><p>b</p></li><li value="2024"><p>c</p></li></ol>';
			const between = await pasteInto(years, 'a', 1, '<h2>Mid</h2>');
			expect(between.getHTML()).toBe(
				'<ol start="2019"><li><p>a</p></li></ol><h2>Mid</h2><ol start="2021"><li value="2021"><p>b</p></li><li value="2024"><p>c</p></li></ol>',
			);
			const joined = await pasteInto(years, 'a', 1, '<ol><li><p>x</p></li></ol>');
			expect(joined.getHTML()).toBe(
				'<ol start="2019"><li><p>a</p></li><li><p>x</p></li><li value="2021"><p>b</p></li><li value="2024"><p>c</p></li></ol>',
			);
			const inside = await pasteInto(years.replace('<p>b</p>', '<p>beta</p>'), 'beta', 2, '<h2>Mid</h2>');
			expect(inside.getHTML()).toBe(
				'<ol start="2019"><li><p>a</p></li><li value="2021"><p>be</p></li></ol><h2>Mid</h2>'
					+ '<ol start="2022"><li><p>ta</p></li><li value="2024"><p>c</p></li></ol>',
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
	const photos = (...names: string[]) => names.map((n) => new File(['x'], n, { type: 'image/png' }));

	/** Each top-level block: its type, and its text, or its name with its address's shape. */
	function blocks(editor: Editor): string[] {
		const out: string[] = [];
		editor.state.doc.forEach((n) => {
			if (!n.isAtom) return void out.push(`${n.type.name}:${n.textContent}`);
			const src = String(n.attrs.src);
			out.push(`${n.type.name}:${n.attrs.alt ?? n.attrs.name}${UPLOAD_SRC.test(src) ? '' : ` at ${src}`}`);
		});
		return out;
	}

	/** The Drive ids the page's file blocks name, in document order. */
	function fileIds(editor: Editor): string[] {
		const out: string[] = [];
		editor.state.doc.descendants((n) => {
			if (n.isAtom && typeof n.attrs.src === 'string' && UPLOAD_SRC.test(n.attrs.src)) out.push(driveFileId(n.attrs.src)!);
			return true;
		});
		return out;
	}

	it('an audio file becomes its audio block at once, at its final address, and the file is sent after', () => {
		const send = vi.fn<Send>();
		const editor = track(htmlEditor('<p>Notes</p>', [mediaUploads(send)]));
		addFiles(editor.view, [new File(['abc'], 'memo.m4a', { type: 'audio/mp4' })], editor.state.doc.content.size - 1, { send });
		expect(blocks(editor)).toEqual(['paragraph:Notes', 'audio:memo.m4a']);
		const [id] = fileIds(editor);
		expect(editor.getJSON().content?.[1]).toMatchObject({ type: 'audio', attrs: { src: `/drive/${id}`, name: 'memo.m4a' } });
		expect(send).toHaveBeenCalledOnce();
		expect(send.mock.calls[0][0]).toBe(id);
		expect(send.mock.calls[0][1].name).toBe('memo.m4a');
	});

	it('several files on an empty line all go in there, in the order given, each at an address of its own', () => {
		const editor = track(htmlEditor('<p>Trip</p><p></p><p>After</p>', [mediaUploads(sends)]));
		// The Image command's flow: the trigger deleted, the place anchored while the dialog is open.
		const at = anchorAt(editor.state, posIn(editor, 'Trip') + 6);
		addFiles(editor.view, photos('harbour.png', 'tram.png', 'castle.png'), at, { send: sends });
		expect(blocks(editor)).toEqual(['paragraph:Trip', 'image:harbour.png', 'image:tram.png', 'image:castle.png', 'paragraph:After']);
		const ids = fileIds(editor);
		expect(new Set(ids).size).toBe(3);
		expect(sends.mock.calls.map(([id, file]) => [id, file.name])).toEqual([
			[ids[0], 'harbour.png'],
			[ids[1], 'tram.png'],
			[ids[2], 'castle.png'],
		]);
	});

	it('several files at the end of a line go after it in order, and in the middle of one, between its halves', () => {
		const atEnd = track(htmlEditor('<p>Notes</p><p>After</p>', [mediaUploads(sends)]));
		addFiles(atEnd.view, photos('one.png', 'two.png', 'three.png'), posIn(atEnd, 'Notes', 5), { send: sends });
		expect(blocks(atEnd)).toEqual(['paragraph:Notes', 'image:one.png', 'image:two.png', 'image:three.png', 'paragraph:After']);

		const middle = track(htmlEditor('<p>Notes More</p>', [mediaUploads(sends)]));
		addFiles(middle.view, photos('one.png', 'two.png'), posIn(middle, 'More'), { send: sends });
		expect(blocks(middle)).toEqual(['paragraph:Notes ', 'image:one.png', 'image:two.png', 'paragraph:More']);
	});

	it('on a shared page, several files in the middle of a line go between its halves, in order, every block with an id of its own', async () => {
		const { doc, writtenBy } = pageDoc('<p>Notes More</p><p>After</p>');
		const { editor } = bind(doc);
		await settle();
		addFiles(editor.view, photos('one.png', 'two.png', 'three.png'), posIn(editor, 'More'), { send: sends });
		await settle();
		expect(blocks(editor), `page written by ${writtenBy}`).toEqual([
			'paragraph:Notes ',
			'image:one.png',
			'image:two.png',
			'image:three.png',
			'paragraph:More',
			'paragraph:After',
		]);
		const ids = editor.state.doc.content.content.map((n) => n.attrs[contract.id.attr]);
		expect(new Set(ids).size).toBe(ids.length);
		expect(treeOf(doc)).toEqual(editor.state.doc.toJSON());
	});

	/** A paste of `files` alone, as a copied picture is. */
	function pasteFiles(editor: Editor, files: File[]): boolean {
		const event = { clipboardData: { getData: () => '', files }, preventDefault() {} } as unknown as ClipboardEvent;
		return !!editor.view.someProp('handlePaste', (f) => f(editor.view, event, editor.state.doc.slice(0, 0)));
	}

	it('a file pasted over words replaces them at once, and one undo puts them back', async () => {
		const { doc } = pageDoc('<p>Before</p><p>Keep these words</p>');
		const { editor } = bind(doc);
		await settle();
		const from = posIn(editor, 'these');
		editor.commands.setTextSelection({ from, to: from + 'these'.length });
		expect(pasteFiles(editor, photos('new.png'))).toBe(true);
		expect(blocks(editor)).toEqual(['paragraph:Before', 'paragraph:Keep ', 'image:new.png', 'paragraph: words']);
		editor.commands.undo();
		expect(blocks(editor)).toEqual(['paragraph:Before', 'paragraph:Keep these words']);
		expect(treeOf(doc)).toEqual(editor.state.doc.toJSON());
	});

	it('a file pasted over a selected image replaces it at once, and undo puts the image back', async () => {
		const { doc } = pageDoc('<p>A</p><img src="/api/media/old.png" alt="Old"><p>B</p>');
		const { editor } = bind(doc);
		await settle();
		editor.view.dispatch(editor.state.tr.setSelection(NodeSelection.create(editor.state.doc, editor.state.doc.child(0).nodeSize)));
		expect(pasteFiles(editor, photos('new.png'))).toBe(true);
		expect(blocks(editor)).toEqual(['paragraph:A', 'image:new.png', 'paragraph:B']);
		editor.commands.undo();
		expect(blocks(editor)).toEqual(['paragraph:A', 'image:Old at /api/media/old.png', 'paragraph:B']);
	});

	it('a picked file whose line someone deleted while the dialog was open goes in where the line was', async () => {
		vi.mocked(toast).mockClear();
		const { editor } = bind(pageDoc('<p>Before</p><p>Draft line.</p><p>End</p>').doc);
		await settle();
		const at = anchorAt(editor.state, posIn(editor, 'Draft', 2));
		const from = posIn(editor, 'Draft line.') - 1;
		editor.commands.deleteRange({ from, to: from + editor.state.doc.child(1).nodeSize });
		editor.commands.setTextSelection(posIn(editor, 'End', 3));
		addFiles(editor.view, photos('one.png', 'two.png'), at, { send: sends });
		expect(blocks(editor)).toEqual(['paragraph:Before', 'image:one.png', 'image:two.png', 'paragraph:End']);
		expect(toast).not.toHaveBeenCalled();
		// A place no longer in the page at all: the cursor, said.
		addFiles(editor.view, photos('three.png'), { resolve: () => null }, { send: sends });
		expect(blocks(editor)).toEqual(['paragraph:Before', 'image:one.png', 'image:two.png', 'paragraph:End', 'image:three.png']);
		expect(toast).toHaveBeenCalledOnce();
	});

	// Word, Excel and PowerPoint copy a picture of the range beside its HTML.
	it("pastes Office's HTML, not the picture of it beside it; a copied picture becomes its block", async () => {
		const editor = track(htmlEditor('<p>Notes</p>', [pasteHandling(), mediaUploads(sends)]));
		editor.commands.setTextSelection(posIn(editor, 'Notes', 5));
		const office = new DataTransfer();
		office.setData('text/html', '<table><tr><td>Q1</td><td>40</td></tr></table>');
		office.setData('text/plain', 'Q1\t40');
		office.items.add(new File(['x'], 'image.png', { type: 'image/png' }));
		editor.view.dom.dispatchEvent(new ClipboardEvent('paste', { clipboardData: office, bubbles: true, cancelable: true }));
		await settle();
		expect(sends).not.toHaveBeenCalled();
		expect(blocks(editor)).toEqual(['paragraph:Notes', 'table:Q140']);

		const picture = new DataTransfer();
		picture.setData('text/html', '<img src="https://images.example.com/photo-1">');
		picture.items.add(new File(['x'], 'image.png', { type: 'image/png' }));
		editor.view.dom.dispatchEvent(new ClipboardEvent('paste', { clipboardData: picture, bubbles: true, cancelable: true }));
		await settle();
		expect(sends).toHaveBeenCalledOnce();
	});

	// A Word range holding a picture: its HTML carries the picture as a file
	// on the copying computer, and the picture of the range comes beside it.
	it("pastes the words of a Word range that holds a picture, not the picture of the range", async () => {
		const editor = track(htmlEditor('<p>Notes</p>', [pasteHandling(), mediaUploads(sends)]));
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
		expect(sends).not.toHaveBeenCalled();
		const text = blocks(editor).join(' ');
		expect(text).toContain('Quarterly results were strong.');
		expect(text).toContain('Next steps follow.');
	});

	it('a file larger than a page holds is said and left out; the rest go in', () => {
		vi.mocked(toast.error).mockClear();
		const editor = track(htmlEditor('<p>Notes</p>', [mediaUploads(sends)]));
		const huge = new File(['x'], 'film.mov', { type: 'video/quicktime' });
		Object.defineProperty(huge, 'size', { value: MAX_UPLOAD_BYTES + 1 });
		addFiles(editor.view, [huge], 6, { send: sends });
		expect(blocks(editor)).toEqual(['paragraph:Notes']);
		addFiles(editor.view, [huge, ...photos('small.png')], 6, { send: sends });
		expect(blocks(editor)).toEqual(['paragraph:Notes', 'image:small.png']);
		expect(toast.error).toHaveBeenCalledTimes(2);
		expect(sends).toHaveBeenCalledOnce();
		// The server's own limit (`api/media.rs`).
		const media = fs.readFileSync(path.resolve(HERE, '../../../../../virtues-core/src/api/media.rs'), 'utf8');
		expect(/const MAX_FILE_SIZE: usize = 100 \* 1024 \* 1024;/.test(media)).toBe(MAX_UPLOAD_BYTES === 100 * 1024 * 1024);
	});

	// The page's markdown reads a Drive file's kind from its name's
	// extension: a picture pasted as `image` came back from it as a file.
	it('names a file with no extension by its type, so its markdown reads back as the same block', () => {
		expect(nameForBlock(new File(['x'], 'image', { type: 'image/png' }))).toBe('image.png');
		expect(nameForBlock(new File(['x'], 'Voice memo', { type: 'audio/mp4' }))).toBe('Voice memo.m4a');
		expect(nameForBlock(new File(['x'], 'scan.TIF', { type: 'image/tiff' }))).toBe('scan.TIF');
		expect(nameForBlock(new File(['x'], 'notes', { type: 'application/x-unknown' }))).toBe('notes');
		const clip = new File(['x'], 'clip', { type: 'video/quicktime' });
		expect(kindOfFile(clip, nameForBlock(clip))).toBe('video');
		const unknown = new File(['x'], 'scan.heic', { type: '' });
		expect(kindOfFile(unknown, nameForBlock(unknown))).toBe('image');
		const editor = track(htmlEditor('<p>Notes</p>', [mediaUploads(sends)]));
		addFiles(editor.view, [new File(['x'], 'image', { type: 'image/png' })], 6, { send: sends });
		expect(kindOfLink(String(editor.state.doc.child(1).attrs.src), String(editor.state.doc.child(1).attrs.alt))).toBe('image');
	});

	it('the upload is sent to the address the block holds, and a failure it can recover from is tried again there', async () => {
		vi.useFakeTimers();
		try {
			const editor = track(htmlEditor('<p>Notes</p>', [mediaUploads()]));
			addFiles(editor.view, photos('harbour.png'), 6);
			const [id] = fileIds(editor);
			expect(transfers.map((t) => t.id)).toEqual([id]);
			expect(get(watchUpload(id))).toMatchObject({ phase: 'uploading', name: 'harbour.png' });
			expect(uploadsAtStake()).toBe(1);
			transfers[0].progress?.(1, 2);
			expect(get(watchUpload(id))).toMatchObject({ phase: 'uploading', sent: 1, total: 2 });
			// The network: tried again in two seconds, to the same address.
			transfers[0].fail(new UploadError("Couldn't reach your server", 0));
			await vi.advanceTimersByTimeAsync(2000);
			expect(transfers.map((t) => t.id)).toEqual([id, id]);
			transfers[1].done();
			await vi.advanceTimersByTimeAsync(0);
			expect(get(watchUpload(id))).toMatchObject({ phase: 'done' });
			expect(uploadsAtStake()).toBe(0);
		} finally {
			vi.useRealTimers();
		}
	});

	it('an upload out of retries keeps its block, which says why, and Try again sends the same file to the same address', async () => {
		vi.useFakeTimers();
		try {
			const editor = track(htmlEditor('<p>Notes</p>', [mediaUploads()]));
			addFiles(editor.view, photos('harbour.png'), 6);
			const before = editor.getJSON();
			const [id] = fileIds(editor);
			// Out of space is no refusal: emptying the trash makes room.
			for (let i = 0; i < 5; i++) {
				transfers[i].fail(new UploadError('Your server is out of space.', 507));
				await vi.advanceTimersByTimeAsync(60_000);
			}
			expect(transfers).toHaveLength(5);
			expect(get(watchUpload(id))).toMatchObject({ phase: 'failed', reason: 'Your server is out of space.', refused: false });
			expect(editor.getJSON()).toEqual(before);
			// Still the only copy of the file: a reload would lose it.
			expect(uploadsAtStake()).toBe(1);
			retryUpload(id);
			expect(transfers.map((t) => [t.id, t.file.name])).toEqual(Array.from({ length: 6 }, () => [id, 'harbour.png']));
			transfers[5].done();
			await vi.advanceTimersByTimeAsync(0);
			expect(get(watchUpload(id))).toMatchObject({ phase: 'done' });
			expect(uploadsAtStake()).toBe(0);
			expect(editor.getJSON()).toEqual(before);
		} finally {
			vi.useRealTimers();
		}
	});

	// Undone or removed, a block whose file had stopped short of a refusal
	// stayed at stake for good: every close asked first, the app never took a
	// new UI, every reconnect sent the file again, and nothing on any page
	// was left to let it go. It is let go once the server says no page names
	// it, and redo sends it again.
	it('a file whose block is undone or removed is let go once no page names it, and redo sends it again', async () => {
		const { doc } = pageDoc('<p>Notes</p>');
		const { editor } = bind(doc, { plugins: false });
		editor.registerPlugin(mediaUploads().config.addProseMirrorPlugins!.call({ editor } as never)[0]);
		await settle();
		const asksBeforeClosing = () => {
			const event = new Event('beforeunload', { cancelable: true });
			window.dispatchEvent(event);
			return event.defaultPrevented;
		};
		vi.useFakeTimers();
		try {
			addFiles(editor.view, photos('harbour.png'), posIn(editor, 'Notes', 5));
			const [id] = fileIds(editor);
			for (let i = 0; i < 5; i++) {
				transfers[i].fail(new UploadError('Your server is out of space.', 507));
				await vi.advanceTimersByTimeAsync(60_000);
			}
			expect(get(watchUpload(id))).toMatchObject({ phase: 'failed', refused: false });
			expect(uploadsAtStake()).toBe(1);

			// Its page saved without it, the server names it nowhere.
			server.names = () => false;
			editor.commands.undo();
			expect(fileIds(editor)).toEqual([]);
			await vi.advanceTimersByTimeAsync(CHECK_AFTER_MS);
			expect(uploadsAtStake()).toBe(0);
			expect(asksBeforeClosing()).toBe(false);
			window.dispatchEvent(new Event('online'));
			await vi.advanceTimersByTimeAsync(TRY_AGAIN_EVERY_MS);
			expect(transfers).toHaveLength(5);

			editor.commands.redo();
			expect(fileIds(editor)).toEqual([id]);
			expect(transfers.map((t) => t.id)).toEqual(Array.from({ length: 6 }, () => id));
			expect(uploadsAtStake()).toBe(1);

			// Remove, while it is on its way: once let go, a failure is not tried again.
			let at = -1;
			editor.state.doc.forEach((n, pos) => {
				if (n.attrs.src === `/drive/${id}`) at = pos;
			});
			editor.view.dispatch(editor.state.tr.delete(at, at + editor.state.doc.nodeAt(at)!.nodeSize));
			await vi.advanceTimersByTimeAsync(CHECK_AFTER_MS);
			expect(uploadsAtStake()).toBe(0);
			transfers[5].fail(new UploadError("Couldn't reach your server", 0));
			await vi.advanceTimersByTimeAsync(TRY_AGAIN_EVERY_MS);
			window.dispatchEvent(new Event('online'));
			await vi.advanceTimersByTimeAsync(0);
			expect(transfers).toHaveLength(6);
			expect(asksBeforeClosing()).toBe(false);
		} finally {
			vi.useRealTimers();
		}
	});

	// Copied into a page that was then closed, and taken out of the page it
	// was given to, the block's file was let go on what the open pages
	// showed: never sent, not at stake, and the copy named a file that never
	// came. Another device's or the model's move to a page not open here
	// went the same way.
	it('a file whose block was copied into a page now closed goes on, while that page names it', async () => {
		vi.useFakeTimers();
		const one = track(htmlEditor('<p>Notes</p>', [mediaUploads()]));
		const other = htmlEditor('<p>Elsewhere</p>', [mediaUploads()]);
		registerTreeEditor('page_one', one);
		registerTreeEditor('page_other', other);
		try {
			addFiles(one.view, photos('harbour.png'), posIn(one, 'Notes', 5));
			const [id] = fileIds(one);
			transfers[0].fail(new UploadError("Couldn't reach your server", 0));
			await vi.advanceTimersByTimeAsync(0);
			const block = one.state.doc.lastChild!;
			other.view.dispatch(other.state.tr.insert(other.state.doc.content.size, other.schema.nodeFromJSON(block.toJSON())));
			unregisterTreeEditor('page_other', other);
			other.destroy();
			one.view.dispatch(one.state.tr.delete(one.state.doc.content.size - block.nodeSize, one.state.doc.content.size));
			expect(fileIds(one)).toEqual([]);
			await vi.advanceTimersByTimeAsync(CHECK_AFTER_MS);
			expect(uploadsAtStake()).toBe(1);
			expect(transfers.map((t) => t.id)).toEqual([id, id]);
			transfers[1].done();
			await vi.advanceTimersByTimeAsync(0);
			expect(get(watchUpload(id))).toMatchObject({ phase: 'done' });
			expect(uploadsAtStake()).toBe(0);
		} finally {
			unregisterTreeEditor('page_one', one);
			vi.useRealTimers();
		}
	});

	// Shared from the device sending it, a page went without an image on its
	// way whenever the server had not heard of it yet.
	it('counts the images on an open page that this device has not finished sending', async () => {
		const editor = track(htmlEditor('<p>Notes</p><img src="https://images.example.com/photo-1" alt="Harbour">', [mediaUploads()]));
		registerTreeEditor('page_shared', editor);
		try {
			expect(imagesOnTheirWay('page_shared')).toBe(0);
			addFiles(editor.view, [...photos('harbour.png', 'pier.png'), new File(['%PDF'], 'report.pdf', { type: 'application/pdf' })], 6);
			expect(imagesOnTheirWay('page_shared')).toBe(2);
			expect(imagesOnTheirWay('page_closed')).toBe(0);
			transfers[0].done();
			await settle();
			expect(imagesOnTheirWay('page_shared')).toBe(1);
		} finally {
			unregisterTreeEditor('page_shared', editor);
		}
	});

	// Turn into link left the page pointing at the file's address while the
	// upload, taken for gone, stopped trying after a failure and let the app
	// close: a link to an address nothing would fill, with nothing said.
	it('a file whose block is turned into a link to its address stays at stake and is tried again', async () => {
		vi.useFakeTimers();
		try {
			const editor = track(htmlEditor('<p>Notes</p>', [mediaUploads()]));
			addFiles(editor.view, [new File(['%PDF'], 'report.pdf', { type: 'application/pdf' })], 6);
			const [id] = fileIds(editor);
			let at = -1;
			editor.state.doc.forEach((n, pos) => {
				if (n.attrs.src === `/drive/${id}`) at = pos;
			});
			// What MediaNode's Turn into link writes in the block's place.
			const { schema } = editor;
			const link = schema.nodes.paragraph.create(null, schema.text('report.pdf', [schema.marks.link.create({ href: `/drive/${id}` })]));
			editor.view.dispatch(editor.state.tr.replaceWith(at, at + editor.state.doc.nodeAt(at)!.nodeSize, link));
			expect(fileIds(editor)).toEqual([]);
			expect(uploadsAtStake()).toBe(1);
			transfers[0].fail(new UploadError("Couldn't reach your server", 0));
			await vi.advanceTimersByTimeAsync(2000);
			expect(transfers.map((t) => t.id)).toEqual([id, id]);
			expect(uploadsAtStake()).toBe(1);
			transfers[1].done();
			await vi.advanceTimersByTimeAsync(0);
			expect(uploadsAtStake()).toBe(0);
		} finally {
			vi.useRealTimers();
		}
	});

	it('a file whose block is cut from one page and pasted into another stays at stake', async () => {
		const one = htmlEditor('<p>Notes</p>', [mediaUploads()]);
		const other = htmlEditor('<p>Elsewhere</p>', [mediaUploads()]);
		track(one);
		track(other);
		registerTreeEditor('page_one', one);
		registerTreeEditor('page_other', other);
		try {
			addFiles(one.view, photos('harbour.png'), posIn(one, 'Notes', 5));
			const [id] = fileIds(one);
			const block = one.state.doc.lastChild!;
			// Pasted into the other page first, then cut from this one.
			other.view.dispatch(other.state.tr.insert(other.state.doc.content.size, other.schema.nodeFromJSON(block.toJSON())));
			one.view.dispatch(one.state.tr.delete(one.state.doc.content.size - block.nodeSize, one.state.doc.content.size));
			expect(fileIds(one)).toEqual([]);
			expect(fileIds(other)).toEqual([id]);
			expect(uploadsAtStake()).toBe(1);
		} finally {
			unregisterTreeEditor('page_one', one);
			unregisterTreeEditor('page_other', other);
		}
	});

	it('a refused upload keeps its block, which says why, and is never sent again', async () => {
		const editor = track(htmlEditor('<p>Notes</p>', [mediaUploads()]));
		addFiles(editor.view, photos('harbour.png'), 6);
		const before = editor.getJSON();
		const [id] = fileIds(editor);
		transfers[0].fail(new UploadError('harbour.png is over 100 MB, the most a page holds.', 413));
		await settle();
		expect(get(watchUpload(id))).toMatchObject({ phase: 'failed', refused: true });
		// Every other device has stopped asking: sent again, it would be refused again.
		retryUpload(id);
		expect(transfers).toHaveLength(1);
		expect(uploadsAtStake()).toBe(0);
		expect(editor.getJSON()).toEqual(before);
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
		for (const file of ['components/tabs/views/AssetView.svelte', 'utils/refRoutes.ts', 'components/RefCard.svelte']) {
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
		const convert = realConvert();
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
			const editor = track(
				createPageEditor({
					element,
					doc: { ydoc: doc, provider },
					editable: true,
					placeholder: 'Write',
					plugins: [pasteHandling(), mediaUploads(sends)],
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
			// At once, in the edit the paste made.
			expect(cells(), `${anchor}..${head}`).toEqual(want);
			await settle();
			expect(cells(), `${anchor}..${head} synced`).toEqual(want);
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

describe('code highlighting, kept', () => {
	// A drag dropped where it started, or a block replaced by its copy, makes
	// a new document equal to the old one: the colours were mapped away, and
	// with nothing to say the code had changed they were never drawn again.
	it('is drawn again after a change that puts back the same code', async () => {
		const html = '<p>Intro</p><pre data-language="rust"><code>fn kept() {}</code></pre>';
		const editor = track(htmlEditor(html, [codeHighlight(async (code) => [[{ content: code, color: 'rgb(9, 8, 7)', offset: 0 }]])]));
		const coloured = () => editor.view.dom.querySelectorAll('[style*="rgb(9, 8, 7)"]').length;
		await settle(250);
		expect(coloured()).toBeGreaterThan(0);
		let at = -1;
		editor.state.doc.forEach((node, offset) => {
			if (node.type.name === 'codeBlock') at = offset;
		});
		const block = editor.state.doc.nodeAt(at)!;
		editor.view.dispatch(editor.state.tr.replaceWith(at, at + block.nodeSize, block.copy(block.content)));
		expect(coloured(), 'mapped away by the change').toBe(0);
		await settle(250);
		expect(coloured()).toBeGreaterThan(0);
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
		// Held, the wire keeps each side's updates until `release`: edits the
		// two devices make meanwhile are concurrent, as on a slow network.
		const wire = { held: false, toPhone: [] as Uint8Array[], toHere: [] as Uint8Array[] };
		doc.on('update', (u: Uint8Array, origin: unknown) => {
			if (origin === 'wire') return;
			if (wire.held) wire.toPhone.push(u);
			else Y.applyUpdate(other, u, 'wire');
		});
		other.on('update', (u: Uint8Array, origin: unknown) => {
			if (origin === 'wire') return;
			if (wire.held) wire.toHere.push(u);
			else Y.applyUpdate(doc, u, 'wire');
		});
		const { editor, element } = bind(doc);
		const phone = bind(other, { plugins: false }).editor;
		const hold = () => (wire.held = true);
		const release = () => {
			wire.held = false;
			for (const u of wire.toPhone.splice(0)) Y.applyUpdate(other, u, 'wire');
			for (const u of wire.toHere.splice(0)) Y.applyUpdate(doc, u, 'wire');
		};
		return { editor, phone, element, hold, release };
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

	it('a picked file goes in where the command ran, the phone writing while the dialog was open', async () => {
		const { editor, phone } = twoDevices('<h2>Trip to Lisbon</h2><p>Lunch with Nick on Friday.</p>');
		await settle();
		const end = posIn(editor, 'Lunch with Nick on Friday.', 'Lunch with Nick on Friday.'.length);
		const at = anchorAt(editor.state, end);
		// The file dialog is open while the phone writes.
		await phoneEditsAbove(phone, 'Trip to Lisbon');
		addFiles(editor.view, [new File(['abc'], 'memo.m4a', { type: 'audio/mp4' })], at, { send: sends });
		phone.commands.insertContentAt(0, '<p>And again, while it uploads.</p>');
		await settle();
		const want = [
			'paragraph:And again, while it uploads.',
			'paragraph:Added on the phone first.',
			'heading:Moved: Trip to Lisbon',
			'paragraph:Lunch with Nick on Friday.',
			'audio:',
		];
		expect(blocks(editor)).toEqual(want);
		expect(blocks(phone)).toEqual(want);
		expect(String(phone.state.doc.lastChild!.attrs.src)).toMatch(UPLOAD_SRC);
	});

	it('a paste goes in where it was pasted, and the phone writing at the same moment merges around it', async () => {
		const { editor, phone, hold, release } = twoDevices('<p>Plan</p><p></p><p>Lunch with Nick.</p>');
		await settle();
		vi.mocked(toast).mockClear();
		hold();
		editor.commands.setTextSelection(editor.state.doc.child(0).nodeSize + 1);
		pasteMarkdown(editor.view, '- passport\n- tickets', editor.state.selection.from);
		phone.commands.insertContentAt(posIn(phone, 'Plan', 'Plan'.length), ' for Friday');
		await phoneEditsAbove(phone, 'Lunch with Nick');
		release();
		await settle();
		expect(blocks(editor)).toEqual([
			'paragraph:Added on the phone first.',
			'paragraph:Plan for Friday',
			'bulletList:passporttickets',
			'paragraph:Moved: Lunch with Nick.',
		]);
		expect(toast).not.toHaveBeenCalled();
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

	/** Paste markdown or a picture over the selection: in the page when it returns. */
	function overSelection(editor: Editor, via: 'paste' | 'upload'): void {
		const { selection } = editor.state;
		if (via === 'paste') pasteMarkdown(editor.view, '## Plan', selection.from, { replacing: selection });
		else addFiles(editor.view, [new File(['x'], 'plan.png', { type: 'image/png' })], selection.from, { replacing: selection, send: sends });
	}

	// A paste or files over a selection replace it in the edit the person
	// makes. Words the phone typed into it at the same moment are its own,
	// which the merge keeps: nothing here deletes what it never saw.
	it('a paste or files over a selection replace it at once, and words the phone typed into it at the same moment stay', async () => {
		for (const via of ['paste', 'upload'] as const) {
			const landed = via === 'paste' ? 'heading:Plan' : 'image:';
			for (const [phoneTypes, typed] of [
				[(p: Editor) => p.commands.insertContentAt(posIn(p, 'Hello world', 5), ' there'), 'there'],
				[(p: Editor) => p.commands.insertContentAt(posIn(p, 'Hello world'), 'Oh, '), 'Oh,'],
				[(p: Editor) => p.commands.insertContentAt(posIn(p, 'Hello world', 'Hello world'.length), '!'), '!'],
			] as const) {
				const { editor, phone, hold, release } = twoDevices('<p>Say Hello world</p><p>Next</p>');
				await settle();
				vi.mocked(toast).mockClear();
				const from = posIn(editor, 'Hello world');
				editor.commands.setTextSelection({ from, to: from + 'Hello world'.length });
				hold();
				overSelection(editor, via);
				expect(blocks(editor), `${via}: at once`).toEqual(['paragraph:Say ', landed, 'paragraph:Next']);
				phoneTypes(phone);
				release();
				await settle();
				const merged = blocks(editor);
				expect(merged.join(' | '), `${via}: ${typed} kept`).toContain(typed);
				expect(merged.join(' | ')).not.toContain('Hello');
				expect(merged).toContain(landed);
				expect(toast).not.toHaveBeenCalled();
				expect(blocks(phone)).toEqual(merged);
				for (const e of [editor, phone]) e.destroy();
			}
		}
	});

	// The person types the moment a paste or files went in over a selection:
	// before it, at the caret where the selection was, and after it. The
	// paste was once held there while the server answered, and typing inside
	// or beside the held words moved it, lost them, or split the undo.
	it('typing on this device at once, before, where and after a paste or files over a selection, lands where it is typed', async () => {
		for (const via of ['paste', 'upload'] as const) {
			const landed = via === 'paste' ? 'heading:Plan' : 'image:';
			for (const where of ['before', 'where it was', 'after'] as const) {
				const { editor, phone } = twoDevices('<p>Say Hello world</p><p>Next</p>');
				await settle();
				const from = posIn(editor, 'Hello world');
				editor.commands.setTextSelection({ from, to: from + 'Hello world'.length });
				overSelection(editor, via);
				if (where === 'before') editor.commands.setTextSelection(posIn(editor, 'Say'));
				if (where === 'after') editor.commands.setTextSelection(posIn(editor, 'Next'));
				typeIn(editor, 'Oh');
				await settle();
				const what = `${via}, typed ${where}`;
				// The caret, where the selection was, ends after what went in: at
				// the end of the pasted heading, or in the line after the file.
				const want = {
					before: ['paragraph:OhSay ', landed, 'paragraph:Next'],
					'where it was': ['paragraph:Say ', via === 'paste' ? 'heading:PlanOh' : landed, via === 'paste' ? 'paragraph:Next' : 'paragraph:OhNext'],
					after: ['paragraph:Say ', landed, 'paragraph:OhNext'],
				}[where];
				expect(blocks(editor), what).toEqual(want);
				expect(blocks(phone), what).toEqual(want);
				// The typing is its own step, and the paste or the files the next.
				editor.commands.undo();
				expect(blocks(editor).join(' | '), what).not.toContain('Oh');
				editor.commands.undo();
				expect(blocks(editor), what).toEqual(['paragraph:Say Hello world', 'paragraph:Next']);
				for (const e of [editor, phone]) e.destroy();
			}
		}
	});

	it('a paste that splits a paragraph keeps its id on the words before it, and gives every block its own', async () => {
		const { editor, phone } = twoDevices('<p>Plan</p><p>Before after</p>');
		await settle();
		const id = editor.state.doc.child(1).attrs[contract.id.attr];
		const at = posIn(editor, 'Before after', 7);
		editor.commands.setTextSelection(at);
		pasteMarkdown(editor.view, '## Pasted\n\n- one', at);
		await settle();
		expect(blocks(editor)).toEqual(['paragraph:Plan', 'paragraph:Before ', 'heading:Pasted', 'bulletList:one', 'paragraph:after']);
		expect(editor.state.doc.child(1).attrs[contract.id.attr]).toBe(id);
		const all = ids(editor);
		expect(all.every(Boolean)).toBe(true);
		expect(new Set(all).size).toBe(all.length);
		expect(blocks(phone)).toEqual(blocks(editor));
	});

	it('a paste into a paragraph the phone deleted at the same moment merges as typing there does', async () => {
		const { editor, phone, hold, release } = twoDevices('<p>Plan</p><p>Draft line.</p><p>Lunch with Nick.</p>');
		await settle();
		vi.mocked(toast).mockClear();
		hold();
		pasteMarkdown(editor.view, '- passport', posIn(editor, 'Draft line.', 5));
		const from = posIn(phone, 'Draft line.') - 1;
		phone.commands.deleteRange({ from, to: from + phone.state.doc.child(1).nodeSize });
		release();
		await settle();
		expect(blocks(editor)).toContain('bulletList:passport');
		expect(blocks(editor).join(' | ')).not.toContain('Draft');
		expect(blocks(editor)[0]).toBe('paragraph:Plan');
		expect(blocks(editor).at(-1)).toBe('paragraph:Lunch with Nick.');
		expect(blocks(phone)).toEqual(blocks(editor));
		expect(toast).not.toHaveBeenCalled();
	});

	// Files over a selection whose block the phone deleted at the same
	// moment: the files' block is the person's edit, which the merge keeps,
	// and the upload that follows writes nothing that could land elsewhere.
	it('files over a selection in a paragraph the phone deleted at the same moment stay in the page', async () => {
		const { editor, phone, hold, release } = twoDevices('<p>Plan</p><p>Draft line.</p><p>Lunch with Nick.</p>');
		await settle();
		vi.mocked(toast).mockClear();
		const from = posIn(editor, 'Draft line.');
		editor.commands.setTextSelection({ from, to: from + 'Draft'.length });
		hold();
		overSelection(editor, 'upload');
		const id = sends.mock.calls.at(-1)![0];
		const at = posIn(phone, 'Draft line.') - 1;
		phone.commands.deleteRange({ from: at, to: at + phone.state.doc.child(1).nodeSize });
		release();
		await settle();
		const src = (e: Editor) => {
			let found: unknown = null;
			e.state.doc.descendants((n) => {
				if (n.type.name === 'image') found = n.attrs.src;
				return found === null;
			});
			return found;
		};
		expect(src(editor)).toBe(`/drive/${id}`);
		expect(src(phone)).toBe(`/drive/${id}`);
		expect(blocks(editor).join(' | ')).not.toContain('Draft');
		expect(blocks(editor)[0]).toBe('paragraph:Plan');
		expect(blocks(editor).at(-1)).toBe('paragraph:Lunch with Nick.');
		expect(blocks(phone)).toEqual(blocks(editor));
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

	/** ⌘Z (Ctrl-Z off Apple keyboards), as the browser delivers it. */
	function pressUndo(editor: Editor) {
		const apple = /Mac|iP(hone|[oa]d)/.test(navigator.platform);
		editor.view.dom.dispatchEvent(
			new KeyboardEvent('keydown', { key: 'z', code: 'KeyZ', metaKey: apple, ctrlKey: !apple, bubbles: true, cancelable: true }),
		);
	}

	/** The platform's undo (shake, three fingers, Edit > Undo), as the browser delivers it. */
	function platformUndo(editor: Editor) {
		editor.view.dom.dispatchEvent(new InputEvent('beforeinput', { inputType: 'historyUndo', bubbles: true, cancelable: true }));
	}

	// Within y-undo's capture window of typing just before it, a paste or
	// files would join that typing, and one undo would take both.
	it('a paste right after typing is an undo step of its own', async () => {
		const { doc } = pageDoc('<p>Notes</p><p>Draft</p>');
		const { editor } = bind(doc);
		await settle();
		editor.commands.setTextSelection(posIn(editor, 'Draft', 5));
		typeIn(editor, ' typed');
		pasteMarkdown(editor.view, '## Plan', posIn(editor, 'Notes', 5));
		expect(blocks(editor)).toEqual(['paragraph:Notes', 'heading:Plan', 'paragraph:Draft typed']);
		editor.commands.undo();
		expect(blocks(editor)).toEqual(['paragraph:Notes', 'paragraph:Draft typed']);
		editor.commands.undo();
		expect(blocks(editor)).toEqual(['paragraph:Notes', 'paragraph:Draft']);
		agree(editor, doc);
	});

	it('files added right after typing are an undo step of their own', async () => {
		const { doc } = pageDoc('<p>Notes</p><p>Draft</p>');
		const { editor } = bind(doc);
		await settle();
		editor.commands.setTextSelection(posIn(editor, 'Draft', 5));
		typeIn(editor, ' typed');
		addFiles(editor.view, [new File(['x'], 'a.png', { type: 'image/png' })], posIn(editor, 'Notes', 5), { send: sends });
		expect(blocks(editor)).toEqual(['paragraph:Notes', 'image:', 'paragraph:Draft typed']);
		editor.commands.undo();
		expect(blocks(editor)).toEqual(['paragraph:Notes', 'paragraph:Draft typed']);
		agree(editor, doc);
	});

	// Undo while a paste waited on the server once took the person's own
	// writing instead, and the paste landed anyway: a paste is now an edit
	// from the moment it is made, and undo takes it back like any other.
	it('Undo right after a paste takes it back, and only it, by key or by the platform’s undo; redo puts it back', async () => {
		for (const how of ['key', 'platform'] as const) {
			const { doc } = pageDoc('<p>Notes</p>');
			const { editor } = bind(doc);
			await settle();
			editor.commands.setTextSelection(posIn(editor, 'Notes', 5));
			typeIn(editor, ' I wrote this sentence.');
			await settle(600);
			pasteMarkdown(editor.view, '## Plan\n\n- one', editor.state.selection.from);
			expect(blocks(editor), how).toEqual(['paragraph:Notes I wrote this sentence.', 'heading:Plan', 'bulletList:one']);
			if (how === 'key') pressUndo(editor);
			else platformUndo(editor);
			expect(blocks(editor), how).toEqual(['paragraph:Notes I wrote this sentence.']);
			expect(editor.commands.redo(), how).toBe(true);
			expect(blocks(editor), how).toEqual(['paragraph:Notes I wrote this sentence.', 'heading:Plan', 'bulletList:one']);
			pressUndo(editor);
			pressUndo(editor);
			expect(blocks(editor), how).toEqual(['paragraph:Notes']);
			agree(editor, doc);
		}
	});

	// Undo while a file uploaded once took the person's own writing, and the
	// file landed anyway. Its block is now an edit from the moment the file
	// was given: undo takes it out, its upload ending writes nothing, and
	// redo puts back the block at the address its file reached.
	it('Undo takes a file block back while its file uploads, by key or by the platform’s undo; its upload ending writes nothing; redo puts it back', async () => {
		for (const how of ['key', 'platform'] as const) {
			transfers.length = 0;
			const { doc } = pageDoc('<p>Notes</p>');
			const { editor } = bind(doc);
			const ext = mediaUploads();
			editor.registerPlugin(ext.config.addProseMirrorPlugins!.call({ editor } as never)[0]);
			await settle();
			editor.commands.setTextSelection(posIn(editor, 'Notes', 5));
			typeIn(editor, ' and a paragraph I wrote.');
			await settle(600);
			addFiles(editor.view, [new File(['x'], 'wrong.png', { type: 'image/png' })], editor.state.selection.from);
			const src = String(editor.state.doc.lastChild!.attrs.src);
			expect(src, how).toMatch(UPLOAD_SRC);
			expect(transfers.map((t) => `/drive/${t.id}`), how).toEqual([src]);
			if (how === 'key') pressUndo(editor);
			else platformUndo(editor);
			expect(blocks(editor), how).toEqual(['paragraph:Notes and a paragraph I wrote.']);
			const updates: Uint8Array[] = [];
			const onUpdate = (u: Uint8Array) => updates.push(u);
			doc.on('update', onUpdate);
			transfers[0].done();
			await settle();
			doc.off('update', onUpdate);
			expect(updates, how).toHaveLength(0);
			expect(editor.commands.redo(), how).toBe(true);
			expect(blocks(editor), how).toEqual(['paragraph:Notes and a paragraph I wrote.', 'image:']);
			expect(String(editor.state.doc.lastChild!.attrs.src), how).toBe(src);
			pressUndo(editor);
			pressUndo(editor);
			expect(blocks(editor), how).toEqual(['paragraph:Notes']);
			agree(editor, doc);
		}
	});

	it('Undo takes back the file block, then the paste, then the typing, in the order made', async () => {
		const { doc } = pageDoc('<p>Notes</p><p>Draft</p>');
		const { editor } = bind(doc);
		await settle();
		editor.commands.setTextSelection(posIn(editor, 'Draft', 5));
		typeIn(editor, ' typed');
		await settle(600);
		pasteMarkdown(editor.view, '## Plan', posIn(editor, 'Notes', 5));
		addFiles(editor.view, [new File(['x'], 'a.png', { type: 'image/png' })], posIn(editor, 'Draft typed', 11), { send: sends });
		expect(blocks(editor)).toEqual(['paragraph:Notes', 'heading:Plan', 'paragraph:Draft typed', 'image:']);
		pressUndo(editor);
		expect(blocks(editor)).toEqual(['paragraph:Notes', 'heading:Plan', 'paragraph:Draft typed']);
		pressUndo(editor);
		expect(blocks(editor)).toEqual(['paragraph:Notes', 'paragraph:Draft typed']);
		pressUndo(editor);
		expect(blocks(editor)).toEqual(['paragraph:Notes', 'paragraph:Draft']);
		agree(editor, doc);
	});

	it('a markdown paste over a selection is one undo step', async () => {
		const { doc } = pageDoc('<p>Before</p><p>Select me</p>');
		const provider = fakeProvider(doc);
		cleanups.push(provider.destroy);
		const element = document.createElement('div');
		document.body.append(element);
		const editor = track(
			createPageEditor({ element, doc: { ydoc: doc, provider }, editable: true, placeholder: 'Write', plugins: [pasteHandling()] }),
		);
		await settle();
		const from = posIn(editor, 'Select me');
		editor.commands.setTextSelection({ from, to: from + 'Select me'.length });
		const event = {
			clipboardData: { getData: (type: string) => (type === 'text/plain' ? '## Plan\n\n- one' : ''), files: [] },
			preventDefault() {},
		} as unknown as ClipboardEvent;
		expect(editor.view.someProp('handlePaste', (f) => f(editor.view, event, editor.state.doc.slice(0, 0)))).toBe(true);
		expect(blocks(editor)).toEqual(['paragraph:Before', 'heading:Plan', 'bulletList:one']);
		agree(editor, doc);
		expect(() => editor.commands.undo()).not.toThrow();
		expect(blocks(editor)).toEqual(['paragraph:Before', 'paragraph:Select me']);
		agree(editor, doc);
		expect(editor.commands.redo()).toBe(true);
		expect(blocks(editor)).toEqual(['paragraph:Before', 'heading:Plan', 'bulletList:one']);
		agree(editor, doc);
	});
});

/**
 * The family of defects this guards against had one shape: what the person
 * did was split into a decision, made at once, and a write, made when a
 * server answered, with everything in between to be reconciled (the place,
 * the selection, the order, undo, other devices' edits). A paste now
 * converts in the page, and a file's block goes in when it is given: the
 * page changes once, when the person acts. These hold that shape.
 */
describe('nothing in the editor lands after a wait', () => {
	function clipboard(data: Record<string, string>, files: File[] = []) {
		return { clipboardData: { getData: (type: string) => data[type] ?? '', files }, preventDefault() {} } as unknown as ClipboardEvent;
	}

	function dropEvent(editor: Editor, data: Record<string, string>, files: File[] = []) {
		const transfer = new DataTransfer();
		for (const [type, value] of Object.entries(data)) transfer.setData(type, value);
		for (const file of files) transfer.items.add(file);
		const event = new DragEvent('drop', { bubbles: true, cancelable: true, clientX: 10, clientY: 10 });
		Object.defineProperty(event, 'dataTransfer', { value: transfer });
		return event;
	}

	const VSCODE_MARKDOWN = JSON.stringify({ version: 1, mode: 'markdown' });

	it('a paste, a drop and files pasted, dropped or picked are in the page when their handler returns, nothing fetched and nothing written after', async () => {
		const fetch = vi.fn(() => new Promise<Response>(() => {}));
		vi.stubGlobal('fetch', fetch);
		try {
			const cases: [string, (editor: Editor) => void, string][] = [
				['markdown text', (e) => void e.view.someProp('handlePaste', (f) => f(e.view, clipboard({ 'text/plain': '## Plan\n\n- one' }), e.state.doc.slice(0, 0))), 'heading:Plan'],
				[
					"a code editor's markdown",
					(e) =>
						void e.view.someProp('handlePaste', (f) =>
							f(e.view, clipboard({ 'text/plain': '## Plan', 'text/html': '<div style="white-space: pre"><div><span>## Plan</span></div></div>' }), e.state.doc.slice(0, 0)),
						),
					'heading:Plan',
				],
				[
					"VS Code's markdown",
					(e) =>
						void e.view.someProp('handlePaste', (f) =>
							f(e.view, clipboard({ 'text/plain': '## Plan', 'text/html': '<div>## Plan</div>', 'vscode-editor-data': VSCODE_MARKDOWN }), e.state.doc.slice(0, 0)),
						),
					'heading:Plan',
				],
				[
					'markdown dropped',
					(e) => {
						e.view.posAtCoords = () => ({ pos: 1, inside: -1 });
						e.view.dom.dispatchEvent(dropEvent(e, { 'text/plain': '## Plan' }));
					},
					'heading:Plan',
				],
				['files pasted', (e) => void e.view.someProp('handlePaste', (f) => f(e.view, clipboard({}, [new File(['x'], 'a.png', { type: 'image/png' })]), e.state.doc.slice(0, 0))), 'image:'],
				[
					'files dropped',
					(e) => {
						e.view.posAtCoords = () => ({ pos: 1, inside: -1 });
						e.view.dom.dispatchEvent(dropEvent(e, {}, [new File(['x'], 'a.png', { type: 'image/png' })]));
					},
					'image:',
				],
				['files picked', (e) => addFiles(e.view, [new File(['x'], 'a.png', { type: 'image/png' })], anchorAt(e.state, 1)), 'image:'],
			];
			for (const [what, act, landed] of cases) {
				const { doc } = pageDoc('<p></p><p>End</p>');
				const { editor } = bind(doc);
				editor.registerPlugin(mediaUploads().config.addProseMirrorPlugins!.call({ editor } as never)[0]);
				await settle();
				editor.commands.setTextSelection(1);
				await settle();
				vi.useFakeTimers();
				try {
					act(editor);
					const out: string[] = [];
					editor.state.doc.forEach((n) => out.push(`${n.type.name}:${n.isAtom ? '' : n.textContent}`));
					expect(out, what).toContain(landed);
					expect(treeOf(doc), what).toEqual(editor.state.doc.toJSON());
					// What the editor schedules after any edit (a code block's
					// highlight, Tiptap's own events) runs, and none of it writes:
					// the paste's whole effect was the edit it made.
					const landedDoc = editor.state.doc.toJSON();
					const updates: Uint8Array[] = [];
					doc.on('update', (u: Uint8Array) => updates.push(u));
					await vi.runAllTimersAsync();
					expect(updates, what).toHaveLength(0);
					expect(editor.state.doc.toJSON(), what).toEqual(landedDoc);
				} finally {
					vi.useRealTimers();
				}
			}
			expect(fetch).not.toHaveBeenCalled();
		} finally {
			vi.unstubAllGlobals();
		}
	});

	it('an upload ending, failing or tried again writes nothing to the page, in any order', async () => {
		const { doc } = pageDoc('<p>Notes</p>');
		const { editor } = bind(doc);
		editor.registerPlugin(mediaUploads().config.addProseMirrorPlugins!.call({ editor } as never)[0]);
		await settle();
		addFiles(editor.view, photos('one.png', 'two.png', 'three.png'), 6);
		const before = editor.state.doc.toJSON();
		const updates: Uint8Array[] = [];
		doc.on('update', (u: Uint8Array) => updates.push(u));
		transfers[2].done();
		transfers[0].fail(new UploadError('Your server is out of space.', 507));
		transfers[1].done();
		await settle();
		// Back online: sent again now, not after its retry's wait.
		window.dispatchEvent(new Event('online'));
		transfers[3].fail(new UploadError("Couldn't reach your server", 0));
		await settle();
		expect(updates).toHaveLength(0);
		expect(editor.state.doc.toJSON()).toEqual(before);
		expect(transfers.slice(0, 3).map((t) => get(watchUpload(t.id))?.phase)).toEqual(['uploading', 'done', 'done']);
	});

	const photos = (...names: string[]) => names.map((n) => new File(['x'], n, { type: 'image/png' }));

	// The paste was lost when the page closed before the server answered,
	// and nothing said so.
	it('a paste made just before the page closes is in the page', async () => {
		const { doc } = pageDoc('<p>Notes</p>');
		const { editor } = bind(doc);
		await settle();
		editor.commands.setTextSelection(posIn(editor, 'Notes', 5));
		editor.view.someProp('handlePaste', (f) => f(editor.view, clipboard({ 'text/plain': '## Plan\n\n- one' }), editor.state.doc.slice(0, 0)));
		editor.destroy();
		const after = htmlEditor('<p>x</p>');
		track(after);
		const tree = yXmlFragmentToProseMirrorRootNode(doc.getXmlFragment(contract.fragment), after.schema);
		const out: string[] = [];
		tree.forEach((n) => out.push(`${n.type.name}:${n.textContent}`));
		expect(out).toEqual(['paragraph:Notes', 'heading:Plan', 'bulletList:one']);
	});

	// The server stored the file, but it never reached the page when the
	// page closed while it uploaded.
	it('files added just before the page closes are in the page, and their upload ends after', async () => {
		const { doc } = pageDoc('<p>Notes</p>');
		const { editor } = bind(doc);
		await settle();
		addFiles(editor.view, photos('harbour.png'), 6);
		const src = String(editor.state.doc.lastChild!.attrs.src);
		editor.destroy();
		transfers[0].done();
		await settle();
		expect(get(watchUpload(transfers[0].id))).toMatchObject({ phase: 'done' });
		const after = htmlEditor('<p>x</p>');
		track(after);
		const tree = yXmlFragmentToProseMirrorRootNode(doc.getXmlFragment(contract.fragment), after.schema);
		expect(tree.lastChild?.type.name).toBe('image');
		expect(tree.lastChild?.attrs.src).toBe(src);
		expect(`/drive/${transfers[0].id}`).toBe(src);
	});
});

describe('keys a writer reaches for', () => {
	const apple = () => /Mac|iP(hone|[oa]d)/.test(navigator.platform);

	/** Press `key` in the editor, as the browser delivers it; whether the editor took it. */
	function press(editor: Editor, key: string, mods: { shift?: boolean; mod?: boolean } = {}): boolean {
		const event = new KeyboardEvent('keydown', {
			key,
			code: key.length === 1 ? `Key${key.toUpperCase()}` : key,
			shiftKey: !!mods.shift,
			metaKey: !!mods.mod && apple(),
			ctrlKey: !!mods.mod && !apple(),
			bubbles: true,
			cancelable: true,
		});
		editor.view.dom.dispatchEvent(event);
		return event.defaultPrevented;
	}

	// The stock link is inclusive while autolink is on: every word typed
	// after a link went into it.
	it('typing just after a link is not part of it', () => {
		const editor = track(htmlEditor('<p>See <a href="https://example.com/a">the notes</a>. Then</p>'));
		editor.commands.setTextSelection(posIn(editor, 'the notes', 9));
		typeIn(editor, 'x');
		const at = posIn(editor, 'notesx', 5);
		expect(editor.state.doc.nodeAt(at)?.text).toContain('x');
		expect(editor.state.doc.nodeAt(at)?.marks.map((m) => m.type.name)).toEqual([]);
		expect(editor.state.doc.nodeAt(at - 1)?.marks.map((m) => m.type.name)).toEqual(['link']);
	});

	// Tab in a code block, and on an item with none above it to nest under,
	// was nobody's: the browser moved the focus to the next control.
	it('Tab and Shift-Tab indent code and nest list items, and never send the focus out of the page', async () => {
		const { doc } = pageDoc('<ul><li><p>First</p></li><li><p>Second</p></li></ul><pre><code>a = 1</code></pre><p>Plain</p>');
		const { editor } = bind(doc);
		await settle();
		editor.commands.setTextSelection(posIn(editor, 'a = 1'));
		expect(press(editor, 'Tab')).toBe(true);
		expect(editor.state.selection.$from.parent.textContent).toBe('  a = 1');
		expect(press(editor, 'Tab', { shift: true })).toBe(true);
		expect(editor.state.selection.$from.parent.textContent).toBe('a = 1');

		const before = editor.getJSON();
		editor.commands.setTextSelection(posIn(editor, 'First', 2));
		expect(press(editor, 'Tab'), 'the first item, which cannot nest').toBe(true);
		expect(editor.getJSON()).toEqual(before);
		editor.commands.setTextSelection(posIn(editor, 'Second', 2));
		expect(press(editor, 'Tab')).toBe(true);
		expect(editor.state.doc.firstChild?.childCount).toBe(1);
		expect(press(editor, 'Tab', { shift: true })).toBe(true);
		expect(editor.state.doc.firstChild?.childCount).toBe(2);

		editor.commands.setTextSelection(posIn(editor, 'Plain', 2));
		expect(press(editor, 'Tab'), 'a paragraph leaves Tab to the browser').toBe(false);
	});

	it('Cmd-A in a code block selects its code, and pressed again the page', async () => {
		const { doc } = pageDoc('<p>Intro</p><pre><code>a = 1\nb = 2</code></pre>');
		const { editor } = bind(doc);
		await settle();
		editor.commands.setTextSelection(posIn(editor, 'b = 2'));
		press(editor, 'a', { mod: true });
		const { from, to } = editor.state.selection;
		expect(editor.state.doc.textBetween(from, to, '|')).toBe('a = 1\nb = 2');
		press(editor, 'a', { mod: true });
		expect(editor.state.selection.from).toBe(0);
		expect(editor.state.selection.to).toBe(editor.state.doc.content.size);
	});

	// UniqueID gives the block a rule made its id in a change of its own,
	// which Tiptap took for the next edit: Backspace emptied the line.
	it('Backspace right after a typed shortcut gives back what was typed, the block keeping its id', async () => {
		for (const [typed, made] of [
			['# ', 'heading'],
			['- ', 'bulletList'],
			['1. ', 'orderedList'],
			['> ', 'blockquote'],
			['[] ', 'taskList'],
		]) {
			const { doc } = pageDoc('<p>Intro</p><p></p>');
			const { editor } = bind(doc);
			await settle();
			const id = editor.state.doc.child(1).attrs[contract.id.attr];
			editor.commands.setTextSelection(editor.state.doc.content.size - 1);
			typeIn(editor, typed);
			expect(editor.state.doc.child(1).type.name, typed).toBe(made);
			expect(press(editor, 'Backspace'), typed).toBe(true);
			const line = editor.state.doc.child(1);
			expect([line.type.name, line.textContent], typed).toEqual(['paragraph', typed]);
			expect(line.attrs[contract.id.attr], typed).toBe(id);
			// The next Backspace is the browser's own, deleting a character.
			expect(press(editor, 'Backspace'), typed).toBe(false);
		}
	});

	it('a shortcut is final once more is typed', async () => {
		const { doc } = pageDoc('<p>Intro</p><p></p>');
		const { editor } = bind(doc);
		await settle();
		editor.commands.setTextSelection(editor.state.doc.content.size - 1);
		typeIn(editor, '# T');
		expect(press(editor, 'Backspace')).toBe(false);
		expect(editor.state.doc.child(1).type.name).toBe('heading');
		expect(editor.state.doc.child(1).textContent).toBe('T');
	});

	// The item went out of the list whole: its nested list and the items
	// after it were two lists side by side, which export as two.
	it('Backspace on a first item that holds a nested list leaves one list after it', async () => {
		for (const [html, kind] of [
			['<ul><li><p>First</p><ul><li><p>Child</p></li></ul></li><li><p>Second</p></li></ul>', 'bulletList'],
			[
				'<ul data-type="taskList"><li data-type="taskItem" data-checked="false"><p>First</p><ul data-type="taskList"><li data-type="taskItem" data-checked="true"><p>Child</p></li></ul></li><li data-type="taskItem" data-checked="false"><p>Second</p></li></ul>',
				'taskList',
			],
		]) {
			const { doc } = pageDoc(html);
			const { editor } = bind(doc);
			await settle();
			const list = editor.state.doc.child(0);
			const ids = [list.child(0).child(1).child(0), list.child(1)].map((item) => item.attrs[contract.id.attr]);
			editor.commands.setTextSelection(posIn(editor, 'First'));
			expect(press(editor, 'Backspace'), kind).toBe(true);
			await settle();
			const top = editor.state.doc;
			expect(top.childCount, kind).toBe(2);
			expect([top.child(0).type.name, top.child(0).textContent], kind).toEqual(['paragraph', 'First']);
			const after = top.child(1);
			expect(after.type.name, kind).toBe(kind);
			expect(after.attrs[contract.id.attr], kind).toBe(list.attrs[contract.id.attr]);
			expect(
				Array.from({ length: after.childCount }, (_, i) => [after.child(i).textContent, after.child(i).attrs[contract.id.attr]]),
				kind,
			).toEqual([
				['Child', ids[0]],
				['Second', ids[1]],
			]);
		}
	});

	// Tiptap's to-do rule joins only an item of the type it wraps in, and
	// the node beside a new to-do is a list: each typed to-do was a list.
	it('a to-do typed under a to-do list joins it', async () => {
		for (const typed of ['[] ', '- [ ] ']) {
			const { doc } = pageDoc('<ul data-type="taskList"><li data-type="taskItem" data-checked="false"><p>Book seats</p></li></ul><p></p>');
			const { editor } = bind(doc);
			await settle();
			const id = editor.state.doc.child(0).attrs[contract.id.attr];
			editor.commands.setTextSelection(editor.state.doc.content.size - 1);
			typeIn(editor, typed);
			typeIn(editor, 'Pack');
			await settle();
			const top = editor.state.doc;
			expect(top.childCount, typed).toBe(1);
			expect(top.child(0).type.name, typed).toBe('taskList');
			expect(top.child(0).attrs[contract.id.attr], typed).toBe(id);
			expect([top.child(0).child(0).textContent, top.child(0).child(1).textContent], typed).toEqual(['Book seats', 'Pack']);
			expect(new Set([top.child(0).child(0).attrs[contract.id.attr], top.child(0).child(1).attrs[contract.id.attr]]).size, typed).toBe(2);
		}
	});

	it('a to-do typed over a to-do list joins it, and Backspace takes it back', async () => {
		const { doc } = pageDoc('<p>Intro</p><p></p><ul data-type="taskList"><li data-type="taskItem" data-checked="false"><p>Pack</p></li></ul>');
		const { editor } = bind(doc);
		await settle();
		const id = editor.state.doc.child(2).attrs[contract.id.attr];
		editor.commands.setTextSelection(posIn(editor, 'Intro', 5) + 2);
		typeIn(editor, '[] ');
		const list = editor.state.doc.child(1);
		expect(editor.state.doc.childCount).toBe(2);
		expect([list.type.name, list.childCount, list.attrs[contract.id.attr]]).toEqual(['taskList', 2, id]);
		expect(press(editor, 'Backspace')).toBe(true);
		expect(editor.state.doc.childCount).toBe(3);
		expect([editor.state.doc.child(1).type.name, editor.state.doc.child(1).textContent]).toEqual(['paragraph', '[] ']);
		expect(editor.state.doc.child(2).attrs[contract.id.attr]).toBe(id);
	});

	it('the caret keeps lines of the page around it as it nears the scroller\'s edge', () => {
		const { doc } = pageDoc('<p>Intro</p>');
		const { editor } = bind(doc);
		const margin = editor.view.someProp('scrollMargin');
		const threshold = editor.view.someProp('scrollThreshold');
		expect(margin).toBeGreaterThanOrEqual(48);
		expect(threshold).toBeGreaterThanOrEqual(48);
	});
});
