/**
 * Reconnect repairs the row it was pressed on. These check what the connect
 * flow sends the box for each kind of source, and that an OAuth reconnect
 * never uses `existing_credential_id`, which a released box cannot finish.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { SourceCatalogItem } from '$lib/api/client';

vi.mock('$lib/api/client', () => ({
	oauthStart: vi.fn(async () => ({ redirect_url: 'https://proxy.example.com/google/start' })),
}));
// The shell's ways of opening the provider's page. These tests check only
// what reaches the box.
vi.mock('$lib/tauri/bridge', () => ({
	openExternal: vi.fn(async () => {}),
	openSignIn: vi.fn(async () => {}),
	canSignInSheet: vi.fn(async () => false),
	SIGN_IN_CLOSED: 'sign-in-closed',
}));
// The app shell: OAuth goes to the system browser and the page stays put.
vi.mock('$lib/utils/platform', () => ({ isTauri: true }));
vi.mock('$lib/config/backend', () => ({ getBackendOrigin: () => 'http://127.0.0.1:7117' }));

const { connectFlow } = await import('./connectFlow.svelte');
const { oauthStart } = await import('$lib/api/client');

function source(id: string, auth_kind: SourceCatalogItem['auth_kind']): SourceCatalogItem {
	return {
		id,
		name: id,
		icon: null,
		description: null,
		auth_kind,
		credential_count: 1,
		fields: auth_kind === 'api_key' ? ['token'] : [],
	};
}

/** The JSON body the box receives, as `apiSend` would serialize it. */
function sentBody(): Record<string, unknown> {
	const [, opts] = vi.mocked(oauthStart).mock.calls.at(-1)!;
	return JSON.parse(JSON.stringify(opts));
}

beforeEach(() => {
	vi.mocked(oauthStart).mockClear();
	connectFlow.close();
	connectFlow.awaitingExternal = false;
});

describe('starting a connect', () => {
	it('sends the credential being reconnected as credential_id', async () => {
		await connectFlow.start(source('google', 'via_proxy'), 'cred_example');

		expect(vi.mocked(oauthStart).mock.calls.at(-1)![0]).toBe('google');
		expect(sentBody()).toEqual({
			return_url: 'http://127.0.0.1:7117/oauth/callback?shell=native',
			credential_id: 'cred_example',
		});
		expect(connectFlow.awaitingExternal).toBe(true);
	});

	it('sends no credential id for a fresh connect', async () => {
		await connectFlow.start(source('google', 'via_proxy'));

		expect(sentBody()).toEqual({
			return_url: 'http://127.0.0.1:7117/oauth/callback?shell=native',
		});
	});

	it('hands the credential being reconnected to the api_key form', async () => {
		const github = source('github', 'api_key');
		await connectFlow.start(github, 'cred_example');

		expect(oauthStart).not.toHaveBeenCalled();
		expect(connectFlow.pending).toEqual({
			kind: 'api_key',
			source: github,
			credentialId: 'cred_example',
		});
	});
});
