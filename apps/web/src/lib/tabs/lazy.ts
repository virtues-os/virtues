/**
 * Lazy tab views.
 *
 * The registry used to import every view statically, so the chat route paid
 * for the wiki, pages, sources, applets, settings and storage views on first
 * paint — about 900 KB of app code, plus whatever those views pull in — before
 * a single message could be typed. A view is now a loader: the chunk arrives
 * the first time a tab of that kind opens, and the browser caches it forever
 * after (content-hashed path, immutable cache header from the box).
 *
 * The two views a session opens on, chat and home, stay eager through
 * `eager()` so a cold start never flashes an empty pane waiting on a chunk.
 */

import type { Component } from 'svelte';
import { deadline } from '$lib/api/client';

// biome-ignore lint/suspicious/noExplicitAny: view props vary by tab type
export type View = Component<any>;
export type ViewLoader = () => Promise<{ default: View }>;

const resolved = new WeakMap<ViewLoader, View>();
const inflight = new WeakMap<ViewLoader, Promise<View>>();

/** Wrap a statically imported view so it satisfies the loader contract and
 *  resolves synchronously through `peekView`. */
export function eager(view: View): ViewLoader {
	const loader: ViewLoader = () => Promise.resolve({ default: view });
	resolved.set(loader, view);
	return loader;
}

/** The view if its chunk has already arrived, else undefined. */
export function peekView(loader: ViewLoader): View | undefined {
	return resolved.get(loader);
}

/** Load the view, sharing one in-flight import per loader.
 *
 *  A failed import is forgotten, not kept. The usual failure is a box upgrade
 *  under an open app: each release ships only its own content-hashed chunks,
 *  so a view this page never opened now points at a file that is gone. Kept,
 *  the rejection was handed to every later open of that view, which stayed
 *  blank until a full reload nobody knew to do. The same happens when the Mac
 *  or phone app swaps in a new copy of the web app. */
export function loadView(loader: ViewLoader): Promise<View> {
	const ready = resolved.get(loader);
	if (ready) return Promise.resolve(ready);
	let pending = inflight.get(loader);
	if (!pending) {
		pending = loader().then(
			(m) => {
				resolved.set(loader, m.default);
				inflight.delete(loader);
				return m.default;
			},
			(err) => {
				inflight.delete(loader);
				throw err;
			},
		);
		inflight.set(loader, pending);
	}
	return pending;
}

const RELOAD_KEY = 'virtues-stale-chunk-reload';
/** One automatic reload per minute at most, so a chunk that is missing for
 *  some other reason (a box mid-restart, a broken build) shows the error
 *  instead of reloading forever. */
const RELOAD_GAP_MS = 60_000;

/**
 * Reload the page to pick up the current build, after a chunk failed to load.
 * Resolves false, and reloads nothing, when this tab already reloaded for the
 * same reason within the last minute, or when the app's own files can't be
 * reached right now: in a browser whose server is down, a reload trades a
 * view that didn't load for the browser's "can't connect" page. The caller
 * then shows its error instead.
 *
 * Chat drafts and page edits are kept on the device (drafts.ts, IndexedDB),
 * so a reload here costs a moment, not work.
 */
export async function reloadForStaleChunk(): Promise<boolean> {
	if (typeof window === 'undefined') return false;
	try {
		const last = Number(sessionStorage.getItem(RELOAD_KEY) ?? 0);
		if (Date.now() - last < RELOAD_GAP_MS) return false;
	} catch {
		// No session storage, no way to tell a first try from a loop.
		return false;
	}
	// SvelteKit's build stamp is served wherever the app's files are: the
	// box in a browser, the app's own copy on the Mac and phone.
	try {
		const res = await fetch('/_app/version.json', {
			cache: 'no-store',
			signal: deadline(5000),
		});
		if (!res.ok) return false;
	} catch {
		return false;
	}
	try {
		sessionStorage.setItem(RELOAD_KEY, String(Date.now()));
	} catch {
		return false;
	}
	window.location.reload();
	return true;
}
