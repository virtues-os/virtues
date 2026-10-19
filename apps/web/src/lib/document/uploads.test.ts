// @vitest-environment happy-dom
/**
 * A page's file on its way. On the device holding it (`startUpload`): what
 * is at stake while its bytes have not reached the server, and when it is
 * sent again. On every other device (`askUpload`): what the server says,
 * asked again until the file is stored or refused, every watched block's in
 * one request.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { PageFileState } from '$lib/api/client';

/** What the server answers for each id, changed as a test goes on. */
let answer: (id: string) => PageFileState = () => ({ state: 'missing' });
const asks: string[][] = [];
/** How each send ends, as the server answers it: a status, 0 for no answer at all; or a promise of one. */
let sendEnds: () => number | Promise<number> = () => 201;
const sends: string[] = [];
/** The ids said to be coming, each time they are said. */
const announced: string[][] = [];
/** Whether a page names each id, as the server answers. */
let named: (id: string) => boolean = () => true;

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
			sends.push(id);
			const status = await sendEnds();
			if (status >= 200 && status < 300) return { id };
			throw new UploadError(status ? `Your server answered ${status}.` : "Couldn't reach your server.", status);
		}),
		announcePageFiles: vi.fn(async (files: { id: string }[]) => {
			announced.push(files.map((f) => f.id));
		}),
		getNamedUploads: vi.fn(async (ids: string[]) => Object.fromEntries(ids.map((id) => [id, named(id)]))),
		UploadError,
	};
});

const {
	ANNOUNCE_EVERY_MS,
	ASK_AT_ONCE,
	ASK_SLOWER_MS,
	ASK_SLOWEST_MS,
	CHECK_AFTER_MS,
	MISSING_GRACE_MS,
	SEND_AT_ONCE,
	SENDER_TRIES_FOR_MS,
	TRY_AGAIN_EVERY_MS,
	askUpload,
	resetUploads,
	retryUpload,
	serverBack,
	startUpload,
	uploadBlocksLeft,
	uploadsAtStake,
	watchUpload,
} = await import('./uploads');
const { get } = await import('svelte/store');

const ID = 'file_00c0ffee00c0ffee';
const uploading: PageFileState = { state: 'uploading', filename: 'harbour.png', received: 10, total: 20 };
/** An upload cut off at `at`, as the server answers for it now: how long ago it stopped. */
const cutOffAt = (at: number): PageFileState => ({
	state: 'failed',
	filename: 'harbour.png',
	reason: 'harbour.png stopped before it finished uploading.',
	refused: false,
	stopped_ms_ago: Date.now() - at,
});

/** Every state `id` is told, in order, while watched. */
function watch(id: string): { told: string[]; stop: () => void } {
	const told: string[] = [];
	const stop = askUpload(id).subscribe((u) => told.push(u.state));
	return { told, stop };
}

beforeEach(() => {
	vi.useFakeTimers();
	resetUploads();
	asks.length = 0;
	sends.length = 0;
	announced.length = 0;
	answer = () => ({ state: 'missing' });
	sendEnds = () => 201;
	named = () => true;
});

afterEach(() => {
	resetUploads();
	vi.useRealTimers();
});

