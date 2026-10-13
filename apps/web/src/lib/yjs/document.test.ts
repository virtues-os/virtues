/**
 * When the editor shows the copy of a page this device kept, before the
 * server syncs: only for a page the server said this binding reads. A copy
 * kept from before the page was raised to a tree carries no stamp, so the
 * copy alone cannot say; shown, it would be text the page no longer holds,
 * and offline it would take edits the server refuses when the device is back.
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

import { createTreeDocument, createYjsDocument } from './document';

/** Open a page whose kept copy holds `text`, stamped `stamp` when given. */
function openWithKeptCopy(pageContract: number | null, text: string, stamp?: number) {
	const doc = createYjsDocument('pg_1', pageContract);
	const kept = fakes.made.persistence!;
	if (text) kept.doc.getText('content').insert(0, text);
	if (stamp !== undefined) kept.doc.getMap('meta').set('contract', stamp);
	kept.emit('synced');
	return { doc, provider: fakes.made.provider! };
}

describe('the copy this device kept', () => {
	it('is shown at once for a page the server said this binding reads', () => {
		const { doc } = openWithKeptCopy(0, 'Old notes');
		expect(doc.contractConfirmed).toBe(true);
		expect(get(doc.isSynced)).toBe(true);
		doc.destroy();
	});

	it('waits for the server when the server has not said', () => {
		const { doc, provider } = openWithKeptCopy(null, 'Old notes');
		expect(doc.contractConfirmed).toBe(false);
		expect(get(doc.isSynced)).toBe(false);
		// Offline, it is not taken as the page either.
		provider.emit('connection-error');
		expect(get(doc.isSynced)).toBe(false);
		// The server's sync is.
		provider.emit('sync');
		expect(get(doc.isSynced)).toBe(true);
		doc.destroy();
	});

	it('is never shown for a page written under a newer contract, nor is the server asked', () => {
		const { doc, provider } = openWithKeptCopy(1, 'Old notes');
		expect(get(doc.refused)).toContain('newer version');
		expect(provider.options.connect).toBe(false);
		provider.emit('connection-error');
		expect(get(doc.isSynced)).toBe(false);
		doc.destroy();
	});

	it('is not shown when it carries a newer stamp, whatever the server said', () => {
		const { doc, provider } = openWithKeptCopy(0, 'Old notes', 1);
		expect(get(doc.isSynced)).toBe(false);
		provider.emit('connection-error');
		expect(get(doc.isSynced)).toBe(false);
		doc.destroy();
	});

	it('takes offline edits, empty or not, on a page the server said this binding reads', () => {
		const { doc, provider } = openWithKeptCopy(0, '');
		expect(get(doc.isSynced)).toBe(false);
		provider.emit('connection-error');
		expect(get(doc.isSynced)).toBe(true);
		expect(provider.options.params.contract).toBe('0');
		doc.destroy();
	});
});

describe('a page converted while it is open', () => {
	it('says so when the server closes it with 4409, and keeps what it shows to copy out', () => {
		const { doc, provider } = openWithKeptCopy(0, 'Typed before the change');
		expect(get(doc.isSynced)).toBe(true);
		provider.emit('closed', { code: 4409, reason: 'your server converted this page; open it again' });
		expect(get(doc.refused)).toContain('converted this page');
		expect(get(doc.isSynced)).toBe(true);
		expect(doc.ytext.toString()).toBe('Typed before the change');
		doc.destroy();
	});
});

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

	it('keeps its copy apart from the markdown one, and asks for contract 1 on the socket', () => {
		const { doc, store, provider } = openTree(1, 1);
		expect(store.name).toBe('tree-pg_2');
		expect(provider.options.params.contract).toBe('1');
		expect(provider.options.connect).toBe(true);
		expect(doc.format).toBe('tree');
		doc.destroy();
	});

	it('shows the kept copy at once when it is this contract and holds a block', () => {
		const { doc } = openTree(1, 1, { blocks: 1, stamp: 1 });
		expect(doc.localCopyReadable()).toBe(true);
		expect(get(doc.isSynced)).toBe(true);
		doc.destroy();
	});

	it('never shows an empty kept copy, even offline', () => {
		const { doc, provider } = openTree(1, 1, { stamp: 1 });
		expect(get(doc.isSynced)).toBe(false);
		provider.emit('connection-error');
		expect(get(doc.isSynced)).toBe(false);
		doc.destroy();
	});

	it('never shows a kept copy stamped differently from the page', () => {
		const { doc, provider } = openTree(1, 1, { blocks: 1, stamp: 0 });
		expect(get(doc.isSynced)).toBe(false);
		provider.emit('connection-error');
		expect(get(doc.isSynced)).toBe(false);
		doc.destroy();
	});

	it('binds nothing when the server could not read the page', () => {
		const { doc, provider } = openTree(null, 1, { blocks: 1, stamp: 1 });
		expect(provider.options.connect).toBe(false);
		expect(get(doc.refused)).toBe("Your server couldn't read this page's document.");
		expect(get(doc.isSynced)).toBe(false);
		doc.destroy();
	});

	it('refuses a page the server holds empty, rather than binding an editor to it', () => {
		const { doc, provider } = openTree(1, 1);
		provider.emit('sync', true);
		expect(get(doc.isSynced)).toBe(false);
		expect(get(doc.refused)).toBe("This page's document is empty on your server.");
		doc.destroy();
	});

	it('syncs from the server when the server holds blocks', () => {
		const { doc, provider } = openTree(1, 1);
		doc.fragment.push([new Y.XmlElement('paragraph')]);
		provider.emit('sync', true);
		expect(get(doc.isSynced)).toBe(true);
		expect(get(doc.refused)).toBeNull();
		doc.destroy();
	});

	it('is read-only, and says so, on a server older than its contract', () => {
		const { doc } = openTree(1, 0);
		expect(doc.readOnly).toBe(true);
		expect(get(doc.refused)).toBe('Your server needs an update before it can save changes to this page.');
		doc.destroy();
	});

	it('drops the kept copy when the server refuses an edit, so opening the page again does not send it', () => {
		fakes.kept.clear();
		const first = openTree(1, 1, { blocks: 1, stamp: 1 });
		first.doc.ydoc.getText('content').insert(0, 'stray');
		first.provider.emit('closed', { code: 4422, reason: 'the document holds `content`' });
		expect(get(first.doc.refused)).toContain("couldn't accept an edit");
		// The open editor still shows it, to copy out.
		expect(first.doc.ydoc.getText('content').toString()).toBe('stray');
		first.doc.destroy();

		const again = createTreeDocument('pg_2', 1, 1);
		const store = fakes.made.persistence!;
		for (const update of fakes.kept.get(store.name) ?? []) Y.applyUpdate(store.doc, update, store);
		store.emit('synced');
		expect(again.ydoc.getText('content').toString()).toBe('');
		expect(get(again.isSynced)).toBe(false);
		again.destroy();
	});

	it('says the page needs a newer app when the server closes it with 4426', () => {
		const { doc, provider } = openTree(1, 1, { blocks: 1, stamp: 1 });
		expect(get(doc.isSynced)).toBe(true);
		provider.emit('closed', { code: 4426, reason: 'contract' });
		expect(get(doc.refused)).toContain('newer version');
		expect(get(doc.isSynced)).toBe(false);
		doc.destroy();
	});
});
