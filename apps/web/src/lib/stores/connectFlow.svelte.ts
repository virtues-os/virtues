/**
 * The connect flow's window state, hoisted out of any one page.
 *
 * The Sources page and a source's page both start the same flow, and the
 * window it opens has to outlive a page swap, so SourcesView renders it once
 * and the pages only say *start this* (agents/plan/sources-redesign-plan.md,
 * slice 6).
 *
 * An account (OAuth through the proxy, and Plaid) opens in steps: what
 * connecting does, then the provider's page, then done. Nothing leaves the
 * app until the person presses Continue, so the window can say first what
 * your server will read and what your assistant may do there.
 */
import { connectIntent, OWN_APP_SOURCES } from '$lib/components/sources/connectDispatch';
import type { SourceCatalogItem } from '$lib/api/client';

/** intro: what connecting does · opening: asking the server for the
 *  provider's page · waiting: that page is open in the browser · done ·
 *  error: the page could not be opened. */
export type AccountStep = 'intro' | 'opening' | 'waiting' | 'done' | 'error';

type Pending =
	| { kind: 'none' }
	| { kind: 'pair'; deviceType: 'ios' | 'mac'; displayName: string }
	| { kind: 'import'; sourceId: string }
	/** Signing in with an app the person made at the vendor (Spotify's recent
	 *  plays), new or again for `credentialId`. */
	| { kind: 'own_app'; sourceId: string; credentialId?: string }
	| { kind: 'api_key'; source: SourceCatalogItem; credentialId?: string }
	| {
			kind: 'account';
			source: SourceCatalogItem;
			credentialId?: string;
			step: AccountStep;
			message?: string;
	  };

class ConnectFlowStore {
	pending = $state<Pending>({ kind: 'none' });
	error = $state<string | null>(null);

	/** `credentialId`: reconnecting that credential rather than adding one.
	 *  Both the api_key form and the OAuth dance repair that row in place. */
	async start(source: SourceCatalogItem, credentialId?: string): Promise<void> {
		this.error = null;
		// A credential of a source that also imports is the own-app sign-in
		// (Spotify's recent plays); signing it in again opens that window, not
		// the import.
		if (credentialId && OWN_APP_SOURCES.has(source.id)) {
			this.startOwnApp(source.id, credentialId);
			return;
		}
		if (source.auth_kind === 'via_proxy') {
			this.pending = { kind: 'account', source, credentialId, step: 'intro' };
			return;
		}
		const intent = await connectIntent(source, { credentialId });
		switch (intent.kind) {
			case 'pair':
				this.pending = {
					kind: 'pair',
					deviceType: intent.deviceType,
					displayName: intent.displayName
				};
				return;
			case 'import':
				this.pending = { kind: 'import', sourceId: intent.sourceId };
				return;
			case 'api_key':
				this.pending = { kind: 'api_key', source: intent.source, credentialId };
				return;
			case 'oauth':
				// Only via_proxy sources reach OAuth, and those went to the window above.
				return;
			case 'error':
				this.error = intent.message;
				return;
		}
	}

	/**
	 * The account window's Continue: opens the provider's page. In a browser
	 * tab the page navigates away and this store is torn down. In the app the
	 * page opens in the system browser and the window waits.
	 */
	async continueAccount(): Promise<void> {
		const p = this.pending;
		if (p.kind !== 'account') return;
		this.pending = { ...p, step: 'opening', message: undefined };
		const intent = await connectIntent(p.source, { credentialId: p.credentialId });
		if (this.pending.kind !== 'account') return; // closed meanwhile
		if (intent.kind === 'oauth') {
			if (intent.external) this.pending = { ...this.pending, step: 'waiting' };
			return;
		}
		this.pending = {
			...this.pending,
			step: 'error',
			message: intent.kind === 'error' ? intent.message : undefined
		};
	}

	/** Open the own-app sign-in for `sourceId`. */
	startOwnApp(sourceId: string, credentialId?: string): void {
		this.error = null;
		this.pending = { kind: 'own_app', sourceId, credentialId };
	}

	/** The account the window is waiting on has arrived. */
	accountConnected(): void {
		if (this.pending.kind === 'account') this.pending = { ...this.pending, step: 'done' };
	}

	close(): void {
		this.pending = { kind: 'none' };
	}
}

export const connectFlow = new ConnectFlowStore();
