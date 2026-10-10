/**
 * Yjs Document Manager for CodeMirror
 *
 * Creates and manages Yjs documents for real-time collaborative editing.
 * Uses Y.Text for markdown-native editing with CodeMirror 6.
 * Handles WebSocket sync, IndexedDB persistence, and undo management.
 */

import * as Y from 'yjs';
import { WebsocketProvider } from 'y-websocket';
import { IndexeddbPersistence } from 'y-indexeddb';
import { writable, type Writable } from 'svelte/store';
import { getWsUrl } from '$lib/config/backend';

/**
 * The document contract this binding reads (`crates/virtues-document`). It
 * binds the page's Y.Text, which is contract 0: a page written as a Yjs tree
 * carries a higher version, and the server refuses this binding on it, since
 * an editor deletes from the shared document what it cannot read.
 */
const CONTRACT = 0;

/** Close codes the server ends a refused socket with (`server/yjs.rs`). */
const CLOSE_CONTRACT = 4426;
const CLOSE_REFUSED = 4422;

const NEEDS_NEWER_APP = 'A newer version of Virtues wrote this page. Update the app to open it.';

export interface YjsDocument {
	ydoc: Y.Doc;
	ytext: Y.Text;
	provider: WebsocketProvider;
	persistence: IndexeddbPersistence;
	undoManager: Y.UndoManager;

	// Connection state stores
	isLoading: Writable<boolean>;
	isSynced: Writable<boolean>;
	isConnected: Writable<boolean>;
	/**
	 * Why the server will not sync this page with this editor, once it has
	 * said so: the page needs a newer app, or an edit was refused. Null until
	 * then. A refused editor does not reconnect.
	 */
	refused: Writable<string | null>;
	/**
	 * Whether the server said this binding reads the page, so the copy kept
	 * on this device may be shown before the server syncs.
	 */
	contractConfirmed: boolean;

	// Cleanup
	destroy: () => void;
}

/**
 * Create a Yjs document for a page
 *
 * @param pageId - The page ID to sync
 * @param pageContract - The contract the server says the page is written
 *   under (`contract` in `GET /api/pages/:id`), or null when it has not said.
 *   The copy of the page this device kept is shown before the server syncs
 *   only when the server said this binding reads the page: a copy kept from
 *   before the page was raised to a tree carries no stamp, would show text
 *   the page no longer holds, and take edits the server refuses.
 */
export function createYjsDocument(pageId: string, pageContract: number | null = null): YjsDocument {
	// GC enabled (default) - versions use encodeStateAsUpdate which is self-contained
	const ydoc = new Y.Doc();

	// Use Y.Text for markdown-native editing
	const ytext = ydoc.getText('content');

	// Connection state stores
	const isLoading = writable(true);
	const isSynced = writable(false);
	const isConnected = writable(false);
	const refused = writable<string | null>(null);

	const contractConfirmed = pageContract !== null && pageContract <= CONTRACT;
	// A page written under a newer contract: the server would refuse this
	// binding, so it is not asked.
	const needsNewerApp = pageContract !== null && pageContract > CONTRACT;
	if (needsNewerApp) {
		refused.set(NEEDS_NEWER_APP);
		isLoading.set(false);
	}

	// Base WS URL (y-websocket appends room/pageId). Same-origin on desktop;
	// routed to the iroh loopback on mobile via the backend config.
	const wsUrl = getWsUrl('/ws/yjs');

	// WebSocket provider for real-time sync
	const provider = new WebsocketProvider(wsUrl, pageId, ydoc, {
		connect: !needsNewerApp,
		// Reconnect automatically
		maxBackoffTime: 10000,
		params: { contract: String(CONTRACT) },
	});

	// IndexedDB persistence for offline support
	const persistence = new IndexeddbPersistence(`v2-${pageId}`, ydoc);

	// Track sync state — prefer remote (WebSocket) sync as authoritative.
	// Local (IndexedDB) sync is sufficient ONLY if it has cached content.
	// For brand-new pages, IndexedDB fires 'synced' instantly with an empty doc,
	// which would prematurely show an empty editor before the server delivers content.
	let localSynced = false;
	let remoteSynced = false;

	// The local copy stands in for the page only when the server said this
	// binding reads the page, and the copy carries no stamp above it.
	const localCopyReadable = () =>
		contractConfirmed && Number(ydoc.getMap('meta').get('contract') ?? 0) <= CONTRACT;

	function checkSyncComplete() {
		if (remoteSynced) {
			// Remote sync is authoritative — always trust it
			isSynced.set(true);
			isLoading.set(false);
		} else if (localSynced && ytext.length > 0 && localCopyReadable()) {
			// IndexedDB had cached content — use it for fast offline-first loading
			isSynced.set(true);
			isLoading.set(false);
		}
		// If localSynced but empty, keep waiting for remote sync
	}

	persistence.on('synced', () => {
		localSynced = true;
		checkSyncComplete();
	});

	// Use 'status' event for reliable connection state tracking
	provider.on('status', (event: { status: string }) => {
		isConnected.set(event.status === 'connected');
	});

	provider.on('sync', () => {
		// Remote sync completed - content is now in sync with server
		remoteSynced = true;
		checkSyncComplete();
	});

	provider.on('connection-error', () => {
		// Allow offline editing when connection fails —
		// accept local sync even if empty (best we can do offline)
		if (localSynced && localCopyReadable()) {
			isSynced.set(true);
			isLoading.set(false);
		}
	});

	// The server closes a socket it refuses with a code y-websocket treats as
	// final (4400-4499), and says why.
	provider.on('closed', (event: { code: number; reason: string }) => {
		if (event.code !== CLOSE_CONTRACT && event.code !== CLOSE_REFUSED) return;
		console.warn(`[yjs] the server closed page ${pageId} (${event.code}): ${event.reason}`);
		if (event.code === CLOSE_CONTRACT) {
			refused.set(NEEDS_NEWER_APP);
			// A local copy may already be showing; it is not this page any more.
			isSynced.set(false);
		} else {
			// The editor stays, so what was typed can still be copied out.
			refused.set(
				"Your server couldn't accept an edit to this page, so changes here won't save. Copy what you need, then reload the page.",
			);
		}
		isLoading.set(false);
	});

	// UndoManager for Y.Text
	const undoManager = new Y.UndoManager(ytext, {
		trackedOrigins: new Set([null, 'user', 'ai']),
		captureTimeout: 500,
	});

	// Create the document object
	const doc: YjsDocument = {
		ydoc,
		ytext,
		provider,
		persistence,
		undoManager,
		isLoading,
		isSynced,
		isConnected,
		refused,
		contractConfirmed,
		destroy: () => {
			undoManager.destroy();
			provider.destroy();
			persistence.destroy();
			ydoc.destroy();
		},
	};

	return doc;
}
