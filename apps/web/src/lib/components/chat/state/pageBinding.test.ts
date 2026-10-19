/**
 * The chat's page binding: a page is bound by its id, with no document of
 * its own, so no socket opens and no text is sent: the server reads the
 * page for the model from its own copy.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';

const fakes = vi.hoisted(() => {
	type Item = { type: string; id: string; title: string };
	const items: Item[] = [];
	return {
		items,
		store: {
			get items() {
				return items;
			},
			isAllowed: (type: string, id: string) => items.some((i) => i.type === type && i.id === id),
			addPage: vi.fn(async (id: string, title: string) => {
				if (items.some((i) => i.type === 'page' && i.id === id)) return;
				items.push({ type: 'page', id, title });
			}),
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

vi.mock('$lib/stores/editAllowList.svelte', () => ({ editAllowListStore: fakes.store }));
vi.mock('$lib/stores/window-shell.svelte', () => ({ windowShellStore: { openRouteInSplitOrActive: vi.fn() } }));

import { activePageContext, bindPage, grantEditPermission, openCreatedPage } from './pageBinding';

beforeEach(() => {
	fakes.items.length = 0;
	fakes.store.addPage.mockClear();
});

describe('binding a page in the chat', () => {
	it('binds the page by its id and title, and sends no text for it', async () => {
		await bindPage('page_tree', 'Trip');
		expect(fakes.items).toEqual([{ type: 'page', id: 'page_tree', title: 'Trip' }]);
		expect(activePageContext()).toEqual({ page_id: 'page_tree', page_title: 'Trip' });
	});

	it('binds one page at a time', async () => {
		await bindPage('page_a', 'A');
		await bindPage('page_b', 'B');
		expect(fakes.items.map((i) => i.id)).toEqual(['page_b']);
	});

	it('a page the model created is bound as it opens', () => {
		openCreatedPage('page_new', 'New');
		expect(activePageContext()).toEqual({ page_id: 'page_new', page_title: 'New' });
	});
});

describe('granting a page to a gated tool', () => {
	it('grants a page once', async () => {
		await grantEditPermission('page_md', 'page', 'Trip');
		await grantEditPermission('page_md', 'page', 'Trip');
		expect(fakes.store.addPage).toHaveBeenCalledTimes(1);
		expect(fakes.store.isAllowed('page', 'page_md')).toBe(true);
	});
});
