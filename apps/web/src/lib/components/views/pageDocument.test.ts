/**
 * Which editor a page opens in (`pageDocument.ts`, `PageContent.svelte`).
 *
 * The choice is made in `pageDocument.ts` and tested here against the
 * bindings themselves, with the socket and the device's copy faked. The
 * Svelte components cannot be compiled under vitest here (no Svelte plugin
 * in vitest.config.ts), so how `PageContent` and the panels use the choice
 * is checked in their source.
 */

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { get } from 'svelte/store';
import { describe, expect, it, vi } from 'vitest';
import * as Y from 'yjs';

const fakes = vi.hoisted(() => {
	type Handler = (...args: unknown[]) => void;
	class Emitter {
		private handlers = new Map<string, Handler[]>();
		on(event: string, handler: Handler) {
			this.handlers.set(event, [...(this.handlers.get(event) ?? []), handler]);
		}
		emit(event: string, ...args: unknown[]) {
			for (const handler of this.handlers.get(event) ?? []) handler(...args);
		}
		destroy() {}
	}
	const made: { provider?: FakeProvider; persistence?: FakePersistence } = {};
	class FakeProvider extends Emitter {
		awareness = {};
		constructor(
			public url: string,
			public room: string,
			public doc: Y.Doc,
			public options: { connect: boolean; params: Record<string, string> },
		) {
			super();
			made.provider = this;
		}
	}
	class FakePersistence extends Emitter {
		constructor(
			public name: string,
			public doc: Y.Doc,
		) {
			super();
			made.persistence = this;
		}
	}
	return { made, FakeProvider, FakePersistence };
});

vi.mock('y-websocket', () => ({ WebsocketProvider: fakes.FakeProvider }));
vi.mock('y-indexeddb', () => ({ IndexeddbPersistence: fakes.FakePersistence }));
vi.mock('$lib/config/backend', () => ({ getWsUrl: (p: string) => `ws://box${p}` }));

import { formatOf, mayShowKeptCopy, openPageDocument, stillLoading } from './pageDocument';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const source = (rel: string) => fs.readFileSync(path.resolve(HERE, rel), 'utf8');

