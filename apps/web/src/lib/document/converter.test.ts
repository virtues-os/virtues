/**
 * The page's markdown converter is the crate's, compiled to WebAssembly
 * (`converter.ts`, crates/virtues-document-wasm): one converter, never a
 * second. Held to the native build byte for byte here, over the corpus's
 * markdown (`tests/corpus/markdown.json`) and the markdown export of every
 * case in the contract's corpus (`tests/corpus/cases.json`), each read as a
 * paste and as a writer's. The module is built by vitest's global setup
 * (scripts/build-converter.mjs); a missing one fails. The native half is
 * `document-conformance convert`, named by DOCUMENT_CONFORMANCE_BIN, and
 * skips without it, as the other conformance cases do.
 *
 * Also how the module is run: a trap answers `unavailable` and a fresh
 * instance takes over, memory a large conversion grew is given back, and
 * the editor never waits on a load more than `SETTLE_MS`.
 */

import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { get } from 'svelte/store';
import { converterMemory, convertNow, convertRaw, SWAP_ABOVE, useConverterModule, type Dialect } from './converter';
import { CONVERTER_WASM, useRealConverter } from './test-utils';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../../../..');
const CORPUS = path.join(ROOT, 'crates/virtues-document/tests/corpus');
const bin = process.env.DOCUMENT_CONFORMANCE_BIN ? path.resolve(process.env.DOCUMENT_CONFORMANCE_BIN) : undefined;

/** Run the native binary over `input`, answering its JSON output. */
function native<T>(verb: string, input: unknown): T {
	const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'converter-'));
	try {
		const inPath = path.join(dir, 'in.json');
		const outPath = path.join(dir, 'out.json');
		fs.writeFileSync(inPath, JSON.stringify(input));
		execFileSync(bin!, [verb, inPath, outPath], { stdio: ['ignore', 'ignore', 'inherit'] });
		return JSON.parse(fs.readFileSync(outPath, 'utf8')) as T;
	} finally {
		fs.rmSync(dir, { recursive: true, force: true });
	}
}

type Case = { name: string; markdown: string; mode: Dialect };

/**
 * Every markdown input, in every dialect: the corpus's (an entry's `mode`
 * says which it was written for), and the export of each case the contract
 * accepts.
 */
function cases(): Case[] {
	const markdown = JSON.parse(fs.readFileSync(path.join(CORPUS, 'markdown.json'), 'utf8')) as {
		name: string;
		markdown: string;
		mode?: Dialect;
	}[];
	const html = JSON.parse(fs.readFileSync(path.join(CORPUS, 'cases.json'), 'utf8')) as { name: string; html: string }[];
	const exports = native<{ name: string; markdown?: string }[]>('batch', html)
		.filter((c): c is { name: string; markdown: string } => typeof c.markdown === 'string')
		.map((c) => ({ name: `export of ${c.name}`, markdown: c.markdown }));
	return [...markdown, ...exports].flatMap(({ name, markdown }) =>
		(['paste', 'written', 'article'] as const).map((mode) => ({ name, markdown, mode })),
	);
}

beforeAll(() => {
	expect(fs.existsSync(CONVERTER_WASM), `the converter is built at ${CONVERTER_WASM}`).toBe(true);
	useRealConverter();
});

describe('one converter', () => {
	it.skipIf(!bin)('the module answers every input as the native build does, byte for byte', () => {
		const all = cases();
		// The corpus's exports are most of it: a converter that drifted on real
		// pages' markdown would show here first.
		expect(all.length).toBeGreaterThan(200);
		const answers = native<{ name: string; answer: string }[]>('convert', all);
		const differ = all.flatMap((c, i) => {
			const here = convertRaw(c.markdown, c.mode);
			return here === answers[i].answer ? [] : [{ name: c.name, mode: c.mode, here, native: answers[i].answer }];
		});
		expect(differ).toEqual([]);
		// The article's syntax reached the module: its own nodes, in its mode only.
		const article = all.filter((c) => c.name.startsWith('article:'));
		expect(article.length).toBeGreaterThan(0);
		for (const c of article) {
			const answer = convertRaw(c.markdown, c.mode) ?? '';
			const own = /virtues-(footnote|figure|veil)/.test(answer);
			if (c.mode === 'article' && !c.name.includes('not a footnote')) expect(own, c.name).toBe(true);
			if (c.mode !== 'article') expect(own, `${c.name} as ${c.mode}`).toBe(false);
		}
	});

	it('reads an article as one only in the article mode', () => {
		const md = 'Lunch with [⟦Nick⟧](/person/person_1).[^ev-1]\n\n[^ev-1]: Message · 12:10 PM\n';
		expect(convertNow(md, 'article')).toEqual({
			kind: 'converted',
			html:
				'<p>Lunch with <a href="/person/person_1"><virtues-veil>Nick</virtues-veil></a>.' +
				'<virtues-footnote label="ev-1" note="Message · 12:10 PM"></virtues-footnote></p>',
			notes: [],
		});
		const written = convertNow(md, 'written');
		expect(written.kind === 'converted' && written.html).not.toMatch(/virtues-(footnote|veil)/);
	});

	it('reads markdown as the crate does: a paste keeps its text, a writer’s suggestion comes in accepted', () => {
		expect(convertNow('## Plan\n\n- [ ] Book seats\n', 'paste')).toEqual({
			kind: 'converted',
			html: '<h2>Plan</h2><ul data-type="taskList"><li data-type="taskItem"><p>Book seats</p></li></ul>',
			notes: [],
		});
		expect(convertNow('- Draft {--old--} with <Login/>\n', 'paste')).toMatchObject({
			kind: 'converted',
			html: '<ul><li><p>Draft {--old--} with &lt;Login/&gt;</p></li></ul>',
		});
		expect(convertNow('The {--quick--}{++slow++} fox.\n', 'written')).toMatchObject({ kind: 'converted', html: '<p>The slow fox.</p>' });
	});

	it('refuses more than one conversion takes, and what the contract refuses, by kind', () => {
		expect(convertNow('a'.repeat((256 << 10) + 1), 'paste')).toEqual({ kind: 'too-large' });
		// Under the size limit, but more nodes than a page holds.
		const rows = '| a | b |\n| - | - |\n' + '| x | y |\n'.repeat(20_000);
		expect(convertNow(rows, 'paste')).toEqual({ kind: 'too-large' });
		expect(convertNow(`${'> '.repeat(150)}Deep.\n`, 'paste')).toMatchObject({ kind: 'refused' });
	});
});

