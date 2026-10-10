/**
 * A block page's history goes through the server: a version is asked for
 * with no document, so nothing this browser holds enters the history
 * unchecked, and a restore is one request the server carries out.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';
import * as Y from 'yjs';

const api = vi.hoisted(() => ({
	request: vi.fn(),
	apiSend: vi.fn(),
}));

vi.mock('$lib/api/client', () => ({
	request: api.request,
	listPageVersions: vi.fn(async () => ({ versions: [] })),
	getPageVersion: vi.fn(),
	restorePageVersionRequest: (pageId: string, versionId: string) =>
		api.apiSend(
			'POST',
			`/pages/${encodeURIComponent(pageId)}/versions/${encodeURIComponent(versionId)}/restore`,
			{},
		),
}));

import { cutServerVersion, restorePageVersion, saveVersion } from './versions';

beforeEach(() => {
	api.request.mockReset();
	api.apiSend.mockReset();
});

describe("a block page's versions", () => {
	it('are cut by the server: the request carries no snapshot', async () => {
		api.request.mockResolvedValue({ id: 'v1', version_number: 3 });
		const version = await cutServerVersion('page_abc', 'Auto-saved (idle)', 'auto', { keepalive: true });
		expect(version).toEqual({ id: 'v1', version_number: 3 });
		const [path, init] = api.request.mock.calls[0];
		expect(path).toBe('/pages/page_abc/versions');
		expect(init).toMatchObject({ method: 'POST', keepalive: true });
		const body = JSON.parse(init.body);
		expect(body).toEqual({ description: 'Auto-saved (idle)', created_by: 'auto' });
		expect(body).not.toHaveProperty('snapshot');
	});

	it('say so with null when the server refuses, as a markdown save does', async () => {
		api.request.mockRejectedValue(new Error('down'));
		expect(await cutServerVersion('page_abc')).toBeNull();
	});

	it('are put back by one request to the restore route, which reports the notes', async () => {
		api.apiSend.mockResolvedValue({ version_number: 7, notes: [{ at: 'block 2', message: 'became a link' }] });
		const restored = await restorePageVersion('page_abc', 'ver_1');
		expect(api.apiSend).toHaveBeenCalledWith('POST', '/pages/page_abc/versions/ver_1/restore', {});
		expect(restored).toEqual({ version_number: 7, notes: [{ at: 'block 2', message: 'became a link' }] });
	});
});

describe("a markdown page's versions", () => {
	it('still carry the snapshot this browser holds', async () => {
		api.request.mockResolvedValue({ id: 'v2' });
		const doc = new Y.Doc();
		doc.getText('content').insert(0, 'Lunch with Nick.');
		await saveVersion(doc, 'page_md', 'Saved');
		const body = JSON.parse(api.request.mock.calls[0][1].body);
		expect(typeof body.snapshot).toBe('string');
		expect(body.content_preview).toBe('Lunch with Nick.');
	});
});
