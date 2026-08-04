/**
 * The connect flow's modal state, hoisted out of any one section.
 *
 * Both Overview (Reconnect on a broken connection) and Catalog (Connect on a
 * tile) start the same flow, and the modals it opens have to be rendered
 * somewhere that outlives a section swap — so the room shell renders them once
 * and the sections only say *start this*. Without it each section would carry
 * its own copy of four modals, which is how the two connect dispatchers drifted
 * in the first place.
 */
import { connectIntent } from '$lib/components/sources/connectDispatch';
import type { SourceCatalogItem } from '$lib/api/client';

type Pending =
	| { kind: 'none' }
	| { kind: 'pair'; deviceType: 'ios' | 'mac'; displayName: string }
	| { kind: 'chat_import' }
	| { kind: 'api_key'; source: SourceCatalogItem };

class ConnectFlowStore {
	pending = $state<Pending>({ kind: 'none' });
	error = $state<string | null>(null);
	/** Set when an OAuth dance was handed to the system browser (Tauri). */
	awaitingExternal = $state(false);

	async start(source: SourceCatalogItem): Promise<void> {
		this.error = null;
		const intent = await connectIntent(source);
		switch (intent.kind) {
			case 'pair':
				this.pending = {
					kind: 'pair',
					deviceType: intent.deviceType,
					displayName: intent.displayName
				};
				return;
			case 'chat_import':
				this.pending = { kind: 'chat_import' };
				return;
			case 'api_key':
				this.pending = { kind: 'api_key', source: intent.source };
				return;
			case 'oauth':
				// Browser: we're navigating away and this store is about to be torn
				// down. Tauri: the SPA stayed mounted, so the shell refreshes on the
				// next window focus.
				this.awaitingExternal = intent.external;
				return;
			case 'error':
				this.error = intent.message;
				return;
		}
	}

	close(): void {
		this.pending = { kind: 'none' };
	}
}

export const connectFlow = new ConnectFlowStore();
