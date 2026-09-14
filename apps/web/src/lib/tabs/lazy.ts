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

/** Load the view, sharing one in-flight import per loader. */
export function loadView(loader: ViewLoader): Promise<View> {
	const ready = resolved.get(loader);
	if (ready) return Promise.resolve(ready);
	let pending = inflight.get(loader);
	if (!pending) {
		pending = loader().then((m) => {
			resolved.set(loader, m.default);
			inflight.delete(loader);
			return m.default;
		});
		inflight.set(loader, pending);
	}
	return pending;
}
