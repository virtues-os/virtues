/**
 * Files on their way to the server, after their blocks are already in the
 * page (`media.ts`). A file's block holds its final address from the start,
 * `/drive/<id>` with an id made here (`mintFileId`), and the bytes follow on
 * their own: nothing here writes to a page, so an upload ending, failing or
 * being tried again changes no document, and undo, redo and other devices'
 * edits never meet it.
 *
 * The queue belongs to the app, not to an editor: an upload goes on when its
 * page closes or another opens, and only closing the app ends it, which the
 * app asks about first (`beforeunload`) while any file is at stake
 * (`uploadsAtStake`), which every reload waits for (`$lib/reload`), and
 * which the desktop app's shell, quitting or updating itself where no
 * `beforeunload` runs, waits for (`watchUploadsAtStake`).
 *
 * At most `SEND_AT_ONCE` files are sent at a time. A browser opens a few
 * connections to a server and holds every request past them where the
 * server cannot see it, while each file's block is already in the page on
 * every device. So each file is said to be coming before its bytes can be
 * (`announcePageFiles`), and again while it waits: a device asking about its
 * block, and a share of its page, wait for it rather than take it for a
 * file never sent. Fewer requests than the browser's connections carry
 * files, so that word, and every other request of the app, still gets
 * through.
 *
 * A network failure or a server error is tried again after 2, 8, 30 and 60
 * seconds, then every few minutes (`TRY_AGAIN_EVERY_MS`) for as long as the
 * app is open, and at once when the device is back online or a page's socket
 * reaches the server again (`serverBack`), or on Try again. A refusal (a
 * file too large, an address already taken) is never sent again, and says
 * why.
 *
 * A block that leaves every page open here (undo, Remove, a cut, another
 * device's delete) may still be in a page this device does not show:
 * pasted into one, or moved there by another device or the model.
 * So its file stays at stake, and goes on, until the server says no page
 * names it (`getNamedUploads`, asked `CHECK_AFTER_MS` after the block left,
 * and before each try every few minutes); then it is let go, not at stake
 * and not sent, until its block is back in a page here (redo, a paste),
 * which sends it again. Let go on what the open pages showed, a file whose
 * block was copied into a page then closed was never sent, and the copy
 * named a file that never came. A block turned into a link to its address
 * has not left.
 *
 * What a block shows comes from here: `watchUpload` on the device that
 * holds the file, and `askUpload` on every other device, which learns from
 * the server whether the bytes are there, still arriving, or never came
 * (`GET /api/media/uploads?ids=`, every block it asks about in one request).
 */

import { readable, writable, type Readable, type Writable } from 'svelte/store';
import {
	announcePageFiles,
	getMediaUploads,
	getNamedUploads,
	sendPageFile,
	UploadError,
	type PageFileState,
} from '$lib/api/client';

/** What the device that holds a file knows of its upload. `preview` is the file itself, while it is an image. */
export type LocalUpload =
	| { phase: 'uploading'; name: string; sent: number; total: number; preview: string | null }
	| { phase: 'done'; name: string; preview: string | null }
	/** `refused`: the server refused the file, so sending it again would be refused again. */
	| { phase: 'failed'; name: string; reason: string; refused: boolean; preview: string | null };

/**
 * What any other device knows of a file: the server's word for it, or
 * `asking` until the first one. `waiting`: the block came before its file,
 * which the server has not heard of yet. `stopped`: the upload stopped short
 * of a refusal, and the device holding the file may still send it again
 * (`SENDER_TRIES_FOR_MS`); after that a stop is `failed`.
 */
export type RemoteUpload =
	| PageFileState
	| { state: 'asking' }
	| { state: 'waiting' }
	| { state: 'stopped'; filename: string; reason: string };

/** Retries after a network failure or a server error, in seconds. */
const RETRY_AFTER = [2, 8, 30, 60];

/** After its retries, a file stopped short of a refusal is tried again this often, for as long as the app is open. */
export const TRY_AGAIN_EVERY_MS = 3 * 60_000;

