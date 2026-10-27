/**
 * MediaNode on a device that does not hold the file: what the server says
 * of it is drawn, a file the browser cannot draw is never loaded again and
 * again, and a file's card asks without waiting for an error that a link
 * never raises. On the device that holds it: a failed upload offers Try
 * again only short of a refusal, and its Remove holds the file for undo
 * once no page names it.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';
import { get } from 'svelte/store';
import type { PageFileState } from '$lib/api/client';
import type { ViewState } from '$lib/document/nodeviews.svelte';

/** What the server answers for each id, changed as a test goes on. */
let answer: (id: string) => PageFileState = () => ({ state: 'done' });
const asks: string[][] = [];

/** How this device's sends end, as the server answers them. */
let sendStatus = 201;

vi.mock('$lib/api/client', () => {
	class UploadError extends Error {
		constructor(
			message: string,
			readonly status: number,
		) {
			super(message);
		}
		get refused(): boolean {
			return this.status >= 400 && this.status < 500 && this.status !== 408 && this.status !== 429;
		}
	}
	return {
		getMediaUploads: vi.fn(async (ids: string[]) => {
			asks.push(ids);
			return Object.fromEntries(ids.map((id) => [id, answer(id)]));
		}),
		sendPageFile: vi.fn(async (id: string) => {
			if (sendStatus < 300) return { id };
			throw new UploadError(`Your server answered ${sendStatus}.`, sendStatus);
		}),
		announcePageFiles: vi.fn(async () => undefined),
		// The block was Removed, and its page saved without it: no page names the file.
		getNamedUploads: vi.fn(async (ids: string[]) => Object.fromEntries(ids.map((id) => [id, false]))),
		UploadError,
	};
});
vi.mock('$lib/utils/refRoutes', () => ({ fileIcon: () => 'mynaui:file' }));
vi.mock('$lib/document/links', () => ({ shareableHref: (src: string) => src }));
vi.mock('$lib/stores/contextMenu.svelte', () => ({ contextMenu: { show() {} } }));
vi.mock('$lib/stores/linkEditor.svelte', () => ({ linkEditor: { show() {} } }));
vi.mock('$lib/components/contextMenu/gesture', () => ({ onContextGesture() {} }));

const { default: MediaNode } = await import('./MediaNode.svelte');
const { resetUploads, startUpload, uploadBlocksBack, uploadBlocksLeft, uploadsAtStake, watchUpload, SENDER_TRIES_FOR_MS, ASK_SLOWER_MS, CHECK_AFTER_MS } =
	await import('$lib/document/uploads');
const { sendPageFile } = await import('$lib/api/client');

const ID = 'file_00c0ffee00c0ffee';
/** An upload cut off at `at`, as the server answers for it now: how long ago it stopped. */
const cutOffAt = (at: number): PageFileState => ({
	state: 'failed',
	filename: 'report.pdf',
	reason: 'report.pdf stopped before it finished uploading.',
	refused: false,
	stopped_ms_ago: Date.now() - at,
});

let target: HTMLElement;
let mounted: ReturnType<typeof mount> | null = null;
let removed = 0;

/** Draw a block; `onRemove` plays what the editor does when the block leaves the page. */
function show(type: string, attrs: Record<string, unknown>, editable = true, onRemove: () => void = () => {}): void {
	const view = {
		type,
		attrs,
		selected: false,
		editable,
		setAttrs() {},
		remove() {
			removed++;
			onRemove();
		},
		replaceWith() {},
		select() {},
		text: () => '',
		editor: {},
	} as unknown as ViewState;
	mounted = mount(MediaNode, { target, props: { view } });
	flushSync();
}

/** Let an ask go out and its answer land. */
async function settle(ms = 0): Promise<void> {
	await vi.advanceTimersByTimeAsync(ms);
	flushSync();
}

const text = () => (target.textContent ?? '').replace(/\s+/g, ' ').trim();

beforeEach(() => {
	vi.useFakeTimers();
	resetUploads();
	asks.length = 0;
	answer = () => ({ state: 'done' });
	sendStatus = 201;
	removed = 0;
	target = document.createElement('div');
	document.body.appendChild(target);
});

afterEach(() => {
	if (mounted) unmount(mounted);
	mounted = null;
	target.remove();
	resetUploads();
	vi.useRealTimers();
});

