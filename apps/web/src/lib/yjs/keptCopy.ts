/**
 * Copies this device kept of a block page that are not the page's document:
 *
 * - the copy of the page from when it was markdown (`v2-<id>`, kept by the
 *   markdown editor older apps carried), once it has moved to blocks
 *   (`keepMarkdownCopy`);
 * - a copy of an earlier document of the page (`tree-<id>` or
 *   `tree-<id>.<lineage>`, `treeStore`), once the page's document has been
 *   made again: put back from a backup and moved to blocks once more
 *   (`keepEarlierTreeCopies`).
 *
 * The page binds only the copy of its own document, so typing that never
 * reached the server (the box out of reach, the app closed before it
 * reconnected) would sit in the others unseen. The first time this device
 * opens the page, each other copy goes to the server, which keeps it as a
 * version when it holds typing the page never had
 * (`POST /api/pages/:id/device-copy`), and is then deleted. If the server
 * can't be reached, it is tried again the next time the page opens.
 */

import * as Y from 'yjs';
import { IndexeddbPersistence } from 'y-indexeddb';
import { toast } from 'svelte-sonner';
import { sendDeviceCopy } from '$lib/api/client';
import { bytesToBase64 } from '$lib/utils/base64';
import { treeStore } from './document';

/** A copy this device kept, opened. */
export interface KeptStore {
	doc: Y.Doc;
	/** Delete the copy from this device. */
	clear: () => Promise<void>;
	/** Close it, leaving it on this device. */
	close: () => void;
}

/** Open the copy kept under `name`; null when this device keeps none. */
export async function openKeptStore(name: string): Promise<KeptStore | null> {
	if (typeof indexedDB === 'undefined') return null;
	// Where the browser lists its stores, an absent one is not opened, which would make it.
	const listed = typeof indexedDB.databases === 'function' ? await indexedDB.databases().catch(() => null) : null;
	if (listed && !listed.some((d) => d.name === name)) return null;
	const doc = new Y.Doc();
	const persistence = new IndexeddbPersistence(name, doc);
	await persistence.whenSynced;
	return {
		doc,
		clear: async () => {
			await persistence.clearData();
			doc.destroy();
		},
		close: () => {
			void persistence.destroy();
			doc.destroy();
		},
	};
}

/** What the person is told when the copy held typing the page never had. */
export const KEPT_NOTICE =
	"Some typing on this device never reached your server before this page moved to blocks. It's kept in this page's History.";

/** Pages whose old copy this app has seen to since it opened. */
const seen = new Set<string>();

/**
 * See to the markdown copy this device kept of `pageId`, a block page: send
 * it to the server, then delete it. Once per page while the app runs.
 */
export async function keepMarkdownCopy(
	pageId: string,
	open: (name: string) => Promise<KeptStore | null> = openKeptStore,
	send: typeof sendDeviceCopy = sendDeviceCopy,
): Promise<void> {
	if (seen.has(pageId)) return;
	seen.add(pageId);
	let store: KeptStore | null;
	try {
		store = await open(`v2-${pageId}`);
	} catch {
		return;
	}
	if (!store) return;
	try {
		// A store that holds nothing has nothing to send.
		if (store.doc.store.clients.size > 0) {
			const { kept } = await send(pageId, bytesToBase64(Y.encodeStateAsUpdate(store.doc)));
			if (kept) toast.info(KEPT_NOTICE);
		}
		await store.clear();
	} catch {
		// Kept on this device, and sent again the next time the page opens.
		seen.delete(pageId);
		store.close();
	}
}

/** What the person is told when a copy of an earlier document had typing the page never had. */
export const EARLIER_COPY_NOTICE =
	"This device had typing from an earlier copy of this page that never reached your server. It's in this page's History now.";

/** Every store this device keeps, by name; null where the browser can't list them. */
async function storeNames(): Promise<string[] | null> {
	if (typeof indexedDB === 'undefined' || typeof indexedDB.databases !== 'function') return null;
	const listed = await indexedDB.databases().catch(() => null);
	return listed ? listed.flatMap((d) => (d.name ? [d.name] : [])) : null;
}

/**
 * The lineage a store of `pageId`'s blocks was kept under: the part after
 * `tree-<id>.`, or null for `tree-<id>` itself. Undefined when the store is
 * not one of this page's blocks (another page's whose id starts the same,
 * a markdown copy).
 */
export function lineageOf(name: string, pageId: string): string | null | undefined {
	const bare = `tree-${pageId}`;
	if (name === bare) return null;
	if (name.startsWith(`${bare}.`) && name.length > bare.length + 1) return name.slice(bare.length + 1);
	return undefined;
}

/** Pages, each with the lineage it was opened under, whose earlier copies this app has seen to. */
const seenEarlier = new Set<string>();

/**
 * See to every copy this device kept of an earlier document of `pageId`,
 * a block page whose document is `lineage`: send each to the server with
 * the lineage it was kept under, then delete it. Once per page and lineage
 * while the app runs. Where the browser can't list its stores, the copy
 * kept before lineages (`tree-<id>`) is the one looked for.
 */
export async function keepEarlierTreeCopies(
	pageId: string,
	lineage: string | null,
	list: () => Promise<string[] | null> = storeNames,
	open: (name: string) => Promise<KeptStore | null> = openKeptStore,
	send: typeof sendDeviceCopy = sendDeviceCopy,
): Promise<void> {
	const key = `${pageId}\n${lineage ?? ''}`;
	if (seenEarlier.has(key)) return;
	seenEarlier.add(key);
	let names: string[] | null;
	try {
		names = await list();
	} catch {
		names = null;
	}
	const own = treeStore(pageId, lineage);
	const candidates = names ?? (lineage ? [treeStore(pageId, null)] : []);
	const earlier = candidates.filter((name) => name !== own && lineageOf(name, pageId) !== undefined);
	let told = false;
	for (const name of earlier) {
		let store: KeptStore | null;
		try {
			store = await open(name);
		} catch {
			seenEarlier.delete(key);
			continue;
		}
		if (!store) continue;
		try {
			// A store that holds nothing has nothing to send.
			if (store.doc.store.clients.size > 0) {
				const { kept } = await send(
					pageId,
					bytesToBase64(Y.encodeStateAsUpdate(store.doc)),
					lineageOf(name, pageId) ?? null,
				);
				if (kept && !told) {
					toast.info(EARLIER_COPY_NOTICE);
					told = true;
				}
			}
			await store.clear();
		} catch {
			// Kept on this device, and sent again the next time the page opens.
			seenEarlier.delete(key);
			store.close();
		}
	}
}

/** Forget which pages were seen to: for tests. */
export function resetKeptCopies(): void {
	seen.clear();
	seenEarlier.clear();
}