/** The most files sent at once: fewer than the six connections a browser opens to a server. */
export const SEND_AT_ONCE = 3;

/** A file waiting for its bytes to go is said to be coming again this often, well within the server's `STALLED_AFTER`. */
export const ANNOUNCE_EVERY_MS = 30_000;

/** A file whose block left every page here is asked about this long after: the edit has reached the server by then. */
export const CHECK_AFTER_MS = 5000;

/** The most ids one request names, as the server takes them. */
export const ASK_AT_ONCE = 100;

/**
 * A new Drive file id, made here so the file's block can hold its final
 * address before any byte is sent: `file_` and 16 hex digits, 64 random
 * bits, the shape the server's own Drive ids take, which it checks.
 */
export function mintFileId(): string {
	const bytes = crypto.getRandomValues(new Uint8Array(8));
	return `file_${Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('')}`;
}

interface Entry {
	file: File;
	preview: string | null;
	state: Writable<LocalUpload>;
	tries: number;
	timer: ReturnType<typeof setTimeout> | null;
	/** A request carries it now. */
	sending: boolean;
	/** That request's bytes have begun, so the server knows the file by them. */
	moving: boolean;
	/** Waiting for a request of its own (`SEND_AT_ONCE`). */
	queued: boolean;
	/** Stored. */
	done: boolean;
	/** The server refused the file: it is never sent again. */
	refused: boolean;
	/** Out of its retries, short of a refusal: tried again every few minutes. */
	failed: boolean;
	/** A page open here shows its block. */
	shown: boolean;
	/** No page names it, the server said, and none open here shows it: not at stake, and not sent until its block is back. */
	released: boolean;
}

/** The files this device has given, by id; until stored, each is the only copy, in this page's memory. */
const local = new Map<string, Entry>();
/** Files waiting for a request, in the order given. */
let waiting: string[] = [];
/** Requests carrying a file now. */
let active = 0;
/** Bumped by `resetUploads`, so a request from before it settles nothing after. */
let epoch = 0;
let announcing = false;
let announceTimer: ReturnType<typeof setTimeout> | null = null;
let checkTimer: ReturnType<typeof setTimeout> | null = null;
let restTimer: ReturnType<typeof setTimeout> | null = null;

/**
 * Files whose bytes have not reached the server and still may: sending,
 * waiting to be sent or tried again, or stopped short of a refusal, which
 * is tried again every few minutes. Each one's block is already in a page
 * on every device, and the file is only in this page's memory, so the app
 * asks before it closes, and reloads only while there are none
 * (`$lib/reload`, `tauri/quietReload.ts`). A refused file is not counted:
 * no reload loses what the server would never take. Nor is one let go,
 * which no page names.
 */
export function uploadsAtStake(): number {
	let n = 0;
	for (const entry of local.values()) if (!entry.done && !entry.refused && !entry.released) n++;
	return n;
}

/** `uploadsAtStake()` as it changes. */
const stake = writable(0);

function restake(): void {
	stake.set(uploadsAtStake());
}

/**
 * How many files are at stake (`uploadsAtStake`), told on every change, for
 * the page's own guards (no quiet reload, a warning before the tab closes).
 * Quitting the desktop app is not held: a file stopped that way shows Try
 * again on its block.
 */
export function watchUploadsAtStake(): Readable<number> {
	return { subscribe: stake.subscribe };
}

/** The files this device holds that are neither stored nor refused: the blocks an editor watches leave and come back. */
export function unsettledUploads(): ReadonlySet<string> {
	const ids = new Set<string>();
	for (const [id, entry] of local) if (!entry.done && !entry.refused) ids.add(id);
	return ids;
}

/**
 * These files' blocks are in no page open here any more. Each goes on as it
 * was, at stake, and the server is asked in a moment whether a page still
 * names it: one none names is let go.
 */
