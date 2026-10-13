/**
 * The page socket: what every page binding shares, whatever it binds (the
 * markdown Y.Text in `createYjsDocument`, the block tree in
 * `createTreeDocument`).
 *
 * - The websocket carries the contract the binding reads (`?contract=N`),
 *   and the server refuses to bind it below the page's (`server/yjs.rs`).
 * - A page written under a newer contract is not asked for at all: an editor
 *   deletes from the shared document what it cannot read.
 * - The server closes a socket it refuses with a code y-websocket treats as
 *   final (4400-4499), and says why.
 * - Remote sync is authoritative. The copy this device kept stands in for
 *   the page before the server syncs only when the binding says it may
 *   (`readable`) and it holds something (`hasContent`).
 * - An edit the server refused is in that copy too, and would be sent again
 *   from it on every open: the copy is dropped when the server refuses, and
 *   the open editor keeps what it shows for the person to copy out.
 */

import { WebsocketProvider } from 'y-websocket';
import { IndexeddbPersistence } from 'y-indexeddb';
import { writable, type Writable } from 'svelte/store';
import type * as Y from 'yjs';
import { getWsUrl } from '$lib/config/backend';

/** Close codes the server ends a refused socket with (`server/yjs.rs`). */
export const CLOSE_CONTRACT = 4426;
export const CLOSE_REFUSED = 4422;
/** The page moved to the other format while this binding had it open. */
export const CLOSE_REFORMATTED = 4409;

export const NEEDS_NEWER_APP = 'A newer version of Virtues wrote this page. Update the app to open it.';
const EDIT_REFUSED =
	"Your server couldn't accept an edit to this page, so changes here won't save. Copy what you need, then reload the page.";
const PAGE_CONVERTED =
	"Your server converted this page while it was open, so changes here won't save. Copy what you need, then open the page again.";

export interface PageSocketOptions {
	/** The document contract the binding reads, sent on the socket. */
	contract: number;
	/** The IndexedDB store this device keeps its copy of the page in. */
	store: string;
	/**
	 * The contract the server says the page is written under (`contract` in
	 * `GET /api/pages/:id`), or null when it has not said.
	 */
	pageContract: number | null;
	/** Whether the copy this device kept may stand in for the page. */
	readable: () => boolean;
	/** Whether the document holds anything to show. */
	hasContent: () => boolean;
	/**
	 * Asked when the server has synced: why the page cannot be shown, or
	 * null when it can.
	 */
	checkRemote?: () => string | null;
	/**
	 * Why the binding must not ask the server at all, known before it would.
	 * A page under a newer contract than `contract` is refused without it.
	 */
	refusal?: string | null;
}

export interface PageSocket {
	provider: WebsocketProvider;
	persistence: IndexeddbPersistence;
	isLoading: Writable<boolean>;
	isSynced: Writable<boolean>;
	isConnected: Writable<boolean>;
	/**
	 * Why the server will not sync this page with this binding, once that is
	 * known. Null until then. A refused binding does not reconnect.
	 */
	refused: Writable<string | null>;
	destroy: () => void;
}

export function connectPage(pageId: string, ydoc: Y.Doc, o: PageSocketOptions): PageSocket {
	const isLoading = writable(true);
	const isSynced = writable(false);
	const isConnected = writable(false);
	const refused = writable<string | null>(null);

	const needsNewerApp = o.pageContract !== null && o.pageContract > o.contract;
	const refusal = o.refusal ?? (needsNewerApp ? NEEDS_NEWER_APP : null);
	if (refusal !== null) {
		refused.set(refusal);
		isLoading.set(false);
	}

	// Base WS URL (y-websocket appends the page id). Same-origin on desktop;
	// routed to the iroh loopback on mobile via the backend config.
	const provider = new WebsocketProvider(getWsUrl('/ws/yjs'), pageId, ydoc, {
		connect: refusal === null,
		maxBackoffTime: 10000,
		params: { contract: String(o.contract) },
	});

	const persistence = new IndexeddbPersistence(o.store, ydoc);

	// For a brand-new page IndexedDB fires 'synced' at once with nothing in
	// it, which would show an empty editor before the server's copy arrives;
	// so the local copy counts only when it holds something.
	let localSynced = false;
	let remoteSynced = false;

	function ready() {
		isSynced.set(true);
		isLoading.set(false);
	}

	function checkSyncComplete() {
		if (remoteSynced) {
			const why = o.checkRemote?.() ?? null;
			if (why === null) {
				ready();
			} else {
				refused.set(why);
				isSynced.set(false);
				isLoading.set(false);
			}
		} else if (localSynced && o.hasContent() && o.readable()) {
			ready();
		}
	}

	persistence.on('synced', () => {
		localSynced = true;
		checkSyncComplete();
	});

	provider.on('status', (event: { status: string }) => {
		isConnected.set(event.status === 'connected');
	});

	// y-websocket also emits `sync` with false when a synced socket drops;
	// the page stays shown, on the copy it has.
	provider.on('sync', (synced?: boolean) => {
		if (synced === false) return;
		remoteSynced = true;
		checkSyncComplete();
	});

	provider.on('connection-error', () => {
		// Offline, the copy this device kept is the best there is, empty or
		// not, when the binding may read it.
		if (localSynced && o.readable()) ready();
	});

	provider.on('closed', (event: { code: number; reason: string }) => {
		if (event.code !== CLOSE_CONTRACT && event.code !== CLOSE_REFUSED && event.code !== CLOSE_REFORMATTED) return;
		console.warn(`[yjs] the server closed page ${pageId} (${event.code}): ${event.reason}`);
		if (event.code === CLOSE_REFORMATTED) {
			// The editor stays, so what was typed can still be copied out. The
			// page opens again in the editor its new format takes, which keeps
			// its own copy on this device.
			refused.set(PAGE_CONVERTED);
		} else if (event.code === CLOSE_CONTRACT) {
			refused.set(NEEDS_NEWER_APP);
			// A local copy may already be showing; it is not this page any more.
			isSynced.set(false);
		} else {
			// The editor stays, so what was typed can still be copied out;
			// the stored copy goes, so the next open starts from the server's.
			refused.set(EDIT_REFUSED);
			persistence.clearData().catch((e: unknown) => {
				console.warn(`[yjs] the copy of page ${pageId} this device kept was not dropped`, e);
			});
		}
		isLoading.set(false);
	});

	return {
		provider,
		persistence,
		isLoading,
		isSynced,
		isConnected,
		refused,
		destroy: () => {
			provider.destroy();
			persistence.destroy();
		},
	};
}
