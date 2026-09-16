/**
 * Tests for the client reporter's failure behavior.
 *
 * Every case here is a defect found by auditing the first version of this
 * file, not a hypothetical. The happy path is verified in a browser; these are
 * the paths a browser will not show you until a user is in them.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

type Call = { events: Array<{ kind: string; message: string }> };

/** Re-import the module fresh so its queue/timer state is not shared. */
async function freshLog() {
	vi.resetModules();
	return await import('./log');
}

function mockFetch(status: number, record: Call[]) {
	return vi.fn(async (_url: string, init?: RequestInit) => {
		record.push(JSON.parse(String(init?.body)) as Call);
		return { ok: status >= 200 && status < 300, status } as Response;
	});
}

describe('client reporter', () => {
	beforeEach(() => {
		vi.useFakeTimers();
	});
	afterEach(() => {
		vi.useRealTimers();
		vi.unstubAllGlobals();
	});

	it('tells the box how many events it had to drop', async () => {
		const calls: Call[] = [];
		vi.stubGlobal('fetch', mockFetch(200, calls));
		const { log } = await freshLog();

		// Overflow the 100-event queue so the oldest are dropped.
		for (let i = 0; i < 160; i++) log.warn('probe', `event ${i}`);
		await log.flush();

		const notice = calls[0].events.find((e) => e.kind === 'client.report.dropped');
		expect(notice, 'a drop notice must ride along').toBeTruthy();
		expect(notice!.message).toContain('60');
	});

	it('never lets the drop notice be the entry the server truncates', async () => {
		// The bug: a full queue produced MAX_BATCH real events PLUS the notice,
		// and the server takes the first MAX_BATCH — so the notice, appended
		// last, was discarded exactly when there was something to report.
		const calls: Call[] = [];
		vi.stubGlobal('fetch', mockFetch(200, calls));
		const { log } = await freshLog();

		for (let i = 0; i < 160; i++) log.warn('probe', `event ${i}`);
		await log.flush();

		expect(calls[0].events.length).toBeLessThanOrEqual(50);
		expect(calls[0].events.some((e) => e.kind === 'client.report.dropped')).toBe(true);
	});

	it('stops posting to a box that has no such route', async () => {
		// Phones update themselves and boxes update when someone types a
		// command, so a client newer than its box is normal, not exceptional.
		const calls: Call[] = [];
		vi.stubGlobal('fetch', mockFetch(404, calls));
		const { log } = await freshLog();

		log.error('probe', 'first');
		await log.flush();
		expect(calls.length).toBe(1);

		log.error('probe', 'second');
		await log.flush();
		log.error('probe', 'third');
		await log.flush();
		expect(calls.length, 'a 404 ends reporting for the session').toBe(1);
	});

	it('keeps the backlog when unpaired, and sends it once pairing succeeds', async () => {
		// The airlock and pairing screens are unpaired BY DEFINITION and are
		// where a failure is most likely, so a 401 must not throw the evidence
		// away the way a 404 does.
		const calls: Call[] = [];
		const fetchMock = vi
			.fn(async (_url: string, init?: RequestInit) => {
				calls.push(JSON.parse(String(init?.body)) as Call);
				return { ok: false, status: 401 } as Response;
			})
			.mockName('unpaired');
		vi.stubGlobal('fetch', fetchMock);
		const { log } = await freshLog();

		log.error('airlock', 'pairing blew up');
		await log.flush();
		expect(calls.length).toBe(1);

		// Now the device pairs.
		vi.stubGlobal('fetch', mockFetch(200, calls));
		log.error('airlock', 'and again');
		await log.flush();

		const sent = calls[calls.length - 1].events.map((e) => e.message);
		expect(sent, 'the pre-pairing error survived').toContain('pairing blew up');
		expect(sent).toContain('and again');
	});

	it('stops the timer when there is nothing to say', async () => {
		const calls: Call[] = [];
		vi.stubGlobal('fetch', mockFetch(200, calls));
		const { log } = await freshLog();

		log.warn('probe', 'one');
		await log.flush();
		expect(calls.length).toBe(1);

		// A timer left running is a wakeup every ten seconds on someone's
		// phone in exchange for nothing.
		await vi.advanceTimersByTimeAsync(60_000);
		expect(calls.length, 'an idle reporter is silent').toBe(1);

		// ...and it comes back when there is something new.
		log.warn('probe', 'two');
		await vi.advanceTimersByTimeAsync(11_000);
		expect(calls.length).toBe(2);
	});

	it('does not throw when the box is unreachable', async () => {
		vi.stubGlobal(
			'fetch',
			vi.fn(async () => {
				throw new Error('offline');
			})
		);
		const { log } = await freshLog();
		log.error('probe', 'boom');
		await expect(log.flush()).resolves.toBeUndefined();
	});

	it('turns an Error into something that survives JSON', async () => {
		// `JSON.stringify(new Error('boom'))` is `{}` — the most common way a
		// client error reaches a server as an empty object.
		const calls: Call[] = [];
		vi.stubGlobal('fetch', mockFetch(200, calls));
		const { log } = await freshLog();

		log.error('probe', 'failed', new Error('the real reason'));
		await log.flush();

		const detail = (calls[0].events[0] as unknown as { detail: { message: string } }).detail;
		expect(detail.message).toBe('the real reason');
	});
});

describe('request-id correlation', () => {
	beforeEach(() => vi.useFakeTimers());
	afterEach(() => {
		vi.useRealTimers();
		vi.unstubAllGlobals();
	});

	it('carries the exact id from an ApiError, and the last-seen one otherwise', async () => {
		const calls: Array<{ events: Array<Record<string, unknown>> }> = [];
		vi.stubGlobal(
			'fetch',
			vi.fn(async (_u: string, init?: RequestInit) => {
				calls.push(JSON.parse(String(init?.body)));
				return { ok: true, status: 200 } as Response;
			})
		);
		const { log, noteRequestId } = await import('./log');

		// The box answered some earlier request with this id.
		noteRequestId('r42_seen');

		// An uncaught error knows no request of its own → the hint rides along.
		log.error('uncaught', 'boom');

		// An ApiError knows exactly which request failed → that wins.
		class ApiErrorLike extends Error {
			readonly requestId = 'r7_exact';
			readonly status = 500;
		}
		log.error('api', 'call failed', new ApiErrorLike('server said no'));

		await log.flush();

		const [uncaught, api] = calls[0].events;
		expect(uncaught.last_request_id).toBe('r42_seen');
		const detail = api.detail as { request_id: string; status: number };
		expect(detail.request_id).toBe('r7_exact');
		expect(detail.status).toBe(500);
		expect(api.last_request_id, 'an exact id needs no hint beside it').toBeUndefined();
	});
});
