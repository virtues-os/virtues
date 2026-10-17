/**
 * A block page's binding: its Yjs document on the page socket (`socket.ts`).
 *
 * `createTreeDocument` binds the page's XML tree for the block editor
 * (`lib/document`): the contract version `contract.json` says, kept under
 * `tree-<id>.<lineage>` (`treeStore`), so a page converted from markdown
 * never reads the copy this device kept of it then (`v2-<id>`, which
 * `keptCopy.ts` sends to the server once), and a page whose document was
 * made again (put back from a backup and moved to blocks once more) never
 * merges in the copy of the earlier one.
 */

import * as Y from 'yjs';
import type { WebsocketProvider } from 'y-websocket';
import type { IndexeddbPersistence } from 'y-indexeddb';
import type { Writable } from 'svelte/store';
import contractJson from '$contract';
import { connectPage } from './socket';

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
 * The store this device keeps a block page's copy in: `tree-<id>.<lineage>`,
 * or `tree-<id>` for a block page from before lineages (its `lineage` null).
 * Two documents of one page never share a store, so a copy of one is never
 * merged into the other as a second copy of its text.
 */
export function treeStore(pageId: string, lineage: string | null): string {
	return lineage ? `tree-${pageId}.${lineage}` : `tree-${pageId}`;
}

/**
 * Bind a block page.
 *
 * @param pageContract - `contract` from `GET /api/pages/:id`: the contract the
 *   page is stamped with, or null when the server could not read the page's
 *   document. Null binds nothing: there is no document to edit.
 * @param boxContract - `box_contract` from the same response: the newest
 *   contract the server reads, or null when the server does not say.
 * @param lineage - `lineage` from the same response: which document of the
 *   page the server holds, which keys this device's copy (`treeStore`).
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
	lineage: string | null = null,
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
		store: treeStore(pageId, lineage),
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