/** A WebAssembly module with the converter's exports, built byte by byte. */
function stubModule(convertBody: number[], answer: string): WebAssembly.Module {
	const uleb = (n: number): number[] => {
		const out: number[] = [];
		do {
			let b = n & 0x7f;
			n >>>= 7;
			if (n) b |= 0x80;
			out.push(b);
		} while (n);
		return out;
	};
	const sleb = (n: bigint): number[] => {
		const out: number[] = [];
		for (;;) {
			const b = Number(n & 0x7fn);
			n >>= 7n;
			if ((n === 0n && !(b & 0x40)) || (n === -1n && b & 0x40)) return [...out, b];
			out.push(b | 0x80);
		}
	};
	const bytes = (s: string) => [...new TextEncoder().encode(s)];
	const name = (s: string) => [...uleb(s.length), ...bytes(s)];
	const vec = (items: number[][]) => [...uleb(items.length), ...items.flat()];
	const section = (id: number, body: number[]) => [id, ...uleb(body.length), ...body];
	const [I32, I64] = [0x7f, 0x7e];
	const at = 2048;
	const data = bytes(answer);
	const packed = (BigInt(at) << 32n) | BigInt(data.length);
	const body = (code: number[]) => [...uleb(code.length + 1), 0x00, ...code];
	const module = [
		...[0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00],
		...section(1, vec([
			[0x60, 1, I32, 1, I32],
			[0x60, 2, I32, I32, 0],
			[0x60, 3, I32, I32, I32, 1, I64],
			[0x60, 0, 1, I32],
		])),
		...section(3, vec([[0], [1], [2], [3]])),
		...section(5, vec([[0x00, 1]])),
		// One mutable i32, 0 in every fresh instance.
		...section(6, vec([[I32, 1, 0x41, 0, 0x0b]])),
		...section(7, vec([
			[...name('memory'), 2, 0],
			[...name('buffer_alloc'), 0, 0],
			[...name('buffer_free'), 0, 1],
			[...name('convert'), 0, 2],
			[...name('editor_max_bytes'), 0, 3],
		])),
		...section(10, vec([
			body([0x41, ...sleb(1024n), 0x0b]),
			body([0x0b]),
			body([...convertBody, 0x42, ...sleb(packed), 0x0b]),
			body([0x41, ...sleb(BigInt(256 << 10)), 0x0b]),
		])),
		...section(11, vec([[0x00, 0x41, ...sleb(BigInt(at)), 0x0b, ...uleb(data.length), ...data]])),
	];
	return new WebAssembly.Module(new Uint8Array(module));
}

