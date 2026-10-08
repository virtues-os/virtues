/**
 * When the editor shows the copy of a page this device kept, before the
 * server syncs: only for a page the server said this binding reads. A copy
 * kept from before the page was raised to a tree carries no stamp, so the
 * copy alone cannot say; shown, it would be text the page no longer holds,
 * and offline it would take edits the server refuses when the device is back.
 */

import { get } from 'svelte/store';
import { describe, expect, it, vi } from 'vitest';
import type * as Y from 'yjs';

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
vi.mock('$lib/config/backend', () => ({ getWsUrl: (path: string) => `ws://box${path}` }));

import { createYjsDocument } from './document';

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