describe('askUpload', () => {
	it('says nothing of the file before the server answers, then follows it until it is stored', async () => {
		answer = () => ({ state: 'missing' });
		const { told, stop } = watch(ID);
		expect(told).toEqual(['asking']);
		await vi.advanceTimersByTimeAsync(0);
		// The block came before its file: missing is waiting, at first.
		expect(told.at(-1)).toBe('waiting');
		answer = () => uploading;
		await vi.advanceTimersByTimeAsync(2000);
		expect(told.at(-1)).toBe('uploading');
		answer = () => ({ state: 'done' });
		await vi.advanceTimersByTimeAsync(2000);
		expect(told.at(-1)).toBe('done');
		const asked = asks.length;
		await vi.advanceTimersByTimeAsync(5 * 60_000);
		expect(asks.length, 'a stored file is asked about no more').toBe(asked);
		stop();
	});

	it('keeps asking after a cut-off, and hears the file arrive when the device holding it sends it again', async () => {
		const at = Date.now();
		answer = () => cutOffAt(at);
		const { told, stop } = watch(ID);
		await vi.advanceTimersByTimeAsync(0);
		// Short of a refusal: the device holding it may still send it.
		expect(told.at(-1)).toBe('stopped');
		answer = () => uploading;
		await vi.advanceTimersByTimeAsync(2000);
		expect(told.at(-1)).toBe('uploading');
		answer = () => ({ state: 'done' });
		await vi.advanceTimersByTimeAsync(2000);
		expect(told.at(-1)).toBe('done');
		stop();
	});

	it('says a stop is final once the device holding it has stopped trying, and still hears a later try', async () => {
		const at = Date.now();
		answer = () => cutOffAt(at);
		const { told, stop } = watch(ID);
		await vi.advanceTimersByTimeAsync(0);
		expect(told.at(-1)).toBe('stopped');
		await vi.advanceTimersByTimeAsync(SENDER_TRIES_FOR_MS + ASK_SLOWER_MS);
		expect(told.at(-1)).toBe('failed');
		// Try again, pressed on that device minutes later.
		answer = () => ({ state: 'done' });
		await vi.advanceTimersByTimeAsync(ASK_SLOWER_MS);
		expect(told.at(-1)).toBe('done');
		stop();
	});

	// A device that first asked long after the stop timed the sending
	// device's tries from its own first answer: it said "may send it again",
	// with no Remove, for two minutes after that device had stopped trying.
	it('times the sending device’s tries from the stop, whenever this device first asks', async () => {
		const longAgo = Date.now() - 20 * 60_000;
		answer = () => cutOffAt(longAgo);
		const late = watch(ID);
		await vi.advanceTimersByTimeAsync(0);
		expect(late.told).toEqual(['asking', 'failed']);
		late.stop();
		resetUploads();

		const lately = Date.now() - (SENDER_TRIES_FOR_MS - 5000);
		answer = () => cutOffAt(lately);
		const { told, stop } = watch(ID);
		await vi.advanceTimersByTimeAsync(0);
		expect(told.at(-1)).toBe('stopped');
		await vi.advanceTimersByTimeAsync(10_000);
		expect(told.at(-1)).toBe('failed');
		stop();
	});

	it('asks no more after a refusal, which the device holding the file never sends again', async () => {
		answer = () => ({ ...cutOffAt(Date.now()), reason: 'Too large.', refused: true });
		const { told, stop } = watch(ID);
		await vi.advanceTimersByTimeAsync(5 * 60_000);
		expect(told).toEqual(['asking', 'failed']);
		expect(asks.length).toBe(1);
		// A block watching it later hears the same, without asking.
		const again = watch(ID);
		await vi.advanceTimersByTimeAsync(60_000);
		expect(again.told).toEqual(['failed']);
		expect(asks.length).toBe(1);
		again.stop();
		stop();
	});

	it('says missing after the grace, keeps asking, and hears the file once it comes', async () => {
		const { told, stop } = watch(ID);
		await vi.advanceTimersByTimeAsync(MISSING_GRACE_MS + 2000);
		expect(told.at(-1)).toBe('missing');
		// Its device back online, long after.
		await vi.advanceTimersByTimeAsync(3 * 60_000);
		answer = () => ({ state: 'done' });
		await vi.advanceTimersByTimeAsync(ASK_SLOWER_MS);
		expect(told.at(-1)).toBe('done');
		stop();
	});

	it('asks about every watched block in one request, at most a hundred ids each', async () => {
		answer = () => uploading;
		const ids = Array.from({ length: ASK_AT_ONCE + 50 }, (_, i) => `file_${i.toString(16).padStart(16, '0')}`);
		const watches = ids.map(watch);
		await vi.advanceTimersByTimeAsync(0);
		expect(asks.map((a) => a.length)).toEqual([ASK_AT_ONCE, 50]);
		expect(watches.every((w) => w.told.at(-1) === 'uploading')).toBe(true);
		for (const w of watches) w.stop();
		// Nothing watches them: nobody asks.
		const asked = asks.length;
		await vi.advanceTimersByTimeAsync(60_000);
		expect(asks.length).toBe(asked);
	});
});