describe('running the module', () => {
	afterEach(() => {
		useRealConverter();
		vi.restoreAllMocks();
	});

	it('a trap answers unavailable, and a fresh instance of the module serves the next call', async () => {
		vi.spyOn(console, 'warn').mockImplementation(() => {});
		// `convert` traps on an instance that has converted once: its global is set.
		const once = [0x23, 0, 0x04, 0x40, 0x00, 0x0b, 0x41, 1, 0x24, 0];
		useConverterModule(stubModule(once, '{"html":"<p>ok</p>","notes":[]}'));
		expect(convertNow('# one', 'paste')).toEqual({ kind: 'converted', html: '<p>ok</p>', notes: [] });
		expect(convertNow('# two', 'paste')).toEqual({ kind: 'unavailable' });
		await vi.waitFor(() => expect(converterMemory()).toBeGreaterThan(0));
		expect(convertNow('# three', 'paste')).toEqual({ kind: 'converted', html: '<p>ok</p>', notes: [] });
	});

	it('a conversion that leaves the memory past SWAP_ABOVE hands over to a fresh instance, which gives it back', async () => {
		const start = converterMemory();
		expect(start).toBeGreaterThan(0);
		expect(start).toBeLessThan(SWAP_ABOVE);
		// Past what one conversion takes, so only the copy in grows the memory.
		expect(convertRaw('a'.repeat(SWAP_ABOVE + (1 << 20)), 'paste')).toBe('{"refused":"too-large"}');
		await vi.waitFor(() => expect(converterMemory()).toBeLessThan(SWAP_ABOVE));
		expect(convertNow('## Plan', 'paste')).toMatchObject({ kind: 'converted', html: '<h2>Plan</h2>' });
	});
});

describe('loading the module', () => {
	// Each test loads a converter module of its own, nothing loaded yet.
	beforeEach(() => vi.resetModules());
	afterEach(() => {
		vi.useRealTimers();
		vi.unstubAllGlobals();
		vi.restoreAllMocks();
	});

	it('the editor waits at most SETTLE_MS for a load that never ends, and markdown then pastes as text', async () => {
		vi.useFakeTimers();
		vi.stubGlobal('fetch', vi.fn(() => new Promise<Response>(() => {})));
		const c = await import('./converter');
		void c.loadConverter();
		expect(get(c.converterSettled)).toBe(false);
		await vi.advanceTimersByTimeAsync(c.SETTLE_MS - 1);
		expect(get(c.converterSettled)).toBe(false);
		await vi.advanceTimersByTimeAsync(1);
		expect(get(c.converterSettled)).toBe(true);
		expect(c.convertNow('## Plan', 'paste')).toEqual({ kind: 'unavailable' });
	});

	it('a load that fails settles at once, never rejects, pastes markdown as text, and is tried again next time', async () => {
		vi.spyOn(console, 'warn').mockImplementation(() => {});
		vi.stubGlobal('fetch', vi.fn(async () => new Response('', { status: 404 })));
		const c = await import('./converter');
		await expect(c.loadConverter()).resolves.toBe(false);
		expect(get(c.converterSettled)).toBe(true);
		expect(c.convertNow('## Plan', 'paste')).toEqual({ kind: 'unavailable' });
		// The next block page opened tries again, and this time it loads.
		const wasm = fs.readFileSync(CONVERTER_WASM);
		vi.stubGlobal('fetch', vi.fn(async () => new Response(wasm, { headers: { 'Content-Type': 'application/wasm' } })));
		await expect(c.loadConverter()).resolves.toBe(true);
		expect(c.convertNow('## Plan', 'paste')).toMatchObject({ kind: 'converted', html: '<h2>Plan</h2>' });
	});

	it('loads the built module from its address, once, and converts with it', async () => {
		const wasm = fs.readFileSync(CONVERTER_WASM);
		const fetch = vi.fn(async () => new Response(wasm, { headers: { 'Content-Type': 'application/wasm' } }));
		vi.stubGlobal('fetch', fetch);
		const c = await import('./converter');
		const [first, second] = await Promise.all([c.loadConverter(), c.loadConverter()]);
		expect([first, second]).toEqual([true, true]);
		expect(fetch).toHaveBeenCalledOnce();
		expect(get(c.converterSettled)).toBe(true);
		expect(c.convertNow('## Plan', 'paste')).toMatchObject({ kind: 'converted', html: '<h2>Plan</h2>' });
	});
});

describe('where the module runs', () => {
	/** `script-src` of a policy, its sources. */
	const scriptSrc = (policy: string) =>
		policy
			.split(';')
			.map((d) => d.trim())
			.find((d) => d.startsWith('script-src '))
			?.split(/\s+/)
			.slice(1);

	// Without 'wasm-unsafe-eval' a browser refuses to compile any
	// WebAssembly: the converter would never load, and markdown would paste
	// as text. Shiki's default engine (Oniguruma) is WebAssembly too.
	it("every policy the app runs under compiles WebAssembly: the box's and the apps'", () => {
		for (const conf of ['tauri.conf.json', 'tauri.ios.conf.json']) {
			const json = JSON.parse(fs.readFileSync(path.join(ROOT, 'apps/web/src-tauri', conf), 'utf8'));
			const policy: string = json.app.security.csp;
			expect(scriptSrc(policy), conf).toContain("'wasm-unsafe-eval'");
			// The picture a device shows of an image it is uploading is the file itself, a `blob:`.
			expect(policy.split(';').find((d) => d.trim().startsWith('img-src')), conf).toContain('blob:');
		}
		const box = fs.readFileSync(path.join(ROOT, 'virtues-core/src/middleware/security.rs'), 'utf8');
		expect(box).toMatch(/script-src 'self' 'wasm-unsafe-eval'; \\/);
	});
});
