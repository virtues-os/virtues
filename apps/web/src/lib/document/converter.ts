/**
 * The page's markdown converter, in the page: the crate's own
 * (`convert_for_editor` in crates/virtues-document), compiled to WebAssembly
 * from the workspace's source and Cargo.lock (crates/virtues-document-wasm,
 * built by scripts/build-converter.mjs). A paste, a drop and the inline
 * writer's end convert through it synchronously, inside the event, so what
 * they make is in the page before anything else can happen to it. There is
 * no second markdown parser in TypeScript, and the server is not asked: a
 * paste waits on nothing.
 *
 * The module is fetched and compiled once (`loadConverter`), early: on app
 * idle (`prefetchConverter`) and when a block page opens, and the editor
 * mounts once it has loaded, failed, or taken three seconds
 * (`converterSettled`). Until then, or where the module cannot run (an older
 * app's content security policy, Lockdown Mode), a conversion answers
 * `unavailable` and markdown is pasted as its text.
 *
 * A module's memory only grows. After a conversion leaves it past
 * `SWAP_ABOVE`, a fresh instance is made from the compiled module in the
 * background and takes over when ready; the old one serves until then. A
 * trap (a panic in Rust, or memory past the module's cap) drops the instance
 * the same way, and that conversion answers `unavailable`.
 */

import { readonly, writable } from 'svelte/store';
import type { DocumentNote } from '$lib/api/client';

export type ConvertResult =
	| { kind: 'converted'; html: string; notes: DocumentNote[] }
	/** More than one conversion takes; less at a time converts. */
	| { kind: 'too-large' }
	/** What the contract refuses in it (nested too deep, say). */
	| { kind: 'refused'; problems: DocumentNote[] }
	/** No converter here yet, or none that can run. */
	| { kind: 'unavailable' };

/**
 * How markdown is read: as a person's paste, as a writer's (the inline
 * writer), or as a writer's into an article, whose footnotes, figures and
 * veils are the article's own (`EditorMode` in the crate).
 */
export type Dialect = 'paste' | 'written' | 'article';

/** Each dialect as the module takes it: `EditorMode`'s number. */
const MODE: Record<Dialect, number> = { written: 0, paste: 1, article: 2 };

interface Exports {
	memory: WebAssembly.Memory;
	buffer_alloc(len: number): number;
	buffer_free(ptr: number, len: number): void;
	convert(ptr: number, len: number, mode: number): bigint;
	editor_max_bytes(): number;
}

/** How long the editor waits for the converter before it mounts without one. */
export const SETTLE_MS = 3000;

/** Past this much memory, a fresh instance takes over: what a large paste grew is given back. */
export const SWAP_ABOVE = 64 << 20;

let compiled: WebAssembly.Module | null = null;
let current: Exports | null = null;
let maxBytes = 0;
let loading: Promise<boolean> | null = null;
let swapping: Promise<void> | null = null;

const settled = writable(false);

/** Whether the converter has loaded, failed to, or taken `SETTLE_MS`: the editor mounts once it has. */
export const converterSettled = readonly(settled);

/** Exports from an instance of `module`, checked to be the converter's. */
function exportsOf(instance: WebAssembly.Instance): Exports {
	const e = instance.exports as unknown as Exports;
	for (const name of ['buffer_alloc', 'buffer_free', 'convert', 'editor_max_bytes'] as const) {
		if (typeof e[name] !== 'function') throw new Error(`the converter has no ${name}`);
	}
	if (!(e.memory instanceof WebAssembly.Memory)) throw new Error('the converter has no memory');
	return e;
}

function take(module: WebAssembly.Module, e: Exports): void {
	compiled = module;
	current = e;
	maxBytes = e.editor_max_bytes();
}

async function fetchModule(): Promise<WebAssembly.Module> {
	// A module of its own, so nothing but this load ever resolves the asset.
	const { default: url } = await import('./converter-url');
	if (typeof WebAssembly.compileStreaming === 'function') {
		try {
			return await WebAssembly.compileStreaming(fetch(url));
		} catch {
			// Served without `application/wasm` (a native app's own scheme): read it whole.
		}
	}
	const response = await fetch(url);
	if (!response.ok) throw new Error(`the converter did not load: ${response.status}`);
	return WebAssembly.compile(await response.arrayBuffer());
}

/**
 * Fetch, compile and start the converter, once; later calls share the
 * first. Resolves whether it can convert, and never rejects. A load that
 * failed (offline, say) is tried again by the next call, when a block page
 * opens; the editor does not wait for that one.
 */