describe('MediaNode, the file held elsewhere', () => {
	for (const [type, tag, attrs] of [
		['image', 'img', { src: `/drive/${ID}`, alt: 'scan.tiff' }],
		['image', 'img', { src: `/api/drive/files/${ID}/download`, alt: 'scan.tiff' }],
		['audio', 'audio', { src: `/drive/${ID}`, name: 'clip.wma' }],
		['video', 'video', { src: `/drive/${ID}`, name: 'clip.mkv' }],
	] as const) {
		it(`loads a stored ${type} the browser cannot draw twice, and asks once (${attrs.src})`, async () => {
			show(type, attrs);
			const loaded: string[] = [];
			for (let i = 0; i < 6; i++) {
				const el = target.querySelector(tag);
				if (!el) break;
				const src = el.getAttribute('src') ?? '';
				if (!loaded.includes(src)) loaded.push(src);
				el.dispatchEvent(new Event('error'));
				flushSync();
				await settle(2000);
			}
			// Drawn again once from the server: it may have arrived between the
			// load and the asking.
			expect(loaded).toEqual([`/api/drive/files/${ID}/download`, `/api/drive/files/${ID}/download?arrived=1`]);
			expect(asks.length).toBe(1);
			expect(text()).not.toContain('uploading');
		});
	}

	it('draws an image again once, when it arrives after it did not load', async () => {
		answer = () => ({ state: 'uploading', filename: 'harbour.png', received: 5, total: 10 });
		show('image', { src: `/drive/${ID}`, alt: 'harbour.png' });
		target.querySelector('img')!.dispatchEvent(new Event('error'));
		flushSync();
		// Nothing said before the server answers.
		expect(text()).not.toContain('uploading');
		await settle();
		expect(text()).toContain('harbour.png is uploading on another device… 50%');
		answer = () => ({ state: 'done' });
		await settle(2000);
		const img = target.querySelector('img')!;
		expect(img.getAttribute('src')).toBe(`/api/drive/files/${ID}/download?arrived=1`);
		// Stored, and it still does not load: the browser cannot draw it.
		img.dispatchEvent(new Event('error'));
		flushSync();
		await settle(10_000);
		expect(target.querySelector('img')).toBeNull();
		expect(text()).toContain('harbour.png');
	});

	// The server's first word was `done` when the upload finished between the
	// image's load and the asking, and the picture stayed a broken card.
	it('draws an image that arrived between its load and the first answer', async () => {
		show('image', { src: `/drive/${ID}`, alt: 'harbour.png' });
		target.querySelector('img')!.dispatchEvent(new Event('error'));
		flushSync();
		await settle();
		expect(target.querySelector('img')?.getAttribute('src')).toBe(`/api/drive/files/${ID}/download?arrived=1`);
		await settle(60_000);
		expect(asks.length).toBe(1);
		expect(target.querySelector('img')).not.toBeNull();
	});

	it("asks for a file's card from the start, and draws its upload", async () => {
		answer = () => ({ state: 'uploading', filename: 'report.pdf', received: 1, total: 4 });
		show('file', { src: `/drive/${ID}`, name: 'report.pdf' });
		await settle();
		expect(asks).toEqual([[ID]]);
		expect(text()).toContain('report.pdf is uploading on another device… 25%');

		const at = Date.now();
		answer = () => cutOffAt(at);
		await settle(2000);
		// The device sending it may try again: no Remove yet.
		expect(text()).toContain('report.pdf stopped uploading on another device');
		expect(text()).toContain('That device may send it again.');
		expect(target.querySelector('button')).toBeNull();

		await settle(SENDER_TRIES_FOR_MS + ASK_SLOWER_MS);
		expect(text()).toContain("report.pdf didn't finish uploading");
		expect([...target.querySelectorAll('button')].map((b) => b.textContent?.trim())).toEqual(['Remove']);

		// Sent again later, and stored.
		answer = () => ({ state: 'done' });
		await settle(ASK_SLOWER_MS);
		const card = target.querySelector('a.doc-file-card');
		expect(card?.getAttribute('href')).toContain(`/api/drive/files/${ID}/download`);
		expect(text()).toBe('report.pdf PDF');
	});

	it("asks once for a stored file's card, and draws the card", async () => {
		show('file', { src: `/drive/${ID}`, name: 'report.pdf' });
		await settle(60_000);
		expect(asks).toEqual([[ID]]);
		expect(text()).toBe('report.pdf PDF');
	});

	it('asks nothing for a card that is no Drive file', async () => {
		show('file', { src: 'https://example.com/report.pdf', name: 'report.pdf' });
		await settle(10_000);
		expect(asks).toEqual([]);
	});
});

describe('MediaNode, the file held here', () => {
	const photo = () => new File(['bytes'], 'harbour.png', { type: 'image/png' });
	const buttons = () => [...target.querySelectorAll('button')].map((b) => b.textContent?.trim());

	it('offers no Try again for a file the server refused, which it would refuse again', async () => {
		sendStatus = 413;
		startUpload(ID, photo());
		show('image', { src: `/drive/${ID}`, alt: 'harbour.png' });
		await settle();
		expect(text()).toContain("Couldn't upload harbour.png");
		expect(buttons()).toEqual(['Remove']);
	});

	// Remove on the failed card let go of the file, so undo brought back a
	// block this device drew as another device's upload, never to be sent
	// again and with no Try again, though the bytes were in its memory.
	it('offers Try again for a file out of retries; Remove holds the file, and undo brings the block back with it', async () => {
		sendStatus = 507;
		startUpload(ID, photo());
		// The editor says when the block leaves the page and comes back (`followFileBlocks`).
		show('image', { src: `/drive/${ID}`, alt: 'harbour.png' }, true, () => uploadBlocksLeft([ID]));
		await settle(SENDER_TRIES_FOR_MS);
		expect(text()).toContain("Couldn't upload harbour.png");
		expect(buttons()).toEqual(['Try again', 'Remove']);
		expect(uploadsAtStake()).toBe(1);
		(target.querySelectorAll('button')[1] as HTMLButtonElement).click();
		flushSync();
		expect(removed).toBe(1);
		await settle(CHECK_AFTER_MS);
		expect(uploadsAtStake()).toBe(0);
		unmount(mounted!);
		mounted = null;

		// Undone: the block is back, its file still held here, and sent again.
		const sent = vi.mocked(sendPageFile).mock.calls.length;
		uploadBlocksBack([ID]);
		expect(get(watchUpload(ID))).toMatchObject({ phase: 'uploading', name: 'harbour.png' });
		expect(vi.mocked(sendPageFile).mock.calls.length).toBe(sent + 1);
		expect(uploadsAtStake()).toBe(1);
		show('image', { src: `/drive/${ID}`, alt: 'harbour.png' });
		await settle(SENDER_TRIES_FOR_MS);
		expect(text()).toContain("Couldn't upload harbour.png");
		expect(text()).not.toContain('another device');
		expect(buttons()).toEqual(['Try again', 'Remove']);
	});
});
