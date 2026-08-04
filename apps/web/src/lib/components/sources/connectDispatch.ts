/**
 * connectDispatch — the single place that maps a catalog source to the right
 * connect flow, so the onboarding "Connect your world" view and the Sources
 * room can't drift. Returns a descriptor the caller turns into modal state;
 * the OAuth case performs the redirect itself and returns `{ kind: 'oauth' }`.
 */
import { oauthStart, type SourceCatalogItem } from '$lib/api/client';
import { getBackendOrigin } from '$lib/config/backend';
import { openExternal } from '$lib/tauri/bridge';
import { isTauri } from '$lib/utils/platform';

export type ConnectIntent =
	| { kind: 'pair'; deviceType: 'ios' | 'mac'; displayName: string }
	| { kind: 'chat_import' }
	| { kind: 'api_key'; source: SourceCatalogItem }
	// `external` = the OAuth dance was handed off to the *system browser*
	// (Tauri); the SPA stays mounted and should refresh on return. `false` =
	// full-page redirect (browser tab), so the caller is navigating away.
	| { kind: 'oauth'; external: boolean }
	| { kind: 'error'; message: string };

/**
 * Start a via_proxy OAuth connect. Returns whether the flow was handed to the
 * system browser (Tauri) rather than a full-page redirect (browser tab).
 *
 * Two shells, two behaviours:
 *  - **Browser / box-served desktop:** same-origin. Navigate the tab to the
 *    proxy; the callback returns to `<origin>/oauth/callback` on the box.
 *  - **Tauri (mobile/Mac):** the SPA lives at a `tauri://` origin and the box
 *    is reachable only at its loopback HTTP port. So (a) the callback must
 *    return to the box's *real* origin — `getBackendOrigin()`, e.g.
 *    `http://127.0.0.1:7117` — not `tauri://localhost`, and (b) we must NOT
 *    call `window.location.assign` (that unmounts the whole app — the bug this
 *    fixes). Open the system browser instead and leave the SPA mounted.
 */
export async function startOAuth(sourceId: string): Promise<{ external: boolean }> {
	const origin = getBackendOrigin() || window.location.origin;
	// Tauri hands OAuth to the system browser, so the callback's usual 302 into
	// `/sources` would strand the user on a second copy of the app in a browser
	// tab. `shell=native` tells the box to render a terminal "return to Virtues"
	// page instead; the query param round-trips through the proxy untouched.
	const returnUrl = isTauri
		? `${origin}/oauth/callback?shell=native`
		: `${origin}/oauth/callback`;
	const { redirect_url } = await oauthStart(sourceId, { return_url: returnUrl });
	if (isTauri) {
		await openExternal(redirect_url);
		return { external: true };
	}
	window.location.assign(redirect_url);
	return { external: false };
}

/**
 * After an external (system-browser) OAuth handoff, the box finalizes the
 * credential server-side while the user is still in their browser. Re-fetch
 * when they switch back to the app — one-shot on the next window focus.
 */
export function reloadOnReturn(reload: () => void): void {
	if (typeof window === 'undefined') return;
	const handler = () => {
		window.removeEventListener('focus', handler);
		void reload();
	};
	window.addEventListener('focus', handler);
}

export async function connectIntent(source: SourceCatalogItem): Promise<ConnectIntent> {
	// One narrative for every device app: get the app, then enter the code. The
	// Mac briefly had its own flow because pairing the Mac app pairs a *viewer*
	// and the collector is a separate daemon — but that split is ours to solve,
	// not something a user should have to hold in their head. The modal states
	// the one extra Mac step plainly instead; making it disappear entirely means
	// the Mac app installing its collector on pair, which is the real fix.
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
			const { external } = await startOAuth(source.id);
			return { kind: 'oauth', external };
		} catch (e) {
			return { kind: 'error', message: e instanceof Error ? e.message : String(e) };
		}
	}

	if (source.auth_kind === 'api_key') {
		return { kind: 'api_key', source };
	}

	return { kind: 'error', message: `Unknown auth_kind for "${source.name}": ${source.auth_kind}` };
}
