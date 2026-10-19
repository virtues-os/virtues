import { afterEach, describe, expect, it, vi } from 'vitest';

/** The upload's answer, held until a test lets it land or fail. */
let land: () => void = () => {};
let fail: (error: Error) => void = () => {};

vi.mock('$lib/api/client', () => ({
	announcePageFiles: vi.fn(async () => undefined),
	getMediaUploads: vi.fn(async () => ({})),
	sendPageFile: vi.fn(
		() =>
			new Promise<void>((resolve, reject) => {
				land = resolve;
				fail = reject;
			}),
	),
	UploadError: class extends Error {},
}));

const { nothingToLose } = await import('./quietReload');
const { resetUploads, startUpload } = await import('$lib/document/uploads');

const calm = { temporaryChat: false, setupShowing: false, replying: false, focused: null };

afterEach(() => {
	resetUploads();
	vi.useRealTimers();
});

describe('nothingToLose', () => {
	it('holds the reload while a page file is on its way, whose block is already in the page', async () => {
		expect(nothingToLose(calm)).toBe(true);
		startUpload('file_00c0ffee00c0ffee', new File(['bytes'], 'harbour.png', { type: 'image/png' }));
		expect(nothingToLose(calm)).toBe(false);
		land();
		await Promise.resolve();
		await Promise.resolve();
		expect(nothingToLose(calm)).toBe(true);
	});

	// Out of retries, the file counted as on its way no more, and a reload
	// while the app was hidden threw away its only copy.
	it('holds it for a page file out of retries, which Try again sends', async () => {
		vi.useFakeTimers();
		startUpload('file_00c0ffee00c0ffee', new File(['bytes'], 'harbour.png', { type: 'image/png' }));
		for (let i = 0; i < 5; i++) {
			fail(new Error("Couldn't reach your server."));
			await vi.advanceTimersByTimeAsync(60_000);
		}
		expect(nothingToLose(calm)).toBe(false);
	});

	it('holds it for what the page holds besides', () => {
		expect(nothingToLose({ ...calm, temporaryChat: true })).toBe(false);
		expect(nothingToLose({ ...calm, setupShowing: true })).toBe(false);
		expect(nothingToLose({ ...calm, replying: true })).toBe(false);
		expect(nothingToLose({ ...calm, focused: { value: 'half a key' } as unknown as Element })).toBe(false);
		expect(nothingToLose({ ...calm, focused: { value: '' } as unknown as Element })).toBe(true);
	});
});
