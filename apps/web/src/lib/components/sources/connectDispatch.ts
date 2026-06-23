/**
 * connectDispatch — the single place that maps a catalog source to the right
 * connect flow, so the onboarding "Connect your world" view and the /sources
 * ConnectionsPanel can't drift. Returns a descriptor the caller turns into
 * modal state; the OAuth case performs the redirect itself and returns
 * `{ kind: 'oauth' }`.
 */
import { oauthStart, type SourceCatalogItem } from '$lib/api/client';

export type ConnectIntent =
	| { kind: 'pair'; deviceType: 'ios' | 'mac'; displayName: string }
	| { kind: 'chat_import' }
	| { kind: 'api_key'; source: SourceCatalogItem }
	| { kind: 'oauth' }
	| { kind: 'error'; message: string };

export async function connectIntent(source: SourceCatalogItem): Promise<ConnectIntent> {
	if (source.auth_kind === 'self_issued_bearer') {
		return {
			kind: 'pair',
			deviceType: source.id === 'mac' ? 'mac' : 'ios',
			displayName: source.name
		};
	}

	// One-time import sources open the upload card, not the api-key form.
	// (chat_import is declared as api_key in the catalog, so test id first.)
	if (source.id === 'chat_import') {
		return { kind: 'chat_import' };
	}

	if (source.auth_kind === 'via_proxy') {
		try {
			const { redirect_url } = await oauthStart(source.id, {
				return_url: `${window.location.origin}/oauth/callback`
			});
			window.location.assign(redirect_url);
			return { kind: 'oauth' };
		} catch (e) {
			return { kind: 'error', message: e instanceof Error ? e.message : String(e) };
		}
	}

	if (source.auth_kind === 'api_key') {
		return { kind: 'api_key', source };
	}

	return { kind: 'error', message: `Unknown auth_kind for "${source.name}": ${source.auth_kind}` };
}
