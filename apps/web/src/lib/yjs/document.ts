/**
 * Page bindings: a page's Yjs document on the page socket (`socket.ts`).
 *
 * - `createYjsDocument` binds a markdown page's Y.Text for the CodeMirror
 *   editor: contract 0, kept on this device under `v2-<id>`.
 * - `createTreeDocument` binds a block page's XML tree for the block editor
 *   (`lib/document`): the contract version `contract.json` says, kept under
 *   `tree-<id>`, so a page converted from markdown never reads its old copy.
 */

import * as Y from 'yjs';
import type { WebsocketProvider } from 'y-websocket';
import type { IndexeddbPersistence } from 'y-indexeddb';
import type { Writable } from 'svelte/store';
import contractJson from '$contract';
import { connectPage } from './socket';

/**
 * The document contract the markdown binding reads (`crates/virtues-document`).
 * It binds the page's Y.Text, which is contract 0: a page written as a Yjs tree
 * carries a higher version, and the server refuses this binding on it, since
 * an editor deletes from the shared document what it cannot read.
 */
const MARKDOWN_CONTRACT = 0;

/** The contract the block editor's schema is built from. */
const TREE_CONTRACT: number = (contractJson as { version: number }).version;
const TREE_FRAGMENT: string = (contractJson as { fragment: string }).fragment;

/** The key the server stamps the document's contract under (`ydoc.rs`, `META`). */
const META = 'meta';
const META_CONTRACT = 'contract';

const UNREADABLE = "Your server couldn't read this page's document.";
const SERVER_NEEDS_UPDATE = 'Your server needs an update before it can save changes to this page.';
const EMPTY_ON_SERVER = "This page's document is empty on your server.";

/** The contract a document is stamped with; 0 when it carries no stamp. */
function stampOf(ydoc: Y.Doc): number {
	// yrs writes the stamp as a 64-bit integer, which arrives as a bigint.
	return Number(ydoc.getMap(META).get(META_CONTRACT) ?? 0);
}

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
 * Create a Yjs document for a markdown page.
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
	const ytext = ydoc.getText('content');

	const contractConfirmed = pageContract !== null && pageContract <= MARKDOWN_CONTRACT;
	const socket = connectPage(pageId, ydoc, {
		contract: MARKDOWN_CONTRACT,
		store: `v2-${pageId}`,
		pageContract,
		// The copy carries no stamp above this binding's contract.
		readable: () => contractConfirmed && stampOf(ydoc) <= MARKDOWN_CONTRACT,
		hasContent: () => ytext.length > 0,
	});

	const undoManager = new Y.UndoManager(ytext, {
		trackedOrigins: new Set([null, 'user', 'ai']),
		captureTimeout: 500,
	});

	return {
		ydoc,
		ytext,
		provider: socket.provider,
		persistence: socket.persistence,
		undoManager,
		isLoading: socket.isLoading,
		isSynced: socket.isSynced,
		isConnected: socket.isConnected,
		refused: socket.refused,
		contractConfirmed,
		destroy: () => {
			undoManager.destroy();
			socket.destroy();
			ydoc.destroy();
		},
	};
}

/** A block page's document, bound for the block editor. */
export interface TreeDocument {
	format: 'tree';
	ydoc: Y.Doc;
	/** The page's tree: the fragment `contract.json` names. */
	fragment: Y.XmlFragment;
	provider: WebsocketProvider;
	persistence: IndexeddbPersistence;
	isLoading: Writable<boolean>;
	isSynced: Writable<boolean>;
	isConnected: Writable<boolean>;
	/**
	 * What to tell the person about this page's binding: why it is not
	 * shown, or why it is shown read-only. Null when there is nothing to say.
	 */
	refused: Writable<string | null>;
	/** Whether the server said the page is written under a contract this editor reads. */
	contractConfirmed: boolean;
	/**
	 * Whether the server is older than this editor's contract, so it would
	 * drop what this editor writes: the editor is made read-only.
	 */
	readOnly: boolean;
	/** Whether the copy this device kept may stand in for the page now. */
	localCopyReadable(): boolean;
	destroy(): void;
}

/**
 * Bind a block page.
 *
 * @param pageContract - `contract` from `GET /api/pages/:id`: the contract the
 *   page is stamped with, or null when the server could not read the page's
 *   document. Null binds nothing: there is no document to edit.
 * @param boxContract - `box_contract` from the same response: the newest
 *   contract the server reads, or null when the server does not say.
 *
 * The editor must be created only once `isSynced` is true. It is never true
 * for an empty tree: Tiptap bound to an empty fragment writes its empty
 * paragraph into the shared document, and the server always writes at
 * least one block.
 */
export function createTreeDocument(
	pageId: string,
	pageContract: number | null,
	boxContract: number | null,
): TreeDocument {
	const ydoc = new Y.Doc();
	const fragment = ydoc.getXmlFragment(TREE_FRAGMENT);

	const contractConfirmed = pageContract !== null && pageContract <= TREE_CONTRACT;
	const readOnly = boxContract !== null && TREE_CONTRACT > boxContract;

	// Kept from this page and this contract, and holding a block.
	const localCopyReadable = () =>
		contractConfirmed && stampOf(ydoc) === pageContract && fragment.length > 0;

	const socket = connectPage(pageId, ydoc, {
		contract: TREE_CONTRACT,
		store: `tree-${pageId}`,
		pageContract,
		readable: localCopyReadable,
		hasContent: () => fragment.length > 0,
		checkRemote: () => (fragment.length > 0 ? null : EMPTY_ON_SERVER),
		refusal: pageContract === null ? UNREADABLE : null,
	});
	if (readOnly && pageContract !== null && pageContract <= TREE_CONTRACT) {
		socket.refused.set(SERVER_NEEDS_UPDATE);
	}

	return {
		format: 'tree',
		ydoc,
		fragment,
		provider: socket.provider,
		persistence: socket.persistence,
		isLoading: socket.isLoading,
		isSynced: socket.isSynced,
		isConnected: socket.isConnected,
		refused: socket.refused,
		contractConfirmed,
		readOnly,
		localCopyReadable,
		destroy: () => {
			socket.destroy();
			ydoc.destroy();
		},
	};
}
