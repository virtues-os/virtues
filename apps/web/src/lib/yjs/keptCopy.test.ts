/**
 * The markdown copy a device kept of a page that has since moved to blocks
 * (`keptCopy.ts`): sent to the server once, then deleted, and the person
 * told when it held typing the page never had.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';
import * as Y from 'yjs';

vi.mock('svelte-sonner', () => ({ toast: { info: vi.fn() } }));
vi.mock('$lib/api/client', () => ({ sendDeviceCopy: vi.fn() }));

import { toast } from 'svelte-sonner';
import {
	EARLIER_COPY_NOTICE,
	KEPT_NOTICE,
	keepEarlierTreeCopies,
	keepMarkdownCopy,
	lineageOf,
	resetKeptCopies,
	type KeptStore,
} from './keptCopy';

/** A kept copy holding `text`, or nothing; records what happens to it. */
function store(text: string | null) {
	const doc = new Y.Doc();
	if (text !== null) doc.getText('content').insert(0, text);
	const what = { cleared: false, closed: false, name: '' };
	const kept: KeptStore = {
		doc,
		clear: async () => void (what.cleared = true),
		close: () => void (what.closed = true),
	};
	const open = vi.fn(async (name: string) => {
		what.name = name;
		return kept;
	});
	return { open, what, doc };
}

beforeEach(() => {
	resetKeptCopies();
	vi.mocked(toast.info).mockClear();
});

describe('keepMarkdownCopy', () => {
	it("sends the device's copy, deletes it, and says so when the server kept typing from it", async () => {
		const { open, what, doc } = store('Groceries\nEggs\n');
		const send = vi.fn(async () => ({ kept: { version_number: 3 } }));
		await keepMarkdownCopy('pg_1', open, send);
		expect(what.name).toBe('v2-pg_1');
		expect(send).toHaveBeenCalledOnce();
		const [page, state] = send.mock.calls[0] as unknown as [string, string];
		expect(page).toBe('pg_1');
		const back = new Y.Doc();
		Y.applyUpdate(back, Uint8Array.from(atob(state), (c) => c.charCodeAt(0)));
		expect(back.getText('content').toString()).toBe(doc.getText('content').toString());
		expect(what.cleared).toBe(true);
		expect(toast.info).toHaveBeenCalledWith(KEPT_NOTICE);

		// Once per page while the app runs.
		await keepMarkdownCopy('pg_1', open, send);
		expect(send).toHaveBeenCalledOnce();
	});

	it('deletes a copy that held nothing new, saying nothing', async () => {
		const { open, what } = store('Groceries\n');
		const send = vi.fn(async () => ({ kept: null }));
		await keepMarkdownCopy('pg_2', open, send);
		expect(what.cleared).toBe(true);
		expect(toast.info).not.toHaveBeenCalled();
	});

	it('deletes an empty copy without sending it', async () => {
		const { open, what } = store(null);
		const send = vi.fn();
		await keepMarkdownCopy('pg_3', open, send);
		expect(send).not.toHaveBeenCalled();
		expect(what.cleared).toBe(true);
	});

	it('keeps the copy on this device when the server is out of reach, and sends it the next time', async () => {
		const { open, what } = store('Groceries\nEggs\n');
		const send = vi.fn(async () => {
			throw new Error('offline');
		});
		await keepMarkdownCopy('pg_4', open, send);
		expect(what.closed).toBe(true);
		expect(what.cleared).toBe(false);
		await keepMarkdownCopy('pg_4', open, send);
		expect(send).toHaveBeenCalledTimes(2);
	});

	it('does nothing where the device keeps no copy', async () => {
		const send = vi.fn();
		await keepMarkdownCopy('pg_5', async () => null, send);
		expect(send).not.toHaveBeenCalled();
	});
});

