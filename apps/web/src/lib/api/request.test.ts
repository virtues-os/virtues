// request(): identical GETs in flight share one fetch, each caller gets its own
// object, writes never share, and a GET without a signal gets a deadline.
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ApiError, request } from './client';

afterEach(() => vi.unstubAllGlobals());

/** A fetch that answers `body` after a tick, recording every call. */
function stubFetch(status = 200, body: unknown = { n: 1 }) {
	const calls: { url: string; init?: RequestInit }[] = [];
	vi.stubGlobal(
		'fetch',
		vi.fn(async (url: string, init?: RequestInit) => {
			calls.push({ url, init });
			await new Promise((r) => setTimeout(r, 5));
			return new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });
		})
	);
	return calls;
}

describe('request', () => {
	it('shares one fetch between identical GETs in flight', async () => {
		const calls = stubFetch();
		const [a, b] = await Promise.all([request<{ n: number }>('/profile'), request<{ n: number }>('/profile')]);
		expect(calls).toHaveLength(1);
		expect(a).toEqual({ n: 1 });
		expect(b).toEqual({ n: 1 });
		a.n = 2;
		expect(b.n).toBe(1);
	});

	it('fetches again once the first GET has settled', async () => {
		const calls = stubFetch();
		await request('/profile');
		await request('/profile');
		expect(calls).toHaveLength(2);
	});

	it('never shares a write or a GET with its own signal or headers', async () => {
		const calls = stubFetch();
		await Promise.all([
			request('/profile', { method: 'PUT', body: '{}' }),
			request('/profile', { method: 'PUT', body: '{}' }),
			request('/profile', { signal: new AbortController().signal }),
			request('/profile', { headers: { 'X-Test': '1' } }),
			request('/profile'),
		]);
		expect(calls).toHaveLength(5);
	});

	it('gives a GET without a signal a deadline, and a write none', async () => {
		const calls = stubFetch();
		await request('/profile');
		await request('/profile', { method: 'POST', body: '{}' });
		expect(calls[0].init?.signal).toBeInstanceOf(AbortSignal);
		expect(calls[1].init?.signal).toBeUndefined();
	});

	it('throws the same ApiError to every caller sharing a failed GET', async () => {
		stubFetch(500, { error: 'box is unhappy' });
		const results = await Promise.allSettled([request('/profile'), request('/profile')]);
		for (const r of results) {
			expect(r.status).toBe('rejected');
			const e = (r as PromiseRejectedResult).reason;
			expect(e).toBeInstanceOf(ApiError);
			expect(e.status).toBe(500);
			expect(e.message).toBe('box is unhappy');
		}
	});
});