describe('the editor a page opens in', () => {
	it('a block page binds its tree on the contract socket, kept under tree-<id>', () => {
		const page = openPageDocument('pg_tree', { format: 'tree', contract: 1, box_contract: 1 });
		expect(page.format).toBe('tree');
		expect(fakes.made.provider!.options.params).toEqual({ contract: '1' });
		expect(fakes.made.persistence!.name).toBe('tree-pg_tree');
		if (page.format !== 'tree') throw new Error('not a tree');
		expect(page.doc.readOnly).toBe(false);
		expect(page.readOnlyNotice).toBeNull();
		page.doc.destroy();
	});

	it('a markdown page, or a page from a server that sends no format, binds its text as today', () => {
		for (const description of [{ format: 'markdown', contract: 0 }, {}]) {
			const page = openPageDocument('pg_md', description);
			expect(page.format).toBe('markdown');
			expect(fakes.made.provider!.options.params).toEqual({ contract: '0' });
			expect(fakes.made.persistence!.name).toBe('v2-pg_md');
			page.doc.destroy();
		}
		expect(formatOf({})).toBe('markdown');
	});

	it('is read-only, and says so, when the server is older than the contract', () => {
		const page = openPageDocument('pg_tree', { format: 'tree', contract: 0, box_contract: 0 });
		if (page.format !== 'tree') throw new Error('not a tree');
		expect(page.doc.readOnly).toBe(true);
		expect(page.readOnlyNotice).toMatch(/needs an update/);
		expect(get(page.doc.refused)).toBe(page.readOnlyNotice);
		// The notice does not stop the page loading and showing.
		expect(stillLoading(page, false, page.readOnlyNotice)).toBe(true);
		page.doc.destroy();
	});

	it('a block page the server could not read is not shown, and not waited on', () => {
		const page = openPageDocument('pg_tree', { format: 'tree', contract: null, box_contract: 1 });
		const why = get(page.doc.refused);
		expect(why).toMatch(/couldn't read/);
		expect(fakes.made.provider!.options.connect).toBe(false);
		expect(stillLoading(page, false, why)).toBe(false);
		expect(mayShowKeptCopy(page, why)).toBe(false);
		page.doc.destroy();
	});
});

describe('showing the copy this device kept, when the server is slow', () => {
	it('a block page: only a copy holding a block, stamped as the page is', () => {
		const page = openPageDocument('pg_tree', { format: 'tree', contract: 1, box_contract: 1 });
		if (page.format !== 'tree') throw new Error('not a tree');
		// Empty: an editor bound to it would write its empty paragraph.
		expect(mayShowKeptCopy(page, null)).toBe(false);
		page.doc.fragment.insert(0, [new Y.XmlElement('paragraph')]);
		// A block, but no stamp: kept from before the page was raised.
		expect(mayShowKeptCopy(page, null)).toBe(false);
		page.doc.ydoc.getMap('meta').set('contract', 1);
		expect(mayShowKeptCopy(page, null)).toBe(true);
		// Not once the server has refused the binding.
		expect(mayShowKeptCopy(page, 'A newer version of Virtues wrote this page.')).toBe(false);
		page.doc.destroy();
	});

	it('a markdown page: as before, when the server said this binding reads it', () => {
		const page = openPageDocument('pg_md', { format: 'markdown', contract: 0 });
		expect(mayShowKeptCopy(page, null)).toBe(true);
		expect(mayShowKeptCopy(page, 'refused')).toBe(false);
		page.doc.destroy();
		const unknown = openPageDocument('pg_md', { format: 'markdown', contract: null });
		expect(mayShowKeptCopy(unknown, null)).toBe(false);
		unknown.doc.destroy();
	});
});

describe('PageContent, in its source', () => {
	const page = source('PageContent.svelte');

	it('mounts DocumentEditor for a block page, editable unless the server is older, and CodeMirror otherwise', () => {
		expect(page).toMatch(/openPageDocument\(pageId, data\)/);
		expect(page).toMatch(/\{#if treeDoc && isSynced\}[\s\S]*?<DocumentEditor[\s\S]*?editable=\{!treeDoc\.readOnly\}[\s\S]*?ai=\{treeAiDriver\}/);
		expect(page).toMatch(/\{:else if yjsDoc && isSynced\}[\s\S]*?<CodeMirrorEditor/);
	});

	it("keeps a block page's versions, copy and view on the server, and prints it", () => {
		expect(page).toMatch(/cutServerVersion\(pageId, description, 'auto', \{ keepalive \}\)/);
		expect(page).toMatch(/getPageMarkdown\(pageId\)/);
		expect(page).toMatch(/<MarkdownView \{pageId\} ydoc=\{treeDoc\.ydoc\} \/>/);
		expect(page).toMatch(/onPrint=\{format === "tree" && canPrint \? printPage : undefined\}/);
		expect(page).toMatch(/class="page-topbar" data-print="hide"/);
		expect(page).toMatch(/class:width-page=/);
	});

	it('the cover is as wide as the text in every width, the A4 sheet included', () => {
		const cover = source('../pages/PageCoverImage.svelte');
		const display = source('../../stores/pageDisplay.svelte.ts');
		const modes = /export type WidthMode = ([^;]+);/.exec(display)![1].match(/"(\w+)"/g)!.map((m) => m.slice(1, -1));
		expect(modes).toContain('page');
		const widthOf = (css: string, selector: string) =>
			new RegExp(`${selector.replace(/\./g, '\\.')} \\{[^}]*max-width: ([^;]+);`).exec(css)?.[1];
		for (const mode of modes) {
			expect(cover, mode).toMatch(new RegExp(`class:width-${mode}=\\{widthMode === "${mode}"\\}`));
			expect(widthOf(cover, `.cover-image-wrapper.width-${mode}`), mode).toBe(widthOf(page, `.page-inner.width-${mode}`));
		}
	});

	it('the history panel previews a block version with DocumentPreview and restores it on the server', () => {
		const panel = source('../pages/VersionHistoryPanel.svelte');
		expect(panel).toMatch(/<DocumentPreview html=\{preview\.html\} \/>/);
		expect(panel).toMatch(/restorePageVersion\(pageId, version\.id\)/);
		expect(panel).toMatch(/cutServerVersion\(pageId\)/);
	});

	it('display settings offer View as markdown on a block page instead of the raw toggle', () => {
		const settings = source('../pages/DisplaySettingsPopover.svelte');
		expect(settings).toMatch(/\{#if format === "tree"\}[\s\S]*View as markdown[\s\S]*\{:else\}[\s\S]*Raw markdown/);
	});

	it('the share sheet says what stays on the server', () => {
		const sheet = source('../applets/ShareSheet.svelte');
		expect(sheet).toContain("'1 applet or file stays on your server.'");
		expect(sheet).toContain('applets and files stay on your server.');
	});
});