describe('a file this device holds', () => {
	const photo = () => new File(['bytes'], 'harbour.png', { type: 'image/png' });
	/** Whether closing the page now would ask first. */
	const asksBeforeClosing = () => {
		const event = new Event('beforeunload', { cancelable: true });
		window.dispatchEvent(event);
		return event.defaultPrevented;
	};

	// Out of retries, the file was let go: a reload or a closed tab threw
	// away the only copy, its block already on every device.
	it('stays at stake once its retries run out, and goes again when the device is back online', async () => {
		sendEnds = () => 0;
		startUpload(ID, photo());
		await vi.advanceTimersByTimeAsync(SENDER_TRIES_FOR_MS);
		expect(sends).toHaveLength(5);
		expect(get(watchUpload(ID))).toMatchObject({ phase: 'failed', refused: false });
		expect(uploadsAtStake()).toBe(1);
		expect(asksBeforeClosing()).toBe(true);

		sendEnds = () => 201;
		window.dispatchEvent(new Event('online'));
		await vi.advanceTimersByTimeAsync(0);
		expect(sends).toHaveLength(6);
		expect(get(watchUpload(ID))).toMatchObject({ phase: 'done' });
		expect(uploadsAtStake()).toBe(0);
		expect(asksBeforeClosing()).toBe(false);
	});

	// Out of its retries, a file was sent again only when the device's
	// network changed or on Try again: the server back from an update, the
	// file stayed at stake for good, and every reload, the hidden update and
	// the app's own reloads waited on it while the toast said it would upload.
	it('out of retries, is tried again every few minutes, and at once when the server is back', async () => {
		sendEnds = () => 0;
		startUpload(ID, photo());
		await vi.advanceTimersByTimeAsync(SENDER_TRIES_FOR_MS);
		expect(sends).toHaveLength(5);
		expect(get(watchUpload(ID))).toMatchObject({ phase: 'failed', refused: false });
		await vi.advanceTimersByTimeAsync(TRY_AGAIN_EVERY_MS);
		expect(sends).toHaveLength(6);
		expect(uploadsAtStake()).toBe(1);

		sendEnds = () => 201;
		await vi.advanceTimersByTimeAsync(SENDER_TRIES_FOR_MS + TRY_AGAIN_EVERY_MS);
		expect(sends.length).toBeGreaterThan(6);
		expect(get(watchUpload(ID))).toMatchObject({ phase: 'done' });
		expect(uploadsAtStake()).toBe(0);
		expect(asksBeforeClosing()).toBe(false);

		// A page's socket reaching the server again sends one waiting now.
		resetUploads();
		sends.length = 0;
		sendEnds = () => 0;
		startUpload(ID, photo());
		await vi.advanceTimersByTimeAsync(SENDER_TRIES_FOR_MS);
		sendEnds = () => 201;
		serverBack();
		await vi.advanceTimersByTimeAsync(0);
		expect(sends).toHaveLength(6);
		expect(uploadsAtStake()).toBe(0);
	});

	// A block cut from its page here and pasted into another, which another
	// device then removed it from: nothing here saw it go, and the file out
	// of retries held every reload for good.
	it('out of retries, is let go once no page names it, whatever device took its block out', async () => {
		sendEnds = () => 507;
		startUpload(ID, photo());
		// Cut here: the server says the page it was pasted into holds it.
		uploadBlocksLeft([ID]);
		await vi.advanceTimersByTimeAsync(SENDER_TRIES_FOR_MS);
		expect(uploadsAtStake()).toBe(1);
		await vi.advanceTimersByTimeAsync(TRY_AGAIN_EVERY_MS);
		expect(sends).toHaveLength(6);
		expect(uploadsAtStake()).toBe(1);

		// Removed there, on another device.
		named = () => false;
		await vi.advanceTimersByTimeAsync(SENDER_TRIES_FOR_MS + TRY_AGAIN_EVERY_MS);
		expect(uploadsAtStake()).toBe(0);
		const sent = sends.length;
		await vi.advanceTimersByTimeAsync(2 * TRY_AGAIN_EVERY_MS);
		window.dispatchEvent(new Event('online'));
		await vi.advanceTimersByTimeAsync(0);
		expect(sends).toHaveLength(sent);
		expect(asksBeforeClosing()).toBe(false);

		// Pasted back into a page by another device: it comes back, and goes.
		named = () => true;
		sendEnds = () => 201;
		await vi.advanceTimersByTimeAsync(TRY_AGAIN_EVERY_MS);
		expect(get(watchUpload(ID))).toMatchObject({ phase: 'done' });
	});

	// A block copied into a page that was then closed is that page's: the
	// server says so, and the file goes on.
	it('whose block left every page here goes on while a page names it', async () => {
		sendEnds = () => 0;
		startUpload(ID, photo());
		uploadBlocksLeft([ID]);
		await vi.advanceTimersByTimeAsync(CHECK_AFTER_MS);
		expect(uploadsAtStake()).toBe(1);
		await vi.advanceTimersByTimeAsync(SENDER_TRIES_FOR_MS);
		expect(sends).toHaveLength(5);
	});

	it('is let go once refused, and never sent again', async () => {
		sendEnds = () => 413;
		startUpload(ID, photo());
		await vi.advanceTimersByTimeAsync(SENDER_TRIES_FOR_MS);
		expect(sends).toHaveLength(1);
		expect(get(watchUpload(ID))).toMatchObject({ phase: 'failed', refused: true });
		expect(uploadsAtStake()).toBe(0);
		retryUpload(ID);
		window.dispatchEvent(new Event('online'));
		await vi.advanceTimersByTimeAsync(0);
		expect(sends).toHaveLength(1);
	});
});

