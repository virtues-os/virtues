/**
 * A block page's binding, and when the editor shows the copy of the page
 * this device kept before the server syncs: only for a page the server said
 * this binding reads, stamped as the page is and holding a block. Shown
 * otherwise, it would be text the page no longer holds, and offline it would
 * take edits the server refuses when the device is back.
 */

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
	/** What each store holds, by name: the updates it saw, as y-indexeddb keeps them. */
	const kept = new Map<string, Uint8Array[]>();
	/** y-websocket's provider, as far as the binding uses it. */
	class FakeProvider extends Emitter {
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
	/** y-indexeddb's persistence: the copy this device kept. */
	class FakePersistence extends Emitter {
		private keep = (update: Uint8Array, origin: unknown) => {
			if (origin !== this) kept.set(this.name, [...(kept.get(this.name) ?? []), update]);
		};
		constructor(
			public name: string,
			public doc: Y.Doc,
		) {
			super();
			made.persistence = this;
			doc.on('update', this.keep);
		}
		clearData() {
			this.doc.off('update', this.keep);
			kept.delete(this.name);
			return Promise.resolve();
		}
		destroy() {
			this.doc.off('update', this.keep);
		}
	}
	return { made, kept, FakeProvider, FakePersistence };
});

vi.mock('y-websocket', () => ({ WebsocketProvider: fakes.FakeProvider }));
vi.mock('y-indexeddb', () => ({ IndexeddbPersistence: fakes.FakePersistence }));
vi.mock('$lib/config/backend', () => ({ getWsUrl: (path: string) => `ws://box${path}` }));

import contractJson from '$contract';
import { createTreeDocument } from './document';

/** The contract this app's block editor reads. */
const V: number = (contractJson as { version: number }).version;

describe('a block page', () => {
	/** Open a block page whose kept copy holds `blocks` paragraphs, stamped `stamp`. */
	function openTree(
		pageContract: number | null,
		boxContract: number | null,
		kept: { blocks?: number; stamp?: number } = {},
	) {
		const doc = createTreeDocument('pg_2', pageContract, boxContract);
		const store = fakes.made.persistence!;
		const fragment = store.doc.getXmlFragment('doc');
		for (let i = 0; i < (kept.blocks ?? 0); i++) fragment.push([new Y.XmlElement('paragraph')]);
		if (kept.stamp !== undefined) store.doc.getMap('meta').set('contract', kept.stamp);
		store.emit('synced');
		return { doc, store, provider: fakes.made.provider! };
	}

	it('keeps its copy apart from the markdown one (`v2-<id>`), and asks for its contract on the socket', () => {
		const { doc, store, provider } = openTree(V, V);
		expect(store.name).toBe('tree-pg_2');
		expect(provider.options.params.contract).toBe(String(V));
		expect(provider.options.connect).toBe(true);
		expect(doc.format).toBe('tree');
		doc.destroy();
	});

	it('shows the kept copy at once when it is this contract and holds a block', () => {
		const { doc } = openTree(V, V, { blocks: 1, stamp: V });
		expect(doc.localCopyReadable()).toBe(true);
		expect(get(doc.isSynced)).toBe(true);
		doc.destroy();
	});

	it('never shows an empty kept copy, even offline', () => {
		const { doc, provider } = openTree(V, V, { stamp: V });
		expect(get(doc.isSynced)).toBe(false);
		provider.emit('connection-error');
		expect(get(doc.isSynced)).toBe(false);
		doc.destroy();
	});

	it('never shows a kept copy stamped differently from the page', () => {
		const { doc, provider } = openTree(V, V, { blocks: 1, stamp: 0 });
		expect(get(doc.isSynced)).toBe(false);
		provider.emit('connection-error');
		expect(get(doc.isSynced)).toBe(false);
		doc.destroy();
	});

	it('binds nothing when the server could not read the page', () => {
		const { doc, provider } = openTree(null, V, { blocks: 1, stamp: V });
		expect(provider.options.connect).toBe(false);
		expect(get(doc.refused)).toBe("Your server couldn't read this page's document.");
		expect(get(doc.isSynced)).toBe(false);
		doc.destroy();
	});

	it('refuses a page the server holds empty, rather than binding an editor to it', () => {
		const { doc, provider } = openTree(V, V);
		provider.emit('sync', true);
		expect(get(doc.isSynced)).toBe(false);
		expect(get(doc.refused)).toBe("This page's document is empty on your server.");
		doc.destroy();
	});

	it('syncs from the server when the server holds blocks', () => {
		const { doc, provider } = openTree(V, V);
		doc.fragment.push([new Y.XmlElement('paragraph')]);
		provider.emit('sync', true);
		expect(get(doc.isSynced)).toBe(true);
		expect(get(doc.refused)).toBeNull();
		doc.destroy();
	});

	it('is read-only, and says so, on a server older than its contract', () => {
		const { doc } = openTree(V, V - 1);
		expect(doc.readOnly).toBe(true);
		expect(get(doc.refused)).toBe('Your server needs an update before it can save changes to this page.');
		doc.destroy();
	});

	it('drops the kept copy when the server refuses an edit, so opening the page again does not send it', () => {
		fakes.kept.clear();
		const first = openTree(V, V, { blocks: 1, stamp: V });
		first.doc.ydoc.getText('content').insert(0, 'stray');
		first.provider.emit('closed', { code: 4422, reason: 'the document holds `content`' });
		expect(get(first.doc.refused)).toContain("couldn't accept an edit");
		// The open editor still shows it, to copy out.
		expect(first.doc.ydoc.getText('content').toString()).toBe('stray');
		first.doc.destroy();

		const again = createTreeDocument('pg_2', V, V);
		const store = fakes.made.persistence!;
		for (const update of fakes.kept.get(store.name) ?? []) Y.applyUpdate(store.doc, update, store);
		store.emit('synced');
		expect(again.ydoc.getText('content').toString()).toBe('');
		expect(get(again.isSynced)).toBe(false);
		again.destroy();
	});

	it("keeps its copy under the document's lineage, so a copy of an earlier document is never bound", () => {
		const doc = createTreeDocument('pg_2', V, V, '0a1b2c3d4e5f6a7b');
		expect(fakes.made.persistence!.name).toBe('tree-pg_2.0a1b2c3d4e5f6a7b');
		doc.destroy();
		const before = createTreeDocument('pg_2', V, V, null);
		expect(fakes.made.persistence!.name).toBe('tree-pg_2');
		before.destroy();
	});

	it('is never shown for a page written under a newer contract, nor is the server asked', () => {
		const { doc, provider } = openTree(V + 1, V + 1, { blocks: 1, stamp: V + 1 });
		expect(get(doc.refused)).toContain('newer version');
		expect(provider.options.connect).toBe(false);
		provider.emit('connection-error');
		expect(get(doc.isSynced)).toBe(false);
		doc.destroy();
	});

	it('says the page needs a newer app when the server closes it with 4426', () => {
		const { doc, provider } = openTree(V, V, { blocks: 1, stamp: V });
		expect(get(doc.isSynced)).toBe(true);
		provider.emit('closed', { code: 4426, reason: 'contract' });
		expect(get(doc.refused)).toContain('newer version');
		expect(get(doc.isSynced)).toBe(false);
		doc.destroy();
	});
});