export function uploadBlocksLeft(ids: Iterable<string>): void {
	let left = false;
	for (const id of ids) {
		const entry = local.get(id);
		if (!entry || entry.done || entry.refused) continue;
		entry.shown = false;
		left = true;
	}
	if (left) scheduleCheck();
}

/** These files' blocks are in a page here (redo, a paste, the page opened): each one let go goes now. */
export function uploadBlocksBack(ids: Iterable<string>): void {
	for (const id of ids) {
		const entry = local.get(id);
		if (!entry) continue;
		entry.shown = true;
		if (!entry.released) continue;
		entry.released = false;
		entry.tries = 0;
		send(id, entry);
	}
	restake();
}

/** Let `entry` go: no page names it. A request already out finishes, and is not tried again. */
function release(entry: Entry): void {
	entry.released = true;
	entry.queued = false;
	if (entry.timer) clearTimeout(entry.timer);
	entry.timer = null;
}

/** Ask the server which of `ids` a page names, and let go each one none does that no page here shows. */
async function checkNamed(ids: string[]): Promise<Record<string, boolean>> {
	const mine = epoch;
	const named: Record<string, boolean> = {};
	for (let i = 0; i < ids.length; i += ASK_AT_ONCE) Object.assign(named, await getNamedUploads(ids.slice(i, i + ASK_AT_ONCE)));
	if (mine !== epoch) return {};
	for (const id of ids) {
		const entry = local.get(id);
		if (entry && named[id] === false && !entry.shown && !entry.done && !entry.refused) release(entry);
	}
	restake();
	return named;
}

/** Ask, `CHECK_AFTER_MS` from now, about every file whose block no page here shows. */
function scheduleCheck(): void {
	if (checkTimer) clearTimeout(checkTimer);
	checkTimer = setTimeout(() => {
		checkTimer = null;
		const ids = [...local].filter(([, e]) => !e.shown && !e.released && !e.done && !e.refused).map(([id]) => id);
		// Unanswered, each goes on: asked again before its next try.
		if (ids.length) void checkNamed(ids).catch(() => {});
	}, CHECK_AFTER_MS);
}

/** Every few minutes, while a file is out of retries or let go: try each again, or let it go. */
function rest(): void {
	if (restTimer) return;
	restTimer = setTimeout(() => {
		restTimer = null;
		void tryAgainNow();
	}, TRY_AGAIN_EVERY_MS);
}

/**
 * Send every file out of retries again, once, unless the server says no
 * page names one that no page here shows, which is let go; and every file
 * let go that a page names again (another device pasted its block), which
 * comes back.
 */
async function tryAgainNow(): Promise<void> {
	const resting = [...local].filter(([, e]) => (e.failed || e.released) && !e.done && !e.refused && !e.sending && !e.queued);
	if (!resting.length) return;
	const unshown = resting.filter(([, e]) => !e.shown).map(([id]) => id);
	let named: Record<string, boolean> | null = null;
	try {
		named = unshown.length ? await checkNamed(unshown) : {};
	} catch {
		// Unanswered: a file still at stake is tried, a file let go stays so.
	}
	for (const [id, entry] of resting) {
		if (entry.done || entry.refused || entry.sending || entry.queued) continue;
		// Once each round; let go and named again, with its retries.
		if (entry.released) {
			if (named?.[id] !== true) continue;
			entry.released = false;
			entry.tries = 0;
		}
		send(id, entry);
	}
	restake();
	if ([...local.values()].some((e) => (e.failed || e.released) && !e.done && !e.refused)) rest();
}

/**
 * The server can be reached again (the device back online, a page's socket
 * connected again): every file waiting on a retry, or out of retries short
 * of a refusal, goes now.
 */
export function serverBack(): void {
	for (const [id, entry] of local) {
		if (entry.sending || entry.queued || entry.done || entry.refused || entry.released) continue;
		if (entry.timer) clearTimeout(entry.timer);
		entry.timer = null;
		entry.tries = 0;
		send(id, entry);
	}
}