// A drop of ten photos opened ten requests at once. The browser sends six
// to a server and holds the rest where the server cannot see them, their
// blocks already in the page: a share of the page went without them, and
// every other device drew them as files that never came.
describe('files given at once', () => {
	const photos = (n: number) =>
		Array.from({ length: n }, (_, i) => [`file_${i.toString(16).padStart(16, '0')}`, new File(['bytes'], `photo-${i}.png`, { type: 'image/png' })] as const);

	it('are each said to be coming before their bytes go, at most a few sent at once, and said again while they wait', async () => {
		const landed: ((status: number) => void)[] = [];
		sendEnds = () => new Promise((settle) => landed.push(settle));
		const given = photos(10);
		for (const [id, file] of given) startUpload(id, file);
		await vi.advanceTimersByTimeAsync(0);
		expect(sends).toEqual(given.slice(0, SEND_AT_ONCE).map(([id]) => id));
		expect(announced).toEqual([given.map(([id]) => id)]);
		expect(uploadsAtStake()).toBe(10);

		// One stored: the next goes.
		landed[0](201);
		await vi.advanceTimersByTimeAsync(0);
		expect(sends).toEqual(given.slice(0, SEND_AT_ONCE + 1).map(([id]) => id));

		// Those still waiting here are said again before the server takes them for stopped.
		await vi.advanceTimersByTimeAsync(ANNOUNCE_EVERY_MS);
		expect(announced.at(-1)).toEqual(given.slice(SEND_AT_ONCE + 1).map(([id]) => id));
		for (let i = 1; i < 10; i++) {
			landed[i](201);
			await vi.advanceTimersByTimeAsync(0);
		}
		expect(sends).toHaveLength(10);
		expect(uploadsAtStake()).toBe(0);
		const said = announced.length;
		await vi.advanceTimersByTimeAsync(5 * ANNOUNCE_EVERY_MS);
		expect(announced).toHaveLength(said);
	});
});

describe('askUpload, beside a file asked about for long', () => {
	// The first ask for a new block waited behind the minute-long ask of a
	// file that never came, and its image drew as a broken card meanwhile.
	it('asks for a new block at once', async () => {
		const old = watch('file_0000000000000001');
		await vi.advanceTimersByTimeAsync(11 * 60_000);
		const asked = asks.length;
		const fresh = watch(ID);
		await vi.advanceTimersByTimeAsync(0);
		expect(asks.length).toBe(asked + 1);
		expect(asks.at(-1)).toContain(ID);
		expect(fresh.told.at(-1)).toBe('waiting');
		// The old block goes back to its own pace.
		await vi.advanceTimersByTimeAsync(ASK_SLOWEST_MS);
		fresh.stop();
		old.stop();
	});

	it('asks for a block that begins asking while a request is out as soon as it answers', async () => {
		let land: () => void = () => {};
		const { getMediaUploads } = await import('$lib/api/client');
		const old = watch('file_0000000000000001');
		await vi.advanceTimersByTimeAsync(11 * 60_000);
		vi.mocked(getMediaUploads).mockImplementationOnce(async (ids: string[]) => {
			asks.push(ids);
			await new Promise<void>((resolve) => (land = resolve));
			return Object.fromEntries(ids.map((id) => [id, answer(id)]));
		});
		await vi.advanceTimersByTimeAsync(ASK_SLOWEST_MS);
		const asked = asks.length;
		const fresh = watch(ID);
		land();
		await vi.advanceTimersByTimeAsync(0);
		expect(asks.length).toBe(asked + 1);
		expect(asks.at(-1)).toContain(ID);
		fresh.stop();
		old.stop();
	});
});
