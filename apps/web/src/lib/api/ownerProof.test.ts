/**
 * `apiSendProven` inside the app: what reaches the box for each kind of app
 * and server, and that an app too old to sign says so instead of sending a
 * request the box can only refuse.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('$lib/utils/platform', () => ({ isTauri: true }));
vi.mock('$lib/tauri/bridge', () => ({ shellSupports: vi.fn(async () => true) }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

const { apiSendProven, ApiError } = await import('./client');
const { shellSupports } = await import('$lib/tauri/bridge');
const { invoke } = await import('@tauri-apps/api/core');

const ROUTE = '/passwords/pw_example/secret';

/** A box that answers the nonce route with `nonce` (a status for an error) and every other route with 200. */
function box(nonce: string | number) {
	const fetch = vi.fn(async (url: string, _init?: RequestInit) => {
		if (url === '/api/owner-proof/nonce') {
			return typeof nonce === 'number'
				? new Response('', { status: nonce, statusText: 'Not Found' })
				: Response.json({ nonce });
		}
		return Response.json({ secret: 'correct horse' });
	});
	vi.stubGlobal('fetch', fetch);
	return fetch;
}

/** The headers the gated request went out with, or undefined when it never went out. */
function gatedHeaders(fetch: ReturnType<typeof box>): Record<string, string> | undefined {
	const call = fetch.mock.calls.find(([url]) => url === `/api${ROUTE}`);
	return call ? ((call[1]?.headers ?? {}) as Record<string, string>) : undefined;
}

beforeEach(() => {
	vi.mocked(invoke).mockReset();
	vi.mocked(shellSupports).mockReset().mockResolvedValue(true);
});
afterEach(() => vi.unstubAllGlobals());

describe('apiSendProven in the app', () => {
	it('signs the method, path and nonce, and sends the proof', async () => {
		const fetch = box('nonce-1');
		vi.mocked(invoke).mockResolvedValue('nonce-1.sig');

		await expect(apiSendProven('GET', ROUTE)).resolves.toEqual({ secret: 'correct horse' });
		expect(vi.mocked(invoke)).toHaveBeenCalledWith('plugin:reach|owner_proof_sign', {
			method: 'GET',
			path: `/api${ROUTE}`,
			nonce: 'nonce-1',
		});
		expect(gatedHeaders(fetch)?.['X-Virtues-Owner-Proof']).toBe('nonce-1.sig');
	});

	it('sends no proof to a server too old to issue a nonce', async () => {
		const fetch = box(404);

		await expect(apiSendProven('GET', ROUTE)).resolves.toEqual({ secret: 'correct horse' });
		expect(vi.mocked(invoke)).not.toHaveBeenCalled();
		expect(gatedHeaders(fetch)).not.toHaveProperty('X-Virtues-Owner-Proof');
	});

	it('tells the owner to update an app too old to sign, and sends nothing', async () => {
		const fetch = box('nonce-1');
		vi.mocked(invoke).mockRejectedValue('reach.owner_proof_sign not allowed. Command not found');
		vi.mocked(shellSupports).mockResolvedValue(false);

		const error = await apiSendProven('GET', ROUTE).catch((e: unknown) => e);
		expect(error).toBeInstanceOf(ApiError);
		expect((error as InstanceType<typeof ApiError>).message).toMatch(/^Update the Virtues app to do this\./);
		expect(vi.mocked(shellSupports)).toHaveBeenCalledWith(15);
		expect(gatedHeaders(fetch)).toBeUndefined();
	});

	it("passes a current app's signing failure through in its own words", async () => {
		const fetch = box('nonce-1');
		const said = "Virtues could not read this Mac's device key from the Keychain. Quit and reopen Virtues to try again.";
		vi.mocked(invoke).mockRejectedValue(said);

		const error = await apiSendProven('GET', ROUTE).catch((e: unknown) => e);
		expect((error as Error).message).toBe(said);
		expect(error).not.toBeInstanceOf(ApiError);
		expect(gatedHeaders(fetch)).toBeUndefined();
	});

	it('never asks an app that signs for its surface', async () => {
		// A shell whose capabilities refuse `command_surface_version` reports
		// surface 0 while it signs fine.
		box('nonce-1');
		vi.mocked(invoke).mockResolvedValue('nonce-1.sig');
		vi.mocked(shellSupports).mockResolvedValue(false);

		await expect(apiSendProven('GET', ROUTE)).resolves.toEqual({ secret: 'correct horse' });
		expect(vi.mocked(shellSupports)).not.toHaveBeenCalled();
	});
});