let listening = false;

function listen(): void {
	if (listening || typeof window === 'undefined') return;
	listening = true;
	window.addEventListener('beforeunload', (event) => {
		if (!uploadsAtStake()) return;
		event.preventDefault();
		// Some browsers show the prompt only for a value returned.
		event.returnValue = '';
	});
	window.addEventListener('online', serverBack);
}

function previewOf(file: File): string | null {
	if (!file.type.startsWith('image/') || typeof URL.createObjectURL !== 'function') return null;
	try {
		return URL.createObjectURL(file);
	} catch {
		return null;
	}
}

/**
 * Say every file is coming that waits for its bytes to go: queued here, or
 * sent with none gone yet, which the browser may be holding. Once in this
 * turn, for every file given or sent in it; then, while any waits in the
 * queue, every `ANNOUNCE_EVERY_MS`.
 */
function announceSoon(): void {
	if (announcing) return;
	announcing = true;
	queueMicrotask(() => {
		announcing = false;
		announce((e) => e.queued || (e.sending && !e.moving));
	});
}

function announce(waits: (entry: Entry) => boolean): void {
	if (announceTimer) clearTimeout(announceTimer);
	announceTimer = null;
	const files = [...local]
		.filter(([, e]) => !e.done && !e.refused && !e.released && waits(e))
		.map(([id, e]) => ({ id, filename: e.file.name, size: e.file.size }));
	// Unheard, the file reads as never sent until its bytes begin, or until it is said again.
	for (let i = 0; i < files.length; i += ASK_AT_ONCE) void announcePageFiles(files.slice(i, i + ASK_AT_ONCE)).catch(() => {});
	if ([...local.values()].some((e) => e.queued)) announceTimer = setTimeout(() => announce((e) => e.queued), ANNOUNCE_EVERY_MS);
}

/** Send `entry` as soon as a request is free, unless it is on its way already, settled or let go. */
function send(id: string, entry: Entry): void {
	if (entry.done || entry.refused || entry.released || entry.sending || entry.queued) return;
	const { file, preview } = entry;
	entry.failed = false;
	entry.state.set({ phase: 'uploading', name: file.name, sent: 0, total: file.size, preview });
	if (active >= SEND_AT_ONCE) {
		entry.queued = true;
		waiting.push(id);
	} else {
		transmit(id, entry);
	}
	announceSoon();
}

/** The next files waiting, while requests are free. */
function pump(): void {
	while (active < SEND_AT_ONCE && waiting.length) {
		const id = waiting.shift()!;
		const entry = local.get(id);
		if (!entry?.queued) continue;
		entry.queued = false;
		if (entry.done || entry.refused || entry.released) continue;
		transmit(id, entry);
	}
}

function transmit(id: string, entry: Entry): void {
	const { file, preview } = entry;
	const name = file.name;
	const mine = epoch;
	active++;
	entry.sending = true;
	entry.moving = false;
	sendPageFile(id, file, (sent, total) => {
		entry.moving = true;
		entry.state.set({ phase: 'uploading', name, sent, total, preview });
	}).then(
		() => {
			if (mine !== epoch) return;
			active--;
			entry.sending = false;
			entry.done = true;
			entry.state.set({ phase: 'done', name, preview });
			restake();
			pump();
		},
		(error: unknown) => {
			if (mine !== epoch) return;
			active--;
			entry.sending = false;
			const refused = error instanceof UploadError && error.refused;
			entry.refused = refused;
			if (!refused && !entry.released && entry.tries < RETRY_AFTER.length) {
				const wait = RETRY_AFTER[entry.tries++] * 1000;
				entry.timer = setTimeout(() => {
					entry.timer = null;
					send(id, entry);
				}, wait);
			} else {
				const reason = error instanceof Error ? error.message : String(error);
				entry.failed = !refused;
				entry.state.set({ phase: 'failed', name, reason, refused, preview });
				if (entry.failed) rest();
			}
			restake();
			pump();
		},
	);
}

