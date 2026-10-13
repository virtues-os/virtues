/**
 * The Share sheet, on the device sending one of the page's images. The
 * server refuses a share only once it has heard of the file, and shared
 * before, the page went without the image and the sheet said it isn't in
 * Drive: the sheet waits for this device's images itself.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';
import { writable } from 'svelte/store';

/** Images on the page this device is still sending. */
let coming = 0;
const stake = writable(0);

vi.mock('$lib/api/client', () => ({
	previewPublication: vi.fn(async () => ({
		title: 'Trip',
		html: '<p>Trip</p>',
		problem: null,
		size_bytes: 20,
		image_count: 1,
		links: [],
		looks_private: [],
		names: [],
		images_left_out: 0,
		embeds_left_out: 0,
		message_count: 0,
		attachments_left_out: 0,
		reads_data: false,
		queries: [],
	})),
	listPublications: vi.fn(async () => []),
	createPublication: vi.fn(),
	updatePublication: vi.fn(),
	revokePublication: vi.fn(),
}));
vi.mock('$lib/document/media', () => ({ imagesOnTheirWay: () => coming }));
vi.mock('$lib/document/uploads', () => ({ watchUploadsAtStake: () => ({ subscribe: stake.subscribe }) }));

const { default: ShareSheet } = await import('./ShareSheet.svelte');
const { previewPublication } = await import('$lib/api/client');

let target: HTMLElement;
let mounted: ReturnType<typeof mount> | null = null;

beforeEach(() => {
	coming = 0;
	stake.set(0);
	vi.mocked(previewPublication).mockClear();
	target = document.createElement('div');
	document.body.appendChild(target);
});

afterEach(() => {
	if (mounted) unmount(mounted);
	mounted = null;
	target.remove();
	document.body.replaceChildren();
});

const text = () => (document.body.textContent ?? '').replace(/\s+/g, ' ');

describe('ShareSheet, for a page', () => {
	it('shares nothing while an image on it is still on its way from here, and shows what leaves once it arrives', async () => {
		coming = 1;
		stake.set(1);
		mounted = mount(ShareSheet, { target, props: { open: true, producer: { kind: 'page', id: 'page_1' }, onClose() {} } });
		flushSync();
		await vi.waitFor(() => expect(text()).toContain("An image on this page hasn't finished uploading"));
		expect(previewPublication).not.toHaveBeenCalled();
		expect(text()).not.toContain('Create link');

		coming = 0;
		stake.set(0);
		flushSync();
		await vi.waitFor(() => expect(previewPublication).toHaveBeenCalledOnce());
		await vi.waitFor(() => expect(text()).toContain('What leaves your server'));
	});
});