/** Block stores this device keeps, by name: each a doc holding `blocks` paragraphs; records what happens to each. */
function treeStores(stores: Record<string, number>) {
	const what = new Map<string, { cleared: boolean; closed: boolean }>();
	const open = vi.fn(async (name: string) => {
		if (!(name in stores)) return null;
		const doc = new Y.Doc();
		const fragment = doc.getXmlFragment('doc');
		for (let i = 0; i < stores[name]; i++) fragment.push([new Y.XmlElement('paragraph')]);
		const state = { cleared: false, closed: false };
		what.set(name, state);
		const kept: KeptStore = {
			doc,
			clear: async () => void (state.cleared = true),
			close: () => void (state.closed = true),
		};
		return kept;
	});
	return { open, what, list: async () => Object.keys(stores) };
}

describe('lineageOf', () => {
	it("reads the lineage off a page's block store, and nothing off another's", () => {
		expect(lineageOf('tree-pg_1.00ab', 'pg_1')).toBe('00ab');
		expect(lineageOf('tree-pg_1', 'pg_1')).toBeNull();
		expect(lineageOf('tree-pg_10.00ab', 'pg_1')).toBeUndefined();
		expect(lineageOf('tree-pg_1.', 'pg_1')).toBeUndefined();
		expect(lineageOf('v2-pg_1', 'pg_1')).toBeUndefined();
	});
});

describe('keepEarlierTreeCopies', () => {
	it("sends each copy of an earlier document with its lineage, deletes it, and leaves the page's own", async () => {
		const { open, what, list } = treeStores({
			'tree-pg_6.aaaa': 1,
			'tree-pg_6': 2,
			'tree-pg_6.bbbb': 1,
			'tree-pg_60.cccc': 1,
			'v2-pg_6': 1,
		});
		const send = vi.fn(async (_page: string, _state: string, lineage?: string | null) => ({
			kept: lineage === 'aaaa' ? { version_number: 4 } : null,
		}));
		await keepEarlierTreeCopies('pg_6', 'bbbb', list, open, send);
		expect(send.mock.calls.map((c) => [c[0], c[2]])).toEqual([
			['pg_6', 'aaaa'],
			['pg_6', null],
		]);
		const [, state] = send.mock.calls[0] as unknown as [string, string];
		const back = new Y.Doc();
		Y.applyUpdate(back, Uint8Array.from(atob(state), (c) => c.charCodeAt(0)));
		expect(back.getXmlFragment('doc').length).toBe(1);
		expect(what.get('tree-pg_6.aaaa')?.cleared).toBe(true);
		expect(what.get('tree-pg_6')?.cleared).toBe(true);
		expect(what.has('tree-pg_6.bbbb')).toBe(false);
		expect(what.has('tree-pg_60.cccc')).toBe(false);
		expect(toast.info).toHaveBeenCalledOnce();
		expect(toast.info).toHaveBeenCalledWith(EARLIER_COPY_NOTICE);

		// Once per page and lineage while the app runs.
		await keepEarlierTreeCopies('pg_6', 'bbbb', list, open, send);
		expect(send).toHaveBeenCalledTimes(2);
	});

	it('keeps a copy on this device when the server is out of reach, and sends it the next time', async () => {
		const { open, what, list } = treeStores({ 'tree-pg_7': 1, 'tree-pg_7.dddd': 1 });
		const send = vi.fn(async () => {
			throw new Error('offline');
		});
		await keepEarlierTreeCopies('pg_7', 'dddd', list, open, send);
		expect(what.get('tree-pg_7')).toEqual({ cleared: false, closed: true });
		await keepEarlierTreeCopies('pg_7', 'dddd', list, open, send);
		expect(send).toHaveBeenCalledTimes(2);
	});

	it("looks for the copy from before lineages where the browser can't list its stores", async () => {
		const { open } = treeStores({ 'tree-pg_8': 1 });
		const send = vi.fn(async () => ({ kept: null }));
		await keepEarlierTreeCopies('pg_8', 'eeee', async () => null, open, send);
		expect(open).toHaveBeenCalledWith('tree-pg_8');
		expect(send).toHaveBeenCalledOnce();

		// A page from before lineages binds that copy itself.
		const none = treeStores({ 'tree-pg_9': 1 });
		await keepEarlierTreeCopies('pg_9', null, async () => null, none.open, send);
		expect(none.open).not.toHaveBeenCalled();
	});
});