/** Send `file` to the address `id` names, now or as soon as a request is free, and again until it arrives or is refused. */
export function startUpload(id: string, file: File): void {
	if (local.has(id)) return;
	listen();
	const preview = previewOf(file);
	const entry: Entry = {
		file,
		preview,
		state: writable<LocalUpload>({ phase: 'uploading', name: file.name, sent: 0, total: file.size, preview }),
		tries: 0,
		timer: null,
		sending: false,
		moving: false,
		queued: false,
		done: false,
		refused: false,
		failed: false,
		shown: true,
		released: false,
	};
	local.set(id, entry);
	restake();
	send(id, entry);
}

/**
 * Try a failed upload again, to the same address: the block in the page is
 * unchanged. Never a refused one, which the server would refuse again, and
 * which every other device has stopped asking about.
 */
export function retryUpload(id: string): void {
	const entry = local.get(id);
	if (!entry || entry.sending || entry.queued || entry.timer || entry.done || entry.refused) return;
	// Try again is on a block that is shown, so it is in a page.
	entry.shown = true;
	entry.released = false;
	entry.tries = 0;
	restake();
	send(id, entry);
}

/** The upload of `id` as this device holds it; null when this device holds no such file. */
export function watchUpload(id: string | null): Readable<LocalUpload | null> {
	const entry = id ? local.get(id) : undefined;
	return entry ? entry.state : readable(null);
}

// ── Other devices' uploads ───────────────────────────────────────────────

/**
 * Asked again this often while the file is not stored, less often after a
 * minute, and once a minute after ten: a file that never came may still
 * come, whenever the device holding it is back.
 */
export const ASK_EVERY_MS = 2000;
export const ASK_SLOWER_MS = 10_000;
export const ASK_SLOWEST_MS = 60_000;
const SLOWER_AFTER_MS = 60_000;
const SLOWEST_AFTER_MS = 10 * 60_000;
/** A block can reach this device before its file reaches the server: `missing` counts only after this. */
export const MISSING_GRACE_MS = 10_000;
/**
 * How long the device holding a file goes on trying quickly after an upload
 * stops short of a refusal: its retries (`RETRY_AFTER`), and the time each
 * takes. Until then, from when the upload stopped, a stop is said as one it
 * may still recover from; after, as a stop, though that device still tries
 * every few minutes (`TRY_AGAIN_EVERY_MS`) and a block hears of it.
 */
export const SENDER_TRIES_FOR_MS = (RETRY_AFTER.reduce((a, b) => a + b, 0) + 20) * 1000;

interface Asked {
	state: Writable<RemoteUpload>;
	/** The server's last answer, or `asking` before its first. */
	now: PageFileState | { state: 'asking' };
	since: number;
	/**
	 * When the upload stopped short of a refusal, by this device's clock, as
	 * the server last said (`stopped_ms_ago`): a device first asking long
	 * after the stop hears at once that the sending device has stopped trying.
	 */
	stoppedAt: number | null;
	watchers: number;
}

const asked = new Map<string, Asked>();
let askTimer: ReturnType<typeof setTimeout> | null = null;
/** When the pending ask goes out. */
let askAt = Infinity;
let asking = false;
/** A block began asking while a request was out: the next request goes as soon as it answers. */
let askAgain = false;

/**
 * Whether the server's word on a file is its last: stored, or refused. A
 * file cut off, gone quiet or missing may still come, sent again by the
 * device holding it (its retries, or Try again), so it is asked about for
 * as long as a block watches it.
 */
function final(a: Asked): boolean {
	return a.now.state === 'done' || (a.now.state === 'failed' && a.now.refused);
}

/** What a block is told of `a` at `now`. */
function told(a: Asked, now: number): RemoteUpload {
	const s = a.now;
	// A block can arrive before its file starts: missing is waiting, at first.
	if (s.state === 'missing' && now - a.since < MISSING_GRACE_MS) return { state: 'waiting' };
	if (s.state === 'failed' && !s.refused && a.stoppedAt !== null && now - a.stoppedAt < SENDER_TRIES_FOR_MS) {
		return { state: 'stopped', filename: s.filename, reason: s.reason };
	}
	return s;
}

