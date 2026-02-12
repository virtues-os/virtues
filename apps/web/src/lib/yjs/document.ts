/**
 * Yjs Document Manager for ProseMirror
 *
 * Creates and manages Yjs documents for real-time collaborative editing.
 * Uses Y.XmlFragment with y-prosemirror for ProseMirror integration.
 * Handles WebSocket sync, IndexedDB persistence, and undo management.
 */

import * as Y from 'yjs';
import { WebsocketProvider } from 'y-websocket';
import { IndexeddbPersistence } from 'y-indexeddb';
import { ySyncPlugin, yCursorPlugin, yUndoPlugin } from 'y-prosemirror';
import type { Plugin } from 'prosemirror-state';
import { writable, type Writable } from 'svelte/store';

export interface YjsDocument {
	ydoc: Y.Doc;
	yxmlFragment: Y.XmlFragment;
	provider: WebsocketProvider;
	persistence: IndexeddbPersistence;
	undoManager: Y.UndoManager;

	/**
	 * ProseMirror plugins for Yjs collaboration.
	 * Add these to your EditorState plugins array.
	 */
	plugins: Plugin[];

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

	// Use XmlFragment for ProseMirror (not Y.Text)
	const yxmlFragment = ydoc.getXmlFragment('content');

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
	const persistence = new IndexeddbPersistence(pageId, ydoc);

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
		} else if (localSynced && yxmlFragment.length > 0) {
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

	// UndoManager for XmlFragment
	const undoManager = new Y.UndoManager(yxmlFragment, {
		trackedOrigins: new Set([null, 'user', 'ai']),
		captureTimeout: 500,
	});

	// ProseMirror plugins for Yjs collaboration
	const plugins: Plugin[] = [
		ySyncPlugin(yxmlFragment),
		yCursorPlugin(provider.awareness),
		yUndoPlugin(),
	];

	// Create the document object
	const doc: YjsDocument = {
		ydoc,
		yxmlFragment,
		provider,
		persistence,
		undoManager,
		plugins,
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

/**
 * Markup instruction for CriticMarkup-based editing
 *
 * The AI sends content WITH CriticMarkup markers ({++additions++}, {--deletions--}),
 * and this function inserts it at the specified position. The CriticMarkup plugin
 * in ProseMirror handles rendering the markers with accept/reject buttons.
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
 * Apply CriticMarkup content to a ProseMirror editor
 *
 * This is the core editing function for CriticMarkup-based AI editing.
 * It parses markdown content (which may contain CriticMarkup markers)
 * and inserts it at the specified position in the ProseMirror document.
 *
 * @param view - The ProseMirror EditorView
 * @param instruction - The markup instruction with content, position, and optional anchor
 * @param parser - The markdown parser function
 * @param serializer - The markdown serializer function
 * @returns true if content was applied successfully
 */
export function applyMarkup(
	view: import('prosemirror-view').EditorView,
	instruction: MarkupInstruction,
	parser: (markdown: string) => import('prosemirror-model').Node,
	serializer: (doc: import('prosemirror-model').Node) => string
): boolean {
	const { state, dispatch } = view;
	const { doc } = state;

	try {
		// Parse the new content
		const newDoc = parser(instruction.content);
		if (!newDoc) return false;

		let tr = state.tr;

		switch (instruction.position) {
			case 'replace_all': {
				// Replace entire document
				tr = tr.replaceWith(0, doc.content.size, newDoc.content);
				break;
			}

			case 'start': {
				// Insert at document start
				tr = tr.insert(0, newDoc.content);
				break;
			}

			case 'end': {
				// Insert at document end
				tr = tr.insert(doc.content.size, newDoc.content);
				break;
			}

			case 'replace': {
				// Replace anchor text with content
				if (instruction.anchor) {
					const currentContent = serializer(doc);
					const idx = currentContent.indexOf(instruction.anchor);
					if (idx !== -1) {
						// Find positions in ProseMirror doc that correspond to the anchor
						const range = findTextRange(doc, instruction.anchor);
						if (range) {
							tr = tr.replaceWith(range.from, range.to, newDoc.content);
						} else {
							console.warn(
								`applyMarkup: anchor not found in doc for replace: "${instruction.anchor.slice(0, 50)}..."`
							);
							return false;
						}
					} else {
						console.warn(
							`applyMarkup: anchor not found for replace: "${instruction.anchor.slice(0, 50)}..."`
						);
						return false;
					}
				}
				break;
			}

			case 'before': {
				// Insert content before anchor
				if (instruction.anchor) {
					const range = findTextRange(doc, instruction.anchor);
					if (range) {
						tr = tr.insert(range.from, newDoc.content);
					} else {
						console.warn(
							`applyMarkup: anchor not found for before: "${instruction.anchor.slice(0, 50)}..."`
						);
						return false;
					}
				}
				break;
			}

			case 'after': {
				// Insert content after anchor
				if (instruction.anchor) {
					const range = findTextRange(doc, instruction.anchor);
					if (range) {
						tr = tr.insert(range.to, newDoc.content);
					} else {
						console.warn(
							`applyMarkup: anchor not found for after: "${instruction.anchor.slice(0, 50)}..."`
						);
						return false;
					}
				}
				break;
			}
		}

		// Apply the transaction with AI origin for undo tracking
		tr.setMeta('origin', 'ai');
		dispatch(tr);

		return true;
	} catch (err) {
		console.error('Failed to apply markup:', err);
		return false;
	}
}

/**
 * Find the position range of text in a ProseMirror document
 */
function findTextRange(
	doc: import('prosemirror-model').Node,
	text: string
): { from: number; to: number } | null {
	let foundFrom: number | null = null;
	let foundTo: number | null = null;
	let currentText = '';
	let startPos = 0;

	doc.descendants((node, pos) => {
		if (foundFrom !== null && foundTo !== null) return false; // Stop if found

		if (node.isText) {
			const nodeText = node.text || '';
			currentText += nodeText;

			// Check if the anchor text is in our accumulated text
			const idx = currentText.indexOf(text);
			if (idx !== -1) {
				// Calculate actual positions
				foundFrom = startPos + idx;
				foundTo = foundFrom + text.length;
				return false; // Stop traversal
			}
		} else if (node.isBlock && currentText.length > 0) {
			// Reset for new block
			currentText = '';
			startPos = pos + 1;
		}
	});

	if (foundFrom !== null && foundTo !== null) {
		return { from: foundFrom, to: foundTo };
	}

	return null;
}
