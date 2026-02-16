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

	// Cleanup
	destroy: () => void;
}

/**
 * Create a Yjs document for a page
 *
 * @param pageId - The page ID to sync
 * @param initialContent - Optional initial content (only used if server doc is empty)
 */
export function createYjsDocument(pageId: string, _initialContent?: string): YjsDocument {
	// GC enabled (default) - versions use encodeStateAsUpdate which is self-contained
	const ydoc = new Y.Doc();

	// Use Y.Text for markdown-native editing
	const ytext = ydoc.getText('content');

	// Connection state stores
	const isLoading = writable(true);
	const isSynced = writable(false);
	const isConnected = writable(false);

	// Build WebSocket URL (base URL only - y-websocket appends the roomname/pageId)
	const wsProtocol = typeof window !== 'undefined' && location.protocol === 'https:' ? 'wss:' : 'ws:';
	const wsHost = typeof window !== 'undefined' ? location.host : 'localhost:8000';
	const wsUrl = `${wsProtocol}//${wsHost}/ws/yjs`;

	// WebSocket provider for real-time sync
	const provider = new WebsocketProvider(wsUrl, pageId, ydoc, {
		connect: true,
		// Reconnect automatically
		maxBackoffTime: 10000,
	});

	// IndexedDB persistence for offline support
	const persistence = new IndexeddbPersistence(`v2-${pageId}`, ydoc);

	// Track sync state — prefer remote (WebSocket) sync as authoritative.
	// Local (IndexedDB) sync is sufficient ONLY if it has cached content.
	// For brand-new pages, IndexedDB fires 'synced' instantly with an empty doc,
	// which would prematurely show an empty editor before the server delivers content.
	let localSynced = false;
	let remoteSynced = false;

	function checkSyncComplete() {
		if (remoteSynced) {
			// Remote sync is authoritative — always trust it
			isSynced.set(true);
			isLoading.set(false);
		} else if (localSynced && ytext.length > 0) {
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
		if (localSynced) {
			isSynced.set(true);
			isLoading.set(false);
		}
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
		destroy: () => {
			undoManager.destroy();
			provider.destroy();
			persistence.destroy();
			ydoc.destroy();
		},
	};

	return doc;
}