export function loadConverter(): Promise<boolean> {
	if (loading) return loading;
	const timer = setTimeout(() => settled.set(true), SETTLE_MS);
	loading = (async () => {
		try {
			const module = await fetchModule();
			take(module, exportsOf(await WebAssembly.instantiate(module, {})));
			// The first conversion reads the contract and compiles its content
			// expressions: done here, not in someone's paste.
			convertNow('', 'paste');
			return true;
		} catch (e) {
			console.warn('[document] the markdown converter did not load; markdown pastes as text', e);
			loading = null;
			return false;
		} finally {
			clearTimeout(timer);
			settled.set(true);
		}
	})();
	return loading;
}

/** Start loading the converter when the app is idle, so a page rarely waits on it. */
export function prefetchConverter(): void {
	if (loading || typeof window === 'undefined') return;
	const idle = (window as { requestIdleCallback?: (f: () => void, o?: { timeout: number }) => number }).requestIdleCallback;
	if (idle) idle(() => void loadConverter(), { timeout: 10_000 });
	else setTimeout(() => void loadConverter(), 2000);
}

/**
 * Use `module` as the converter, synchronously: for tests, which read the
 * built module from disk, and nothing in the app.
 */
export function useConverterModule(module: WebAssembly.Module): void {
	take(module, exportsOf(new WebAssembly.Instance(module, {})));
	loading = Promise.resolve(true);
	settled.set(true);
}

/** How much memory the instance converting now holds, 0 with none: for tests, and nothing in the app. */
export function converterMemory(): number {
	return current?.memory.buffer.byteLength ?? 0;
}

/** A fresh instance of the compiled module, in the background, taking over when ready. */
function swap(): void {
	const module = compiled;
	if (swapping || !module) return;
	swapping = WebAssembly.instantiate(module, {})
		.then((instance) => take(module, exportsOf(instance)))
		.catch((e) => console.warn('[document] the markdown converter could not start again', e))
		.finally(() => {
			swapping = null;
		});
}

/** The converter's answer, as `convert_for_editor_json` writes it. */
function read(answer: string): ConvertResult {
	const parsed = JSON.parse(answer) as
		| { html: string; notes: DocumentNote[] }
		| { refused: 'too-large' }
		| { refused: 'problems'; problems: DocumentNote[] };
	if ('html' in parsed) return { kind: 'converted', html: parsed.html, notes: parsed.notes };
	if (parsed.refused === 'too-large') return { kind: 'too-large' };
	return { kind: 'refused', problems: parsed.problems };
}

/** The answer's JSON exactly as the module wrote it: for the test that holds it to the server's, byte for byte. */
export function convertRaw(markdown: string, as: Dialect): string | null {
	const e = current;
	if (!e) return null;
	const bytes = new TextEncoder().encode(markdown);
	let input = 0;
	try {
		input = e.buffer_alloc(bytes.length);
		new Uint8Array(e.memory.buffer, input, bytes.length).set(bytes);
		const packed = e.convert(input, bytes.length, MODE[as]);
		const at = Number(packed >> 32n);
		const length = Number(packed & 0xffffffffn);
		// Read after the call: the conversion may have grown the memory.
		const answer = new TextDecoder().decode(new Uint8Array(e.memory.buffer, at, length));
		e.buffer_free(at, length);
		e.buffer_free(input, bytes.length);
		if (e.memory.buffer.byteLength > SWAP_ABOVE) swap();
		return answer;
	} catch (err) {
		// A trap leaves the instance in no state to be called again.
		console.warn('[document] the markdown converter stopped; starting it again', err);
		if (current === e) current = null;
		swap();
		return null;
	}
}

/**
 * Markdown as the blocks a paste (`paste`), a writer's text (`written`) or a
 * writer's text into an article (`article`) makes, now. Answers
 * `unavailable` when there is no converter to run.
 */
export function convertNow(markdown: string, as: Dialect): ConvertResult {
	if (!current) return { kind: 'unavailable' };
	// UTF-8, which the crate counts, takes at least a byte for every UTF-16
	// unit: past the limit here is past it there, refused without copying
	// it in. Under it, the crate counts.
	if (markdown.length > maxBytes) return { kind: 'too-large' };
	const answer = convertRaw(markdown, as);
	return answer === null ? { kind: 'unavailable' } : read(answer);
}