/**
 * Ask within `delay`, keeping the soonest of the asks scheduled: a block
 * new to a page asks at once, even while an old one's file is asked about
 * once a minute.
 */
function scheduleAsk(delay: number): void {
	if (asking) {
		if (delay === 0) askAgain = true;
		return;
	}
	const at = Date.now() + delay;
	if (askTimer) {
		if (at >= askAt) return;
		clearTimeout(askTimer);
	}
	askAt = at;
	askTimer = setTimeout(() => {
		askTimer = null;
		askAt = Infinity;
		void askNow();
	}, delay);
}

async function askNow(): Promise<void> {
	const ids = [...asked.entries()].filter(([, a]) => a.watchers > 0 && !final(a)).map(([id]) => id);
	if (!ids.length) return;
	asking = true;
	askAgain = false;
	try {
		for (let i = 0; i < ids.length; i += ASK_AT_ONCE) {
			const batch = ids.slice(i, i + ASK_AT_ONCE);
			const answers = await getMediaUploads(batch);
			const now = Date.now();
			for (const id of batch) {
				const a = asked.get(id);
				const answer = answers[id];
				if (!a || !answer) continue;
				a.now = answer;
				if (answer.state === 'failed' && !answer.refused) {
					a.stoppedAt = Number.isFinite(answer.stopped_ms_ago) ? now - answer.stopped_ms_ago : (a.stoppedAt ?? now);
				} else {
					a.stoppedAt = null;
				}
				a.state.set(told(a, now));
			}
		}
	} catch {
		// Asked again on the next turn.
	} finally {
		asking = false;
	}
	const now = Date.now();
	const open = [...asked.values()].filter((a) => a.watchers > 0 && !final(a));
	if (!open.length) return;
	const newest = Math.max(...open.map((a) => a.since));
	if (askAgain) scheduleAsk(0);
	scheduleAsk(now - newest < SLOWER_AFTER_MS ? ASK_EVERY_MS : now - newest < SLOWEST_AFTER_MS ? ASK_SLOWER_MS : ASK_SLOWEST_MS);
}

/**
 * What the server says of the file at `id`, asked for a block whose file
 * did not load on this device, or a file's card, which loads nothing. Asked
 * again while it is not stored, every watched block's in one request, until
 * the server's word is final or nothing watches it. Subscribing is what
 * asks; `asking` is all a block hears until the first answer.
 */
export function askUpload(id: string): Readable<RemoteUpload> {
	let a = asked.get(id);
	if (!a) {
		a = {
			state: writable<RemoteUpload>({ state: 'asking' }),
			now: { state: 'asking' },
			since: Date.now(),
			stoppedAt: null,
			watchers: 0,
		};
		asked.set(id, a);
	}
	const entry = a;
	return readable<RemoteUpload>(entry.now.state === 'asking' ? entry.now : told(entry, Date.now()), (set) => {
		entry.watchers++;
		const stop = entry.state.subscribe(set);
		// Every block that asks in this turn goes in one request.
		if (!final(entry)) scheduleAsk(0);
		return () => {
			stop();
			entry.watchers--;
		};
	});
}

/** Forget every upload: for tests. */
export function resetUploads(): void {
	epoch++;
	for (const entry of local.values()) if (entry.timer) clearTimeout(entry.timer);
	local.clear();
	waiting = [];
	active = 0;
	announcing = false;
	for (const timer of [announceTimer, checkTimer, restTimer]) if (timer) clearTimeout(timer);
	announceTimer = checkTimer = restTimer = null;
	restake();
	asked.clear();
	if (askTimer) clearTimeout(askTimer);
	askTimer = null;
	askAt = Infinity;
	asking = false;
	askAgain = false;
}
