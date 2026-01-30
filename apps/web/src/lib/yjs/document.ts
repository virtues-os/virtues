/**
 * Yjs Document Manager
 *
 * Creates and manages Yjs documents for real-time collaborative editing.
 * Handles WebSocket sync, IndexedDB persistence, and undo management.
 */

import * as Y from 'yjs';
import { WebsocketProvider } from 'y-websocket';
import { IndexeddbPersistence } from 'y-indexeddb';
import { yCollab, yUndoManagerKeymap } from 'y-codemirror.next';
import { keymap } from '@codemirror/view';
import type { Extension } from '@codemirror/state';
import { writable, type Writable } from 'svelte/store';

export interface YjsDocument {
	ydoc: Y.Doc;
	ytext: Y.Text;
	provider: WebsocketProvider;
	persistence: IndexeddbPersistence;
	undoManager: Y.UndoManager;
	extensions: Extension[];

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
export function createYjsDocument(pageId: string, initialContent?: string): YjsDocument {
	const ydoc = new Y.Doc();
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
		maxBackoffTime: 10000
	});

	// IndexedDB persistence for offline support
	const persistence = new IndexeddbPersistence(pageId, ydoc);

	// Track sync state
	let localSynced = false;

	function checkSyncComplete() {
		// Allow editing once IndexedDB is synced, regardless of remote state
		// This enables offline-first editing - remote sync is nice-to-have
		if (localSynced) {
			isSynced.set(true);
			isLoading.set(false);
		}
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
		checkSyncComplete();
	});

	provider.on('connection-error', () => {
		// Allow offline editing when connection fails
		checkSyncComplete();
	});

	// Single UndoManager tracking all origins (user + ai)
	// Simplified from dual UndoManager approach
	const undoManager = new Y.UndoManager(ytext, {
		trackedOrigins: new Set([null, 'user', 'ai']),
		captureTimeout: 500
	});

	// CodeMirror extensions for Yjs collaboration
	const extensions: Extension[] = [yCollab(ytext, provider.awareness), keymap.of(yUndoManagerKeymap)];

	// Create the document object
	const doc: YjsDocument = {
		ydoc,
		ytext,
		provider,
		persistence,
		undoManager,
		extensions,
		isLoading,
		isSynced,
		isConnected,
		destroy: () => {
			undoManager.destroy();
			provider.destroy();
			persistence.destroy();
			ydoc.destroy();
		}
	};

	return doc;
}

/**
 * Markup instruction for CriticMarkup-based editing
 *
 * The AI sends content WITH CriticMarkup markers ({++additions++}, {--deletions--}),
 * and this function inserts it at the specified position. The CriticMarkup extension
 * in CodeMirror handles rendering the markers with accept/reject buttons.
 */
export interface MarkupInstruction {
	/** Content with CriticMarkup markers to apply */
	content: string;
	/** Position to apply: replace_all, replace, before, after, start, end */
	position: 'start' | 'end' | 'before' | 'after' | 'replace' | 'replace_all';
	/** Anchor text to find for positioning (required for replace/before/after) */
	anchor?: string;
}

/**
 * Apply CriticMarkup content to the document
 *
 * This is the core editing function for CriticMarkup-based AI editing.
 * It inserts content (which may contain CriticMarkup markers) at the
 * specified position. The markers are rendered by the CriticMarkup
 * CodeMirror extension with accept/reject buttons.
 *
 * @param doc - The Yjs document
 * @param instruction - The markup instruction with content, position, and optional anchor
 * @returns true if content was applied successfully
 */
export function applyMarkup(doc: YjsDocument, instruction: MarkupInstruction): boolean {
	const { ytext, ydoc } = doc;
	const currentContent = ytext.toString();

	try {
		ydoc.transact(() => {
			switch (instruction.position) {
				case 'replace_all':
					// Replace entire document
					if (ytext.length > 0) {
						ytext.delete(0, ytext.length);
					}
					ytext.insert(0, instruction.content);
					break;

				case 'start':
					// Insert at document start
					ytext.insert(0, instruction.content);
					break;

				case 'end':
					// Insert at document end
					ytext.insert(ytext.length, instruction.content);
					break;

				case 'replace':
					// Replace anchor text with content
					if (instruction.anchor) {
						const idx = currentContent.indexOf(instruction.anchor);
						if (idx !== -1) {
							ytext.delete(idx, instruction.anchor.length);
							ytext.insert(idx, instruction.content);
						} else {
							console.warn(`applyMarkup: anchor not found for replace: "${instruction.anchor.slice(0, 50)}..."`);
						}
					}
					break;

				case 'before':
					// Insert content before anchor
					if (instruction.anchor) {
						const idx = currentContent.indexOf(instruction.anchor);
						if (idx !== -1) {
							ytext.insert(idx, instruction.content);
						} else {
							console.warn(`applyMarkup: anchor not found for before: "${instruction.anchor.slice(0, 50)}..."`);
						}
					}
					break;

				case 'after':
					// Insert content after anchor
					if (instruction.anchor) {
						const idx = currentContent.indexOf(instruction.anchor);
						if (idx !== -1) {
							ytext.insert(idx + instruction.anchor.length, instruction.content);
						} else {
							console.warn(`applyMarkup: anchor not found for after: "${instruction.anchor.slice(0, 50)}..."`);
						}
					}
					break;
			}
		}, 'ai'); // Tagged as AI edit for undo tracking

		return true;
	} catch (err) {
		console.error('Failed to apply markup:', err);
		return false;
	}
}

