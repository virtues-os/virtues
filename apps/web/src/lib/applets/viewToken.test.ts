import { afterEach, describe, expect, it, vi } from 'vitest';

const request = vi.fn();
vi.mock('$lib/api/client', () => {
	class ApiError extends Error {
		constructor(readonly status: number) {
			super(`HTTP ${status}`);
		}
	}
	return { ApiError, request: (...args: unknown[]) => request(...args) };
});

import { ApiError } from '$lib/api/client';
import { tokenForgotten } from './viewToken';

const refuse = (status: number) => new (ApiError as unknown as new (status: number) => Error)(status);

afterEach(() => request.mockReset());

describe('tokenForgotten', () => {
	it('is true only when the server refuses the token itself', async () => {
		request.mockRejectedValueOnce(refuse(401));
		expect(await tokenForgotten('t1')).toBe(true);
	});

	it('is false for a token the server knows, which refuses the empty query', async () => {
		request.mockRejectedValueOnce(refuse(400));
		expect(await tokenForgotten('t1')).toBe(false);
	});

	it('is false while the server is away, so a restart in progress does not reload the view', async () => {
		request.mockRejectedValueOnce(refuse(502));
		expect(await tokenForgotten('t1')).toBe(false);
		request.mockRejectedValueOnce(new TypeError('Failed to fetch'));
		expect(await tokenForgotten('t1')).toBe(false);
	});

	it('asks with an empty query on the view route', async () => {
		request.mockRejectedValueOnce(refuse(400));
		await tokenForgotten('t1');
		expect(request).toHaveBeenCalledWith('/face/query', expect.objectContaining({ method: 'POST', query: { vt: 't1' }, body: '{"sql":""}' }));
	});
});
