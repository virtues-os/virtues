/**
 * The chat's page binding: a markdown page is bound with its Yjs document,
 * so each turn carries the editor's text; a block page is bound with none,
 * so no socket opens for the server to refuse, and no text is sent for the
 * server to ignore.
 */

import { writable } from 'svelte/store';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const fakes = vi.hoisted(() => {
	type Item = { type: string; id: string; title: string; yjsDoc?: unknown };
	const items: Item[] = [];
	return {
		items,
		getPage: vi.fn(),
		createYjsDocument: vi.fn(),
		store: {
			get items() {
				return items;
			},
			isAllowed: (type: string, id: string) => items.some((i) => i.type === type && i.id === id),
			async addPage(id: string, title: string, yjsDoc?: unknown) {
				if (items.some((i) => i.type === 'page' && i.id === id)) return;
				items.push({ type: 'page', id, title, yjsDoc });
			},
			async add(item: Item) {
				items.push(item);
			},
			async remove(type: string, id: string) {
				const at = items.findIndex((i) => i.type === type && i.id === id);
				if (at >= 0) items.splice(at, 1);
			},
		},
	};
});

vi.mock('$lib/api/client', () => ({ getPage: fakes.getPage }));
vi.mock('$lib/stores/editAllowList.svelte', () => ({ editAllowListStore: fakes.store }));
vi.mock('$lib/stores/window-shell.svelte', () => ({ windowShellStore: { openRouteInSplitOrActive: vi.fn() } }));
vi.mock('$lib/yjs', () => ({ createYjsDocument: fakes.createYjsDocument }));

import { activePageContext, bindPage, grantEditPermission, openCreatedPage } from './pageBinding';

/** A markdown page's document as the chat reads it: synced, holding `text`. */
function markdownDoc(text: string, synced = true) {
	return { ytext: { toString: () => text }, isSynced: writable(synced), destroy: vi.fn() };
}

beforeEach(() => {
	fakes.items.length = 0;
	fakes.getPage.mockReset();
	fakes.createYjsDocument.mockReset();
});

describe('binding a page in the chat', () => {
	it('opens no document for a block page, and sends no text for it', async () => {
		fakes.getPage.mockResolvedValue({ id: 'page_tree', format: 'tree', contract: 1, box_contract: 1 });
		await bindPage('page_tree', 'Trip');
		expect(fakes.createYjsDocument).not.toHaveBeenCalled();
		expect(fakes.items).toEqual([{ type: 'page', id: 'page_tree', title: 'Trip', yjsDoc: undefined }]);
		expect(activePageContext()).toEqual({ page_id: 'page_tree', page_title: 'Trip' });
	});

	it('binds a markdown page with its document, and sends its text once synced', async () => {
		const doc = markdownDoc('# Trip\n\nLunch with Nick.');
		fakes.getPage.mockResolvedValue({ id: 'page_md', format: 'markdown', contract: 0, box_contract: 1 });
		fakes.createYjsDocument.mockReturnValue(doc);
		await bindPage('page_md', 'Trip');
		expect(fakes.createYjsDocument).toHaveBeenCalledWith('page_md', 0);
		expect(activePageContext()).toEqual({
			page_id: 'page_md',
			page_title: 'Trip',
			content: '# Trip\n\nLunch with Nick.',
		});
	});

	it('sends no text for a markdown page that has not synced, rather than an empty page', async () => {
		fakes.getPage.mockResolvedValue({ id: 'page_md', format: 'markdown', contract: 0 });
		fakes.createYjsDocument.mockReturnValue(markdownDoc('', false));
		await bindPage('page_md', 'Trip');
		expect(activePageContext()).toEqual({ page_id: 'page_md', page_title: 'Trip' });
	});

	it('binds as today on a server that sends no format or contract', async () => {
		fakes.getPage.mockResolvedValue({ id: 'page_old', title: 'Old', content: 'Notes' });
		fakes.createYjsDocument.mockReturnValue(markdownDoc('Notes'));
		await bindPage('page_old', 'Old');
		expect(fakes.createYjsDocument).toHaveBeenCalledWith('page_old', 0);
		expect(activePageContext()?.content).toBe('Notes');
	});

	it('binds without a document when the server cannot describe the page', async () => {
		fakes.getPage.mockRejectedValue(new Error('offline'));
		await bindPage('page_x', 'X');
		expect(fakes.createYjsDocument).not.toHaveBeenCalled();
		expect(activePageContext()).toEqual({ page_id: 'page_x', page_title: 'X' });
	});

	it('a page the model created is bound without text until the page opens', () => {
		openCreatedPage('page_new', 'New');
		expect(activePageContext()).toEqual({ page_id: 'page_new', page_title: 'New' });
	});
});

describe('granting a page to a gated tool', () => {
	it('opens no document for a block page', async () => {
		fakes.getPage.mockResolvedValue({ id: 'page_tree', format: 'tree', contract: 1 });
		await grantEditPermission('page_tree', 'page', 'Trip');
		expect(fakes.createYjsDocument).not.toHaveBeenCalled();
		expect(fakes.store.isAllowed('page', 'page_tree')).toBe(true);
	});

	it('opens no second document for a page already granted', async () => {
		fakes.getPage.mockResolvedValue({ id: 'page_md', format: 'markdown', contract: 0 });
		fakes.createYjsDocument.mockReturnValue(markdownDoc('Text'));
		await grantEditPermission('page_md', 'page', 'Trip');
		await grantEditPermission('page_md', 'page', 'Trip');
		expect(fakes.createYjsDocument).toHaveBeenCalledTimes(1);
	});
});
