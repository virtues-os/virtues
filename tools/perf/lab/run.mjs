#!/usr/bin/env node
// The performance lab: one command, the same numbers for the same code.
//
// It builds the web UI from a commit (HEAD by default) in a scratch clone,
// starts a box on its own scratch database with invented data (seed.mjs),
// serves that build from the box the way an installed box does, and drives a
// headless Chromium through fixed scenarios. Each scenario runs in a fresh
// browser context (a cold cache), several times, and reports the median.
// Memory is read only after forcing garbage collection, through the DevTools
// protocol, so it measures what is held rather than what has not been
// collected yet.
//
// Headless Chromium keeps rendering frames, which the app's own transitions
// need: in a hidden or background tab, Svelte's exit transitions never finish
// and every closed element stays in the page, which reads as a leak that is
// not there. Run this, not a hidden browser pane.
//
//     node tools/perf/lab/run.mjs                 measure and compare to budgets.json
//     node tools/perf/lab/run.mjs --update        write budgets.json from this run
//     node tools/perf/lab/run.mjs --web-build DIR reuse a built apps/web/build
//     node tools/perf/lab/run.mjs --ref <git ref> build that commit instead of HEAD
//     node tools/perf/lab/run.mjs --runs 3 --keep (keep the box and database up)
//
// The box binary is the one in the shared target (`cargo build -p virtues`),
// so the server half is a debug build of this checkout: compare its timings
// only with each other. See agents/build/performance.md.

import { spawn, execFileSync } from 'node:child_process';
import { createServer, request as httpRequest, Agent } from 'node:http';
import { existsSync, mkdtempSync, readdirSync, readFileSync, writeFileSync, mkdirSync, symlinkSync, rmSync, openSync, closeSync } from 'node:fs';
import { tmpdir, homedir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium, webkit } from 'playwright-core';
import { seedSql, LONG_CHAT_ID, LONG_CHAT_MESSAGES } from './seed.mjs';
import { startFakeModel, TOKENS, TOKEN_INTERVAL_MS } from './fake-model.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '../../..');
const argv = process.argv.slice(2);
const flag = (name) => argv.includes(name);
const opt = (name, dflt) => {
	const i = argv.indexOf(name);
	return i >= 0 ? argv[i + 1] : dflt;
};

const PORT = Number(opt('--port', 8191));
const RUNS = Number(opt('--runs', 5));
const ORIGIN = `http://127.0.0.1:${PORT}`;
const DB = 'virtues_perf_lab';
const ADMIN_URL = process.env.PERF_ADMIN_DATABASE_URL ?? 'postgres://virtues:virtues@localhost:5432/postgres';
const DB_URL = ADMIN_URL.replace(/\/[^/]*$/, `/${DB}`);
const SERVER_BIN = opt('--server-bin', join(homedir(), '.cargo/shared-target/debug/virtues'));
const BUDGETS = join(here, 'budgets.json');
const RESULTS = join(here, 'results');

// The views a person moves between, opened in a loop for the memory scenario.
const VIEWS = ['/pages', '/wiki', '/settings', '/tasks', '/projects', '/applets', `/chat/${LONG_CHAT_ID}`, '/chat'];
const LAPS = 10;

const log = (...a) => console.log('[lab]', ...a);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

function chromiumPath() {
	if (process.env.CHROMIUM_PATH) return process.env.CHROMIUM_PATH;
	const cache = join(homedir(), 'Library/Caches/ms-playwright');
	const shells = existsSync(cache) ? readdirSync(cache).filter((d) => d.startsWith('chromium_headless_shell-')).sort() : [];
	for (const d of shells.reverse()) {
		const p = join(cache, d, 'chrome-headless-shell-mac-arm64/chrome-headless-shell');
		if (existsSync(p)) return p;
	}
	throw new Error('no headless Chromium found. Set CHROMIUM_PATH, or run `npx playwright-core install chromium-headless-shell`.');
}

/**
 * Build apps/web from a commit in a scratch clone. The source is the commit;
 * dependencies are this checkout's installed node_modules, linked in, so a
 * --ref far from HEAD can build against the wrong versions. Returns the build
 * and the clone to remove afterwards (none for --web-build).
 */
function buildWeb() {
	const given = opt('--web-build');
	if (given) return { webBuild: resolve(given), clone: null };
	const ref = opt('--ref', 'HEAD');
	const dir = mkdtempSync(join(tmpdir(), 'virtues-perf-'));
	log(`building ${ref} in ${dir}`);
	execFileSync('git', ['clone', '--quiet', '--local', '--no-checkout', repo, dir]);
	execFileSync('git', ['-C', dir, 'checkout', '--quiet', execFileSync('git', ['-C', repo, 'rev-parse', ref]).toString().trim()]);
	symlinkSync(join(repo, 'apps/web/node_modules'), join(dir, 'apps/web/node_modules'));
	execFileSync('pnpm', ['build'], { cwd: join(dir, 'apps/web'), stdio: ['ignore', 'ignore', 'inherit'] });
	return { webBuild: join(dir, 'apps/web/build'), clone: dir };
}

/**
 * One lab run at a time on this machine. A second run would drop the first
 * one's database mid-measurement (the port, the database and results/ are
 * fixed), so it refuses while the first is alive.
 */
const LOCK = join(RESULTS, 'lab.lock');
function takeLock() {
	mkdirSync(RESULTS, { recursive: true });
	if (existsSync(LOCK)) {
		const pid = Number(readFileSync(LOCK, 'utf8'));
		let alive = false;
		try {
			process.kill(pid, 0);
			alive = true;
		} catch {
			/* gone: a run that crashed left the file */
		}
		if (alive && pid !== process.pid) throw new Error(`another lab run (pid ${pid}) is going; wait for it, or stop it`);
	}
	writeFileSync(LOCK, String(process.pid));
}
function releaseLock() {
	try {
		if (readFileSync(LOCK, 'utf8') === String(process.pid)) rmSync(LOCK);
	} catch {
		/* already gone */
	}
}

function psql(url, sql) {
	return execFileSync('psql', [url, '-v', 'ON_ERROR_STOP=1', '-q', '-c', sql]).toString();
}

async function startBox(webBuild, modelUrl) {
	psql(ADMIN_URL, `DROP DATABASE IF EXISTS ${DB} WITH (FORCE)`);
	psql(ADMIN_URL, `CREATE DATABASE ${DB}`);
	const state = mkdtempSync(join(tmpdir(), 'virtues-perf-state-'));
	// Only what the box needs from this shell. The rest of the environment,
	// and any .env the box would read beside its working directory, could
	// carry real keys or a relay identity into a scratch box.
	const env = {
		PATH: process.env.PATH,
		HOME: process.env.HOME,
		TMPDIR: process.env.TMPDIR,
		DATABASE_URL: DB_URL,
		NOMAD_PORT_http: String(PORT),
		ENVIRONMENT: 'dev',
		VIRTUES_DEV_SKIP_SETUP: '1',
		VIRTUES_HTTPS_PORT: '0',
		VIRTUES_PUBLIC_ORIGINS: `${ORIGIN},http://localhost:${PORT}`,
		// The model is fake-model.mjs: a fixed reply at a fixed pace, so a
		// streamed turn is the same every run and no real model is asked.
		VIRTUES_API_URL: modelUrl,
		VIRTUES_API_KEY: 'dev-local-key',
		STATIC_DIR: webBuild,
		STORAGE_PATH: join(state, 'storage'),
		VIRTUES_APPLET_STATE_DIR: join(state, 'applets'),
		RUST_LOG: process.env.PERF_RUST_LOG ?? 'warn'
	};
	// Loopback only: with setup skipped and ENVIRONMENT=dev, a box on every
	// interface would be the owner's to anyone on the network. Its log goes
	// straight to results/box.log, a file the box keeps writing after this
	// runner exits (--keep), never a pipe that closes under it.
	mkdirSync(RESULTS, { recursive: true });
	const logPath = join(RESULTS, 'box.log');
	const logFd = openSync(logPath, 'w');
	const box = spawn(SERVER_BIN, ['server', '--host', '127.0.0.1'], { env, cwd: state, stdio: ['ignore', 'ignore', logFd] });
	closeSync(logFd);
	const tail = () => {
		try {
			return readFileSync(logPath, 'utf8').slice(-4000);
		} catch {
			return '';
		}
	};
	for (let i = 0; i < 120; i++) {
		if (box.exitCode !== null) throw new Error(`box exited early:\n${tail()}`);
		try {
			if ((await fetch(`${ORIGIN}/health`)).ok) break;
		} catch {
			/* not up yet */
		}
		await sleep(500);
		if (i === 119) throw new Error(`box never answered /health:\n${tail()}`);
	}
	// A box from an earlier run still on this port would answer /health too.
	if (box.exitCode !== null) throw new Error(`box exited early (is port ${PORT} taken?):\n${tail()}`);
	// Migrations have run by the time /health answers.
	const seedFile = join(state, 'seed.sql');
	writeFileSync(seedFile, seedSql());
	execFileSync('psql', [DB_URL, '-v', 'ON_ERROR_STOP=1', '-q', '-f', seedFile]);
	return { box, state };
}

/** Counters from Chromium itself, after collecting garbage twice. */
async function settledMetrics(cdp, page) {
	await cdp.send('HeapProfiler.collectGarbage');
	await cdp.send('HeapProfiler.collectGarbage');
	const { metrics } = await cdp.send('Performance.getMetrics');
	const m = Object.fromEntries(metrics.map((x) => [x.name, x.value]));
	// heap_mb is the JavaScript heap alone. memory_mb adds the DOM's own memory
	// and ArrayBuffer storage, as Chromium counts them. Neither counted Shiki's
	// WebAssembly engine's 22 MB, so WebAssembly memory is wasm_mb, read from
	// the modules themselves. It is kept apart, not added: Chromium's
	// ArrayBuffer count may include some modules' memory, and adding would
	// count those twice.
	const usage = await cdp.send('Runtime.getHeapUsage');
	const wasm = page
		? await page.evaluate(() => [...(window.__wasmMemories ?? [])].reduce((a, m) => a + m.buffer.byteLength, 0)).catch(() => 0)
		: 0;
	const memory = usage.usedSize + (usage.embedderHeapUsedSize ?? 0) + (usage.backingStorageSize ?? 0);
	return {
		heap_mb: +(m.JSHeapUsedSize / 1e6).toFixed(2),
		memory_mb: +(memory / 1e6).toFixed(2),
		wasm_mb: +(wasm / 1e6).toFixed(2),
		dom_nodes: m.Nodes,
		listeners: m.JSEventListeners,
		documents: m.Documents
	};
}

async function newPage(browser) {
	const context = await browser.newContext({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 1 });
	const page = await context.newPage();
	const cdp = await context.newCDPSession(page);
	await cdp.send('Performance.enable');
	// WebAssembly memory, which no cheap Chromium counter includes: every
	// memory a module exports or is given, kept so its size can be read later.
	await page.addInitScript(() => {
		const memories = (window.__wasmMemories = new Set());
		const keep = (...objs) => {
			for (const o of objs) {
				if (!o || typeof o !== 'object') continue;
				for (const v of Object.values(o)) {
					if (v instanceof WebAssembly.Memory) memories.add(v);
					else if (v && typeof v === 'object' && !(v instanceof WebAssembly.Module)) {
						for (const w of Object.values(v)) if (w instanceof WebAssembly.Memory) memories.add(w);
					}
				}
			}
		};
		const instantiate = WebAssembly.instantiate;
		WebAssembly.instantiate = async function (source, imports) {
			const r = await instantiate.call(this, source, imports);
			keep(imports, (r.instance ?? r).exports);
			return r;
		};
		const streaming = WebAssembly.instantiateStreaming;
		if (streaming) {
			WebAssembly.instantiateStreaming = async function (source, imports) {
				const r = await streaming.call(this, source, imports);
				keep(imports, r.instance.exports);
				return r;
			};
		}
		const Instance = WebAssembly.Instance;
		WebAssembly.Instance = function (module, imports) {
			const i = new Instance(module, imports);
			keep(imports, i.exports);
			return i;
		};
		WebAssembly.Instance.prototype = Instance.prototype;
	});
	// Long tasks, from the first byte of the document.
	await page.addInitScript(() => {
		window.__longTasks = [];
		try {
			new PerformanceObserver((l) => {
				for (const e of l.getEntries()) window.__longTasks.push(e.duration);
			}).observe({ type: 'longtask', buffered: true });
		} catch {
			/* no longtask support */
		}
	});
	const api = [];
	let jsBytes = 0;
	page.on('requestfinished', async (req) => {
		const url = new URL(req.url());
		const timing = req.timing();
		if (url.pathname.startsWith('/api/') || url.pathname.startsWith('/auth/') || url.pathname === '/health') {
			// `startTime` is epoch milliseconds; the rest are offsets from it.
			api.push({ path: url.pathname + url.search, start: timing.startTime + timing.requestStart, end: timing.startTime + timing.responseEnd });
		}
		if (req.resourceType() === 'script') {
			const sizes = await req.sizes().catch(() => null);
			if (sizes) jsBytes += sizes.responseBodySize + sizes.responseHeadersSize;
		}
	});
	return { context, page, cdp, api, jsBytes: () => jsBytes };
}

/**
 * The waterfall's depth: the longest chain of API requests where each started
 * after the one before it finished. An upper bound on how many round trips
 * stand between the page and its data, since a request that merely started
 * late counts as waiting. On loopback a round trip is nearly free; over the
 * relay to a phone each one is tens to hundreds of milliseconds.
 */
function waterfallDepth(reqs) {
	const sorted = [...reqs].sort((a, b) => a.start - b.start);
	const depth = new Map();
	let max = 0;
	for (const r of sorted) {
		let d = 1;
		for (const p of sorted) if (p !== r && p.end <= r.start) d = Math.max(d, (depth.get(p) ?? 1) + 1);
		depth.set(r, d);
		max = Math.max(max, d);
	}
	return max;
}

/** Wait for `locator`, and on a timeout save what the page showed, so a failure explains itself. */
async function waitOrExplain(page, locator, state, timeout) {
	try {
		await locator.waitFor({ state, timeout });
	} catch (e) {
		mkdirSync(RESULTS, { recursive: true });
		const shot = join(RESULTS, `failure-${Date.now()}.png`);
		await page.screenshot({ path: shot }).catch(() => {});
		const text = await page.evaluate(() => document.body?.innerText.slice(0, 300)).catch(() => '');
		throw new Error(`${e.message.split('\n')[0]}\n  at ${page.url()}\n  page text: ${JSON.stringify(text)}\n  screenshot: ${shot}`);
	}
}

async function coldLoad(browser) {
	const { context, page, cdp, api, jsBytes } = await newPage(browser);
	const t0 = Date.now();
	await page.goto(`${ORIGIN}/chat`);
	await waitOrExplain(page, page.locator('.chat-input').first(), 'visible', 30_000);
	const readyAt = Date.now();
	const readyMs = readyAt - t0;
	const before = api.filter((r) => r.end <= readyAt);
	await page.waitForLoadState('networkidle');
	const longTasks = await page.evaluate(() => window.__longTasks);
	const m = await settledMetrics(cdp, page);
	await context.close();
	return {
		ready_ms: readyMs,
		api_before_ready: before.length,
		waterfall_depth: waterfallDepth(before),
		api_total: api.length,
		js_transfer_kb: +(jsBytes() / 1024).toFixed(1),
		long_task_ms: Math.round(longTasks.reduce((a, b) => a + b, 0)),
		...m
	};
}

/** Chromium's running total of layout and style work, in milliseconds. */
async function layoutWorkMs(cdp) {
	const { metrics } = await cdp.send('Performance.getMetrics');
	const m = Object.fromEntries(metrics.map((x) => [x.name, x.value]));
	return (m.LayoutDuration + m.RecalcStyleDuration) * 1000;
}

const twoFrames = (page) => page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));

/**
 * A cold load as the Mac and iPhone apps see it: the UI loads at once, and
 * every request to the box goes through a proxy that holds it RELAY_MS before
 * forwarding it, about a round trip over the relay. The page reaches the
 * proxy the way the apps reach their loopback: the shell's injected
 * `__VIRTUES_BACKEND_ORIGIN__`, which the app's fetch shim
 * (lib/config/backend.ts) sends every box path to, CORS preflights included. The browser opens at most six connections to the
 * proxy, as to the apps' loopback, and a held request keeps its connection,
 * so requests beyond six wait their turn.
 */
const RELAY_MS = 100;
let relayUrl = '';
/** CORS preflights the proxy saw during the current relay_load run. */
let relayPreflights = 0;
/** The fake model, for the stream scenario to read when it sent the last token. */
let fakeModel = null;
async function startRelayProxy() {
	const box = new URL(ORIGIN);
	const agent = new Agent({ keepAlive: true, maxSockets: 64 });
	const server = createServer((req, res) => {
		if (req.method === 'OPTIONS') relayPreflights++;
		setTimeout(() => {
			const up = httpRequest(
				{ host: box.hostname, port: box.port, method: req.method, path: req.url, headers: { ...req.headers, host: box.host }, agent },
				(r) => {
					res.writeHead(r.statusCode ?? 502, r.headers);
					r.pipe(res);
				}
			);
			up.on('error', () => {
				if (!res.headersSent) res.writeHead(502);
				res.end();
			});
			req.pipe(up);
		}, RELAY_MS);
	});
	server.keepAliveTimeout = 60_000;
	await new Promise((r) => server.listen(0, '127.0.0.1', r));
	relayUrl = `http://127.0.0.1:${server.address().port}`;
	return server;
}
async function relayLoad(browser) {
	const { context, page } = await newPage(browser);
	await page.addInitScript((url) => {
		window.__VIRTUES_BACKEND_ORIGIN__ = url;
	}, relayUrl);
	relayPreflights = 0;
	await page.goto(`${ORIGIN}/chat`, { waitUntil: 'commit' });
	// Times from the page's own clock, so they start at navigation.
	const marks = await page.evaluate(
		(title) =>
			new Promise((done) => {
				const out = {};
				const check = () => {
					const now = Math.round(performance.now());
					const input = document.querySelector('.chat-input');
					if (!out.composer && input && input.getBoundingClientRect().height > 0) out.composer = now;
					if (!out.sidebar && [...document.querySelectorAll('aside *')].some((el) => el.childElementCount === 0 && el.textContent === title)) out.sidebar = now;
					if (out.composer && out.sidebar) return done(out);
					if (now > 30_000) return done(out);
					requestAnimationFrame(check);
				};
				check();
			}),
		'A long planning thread'
	);
	// Preflights before the sidebar showed: each is a relay round trip ahead
	// of the request it guards.
	const preflights = relayPreflights;
	await context.close();
	return { composer_ms: marks.composer ?? -1, sidebar_ms: marks.sidebar ?? -1, preflights };
}

/**
 * The long chat scrolled from its newest message to its first with the mouse
 * wheel, in WebKit, the engine of the Mac and iPhone apps. After each step the
 * row that was at the top must have moved down by exactly what the wheel
 * scrolled, including the steps that draw earlier messages above it. A step
 * where it moved more than 40 px off that is a jump the reader sees. WebKit
 * jumped at almost every step while rows were skipped off screen with
 * `content-visibility: auto`; Chromium did not, so this runs in WebKit.
 */
const WHEEL_STEP = 300;
async function webkitScroll() {
	let browser;
	try {
		browser = await webkit.launch({ headless: true });
	} catch (e) {
		throw new Error(`webkit_scroll needs Playwright's WebKit: run \`npx playwright-core install webkit\` in tools/perf/lab (${e.message.split('\n')[0]})`);
	}
	try {
		const context = await browser.newContext({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 1 });
		const page = await context.newPage();
		await page.goto(`${ORIGIN}/chat/${LONG_CHAT_ID}`);
		const last = `${LONG_CHAT_ID}_${String(LONG_CHAT_MESSAGES - 1).padStart(4, '0')}`;
		await waitOrExplain(page, page.locator(`[data-message-id="${last}"]`), 'attached', 60_000);
		await page.waitForTimeout(1000);
		const box = await page.locator('.chat-layout').boundingBox();
		await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
		const state = () =>
			page.evaluate(() => {
				const s = document.querySelector('.chat-layout');
				const top = s.getBoundingClientRect().top;
				const rows = [...s.querySelectorAll('[data-message-id]')];
				const ref = rows.find((r) => r.getBoundingClientRect().bottom > top + 20);
				return { scrollTop: s.scrollTop, drawn: rows.length, ref: ref?.dataset.messageId, refTop: ref?.getBoundingClientRect().top ?? 0, first: rows[0]?.dataset.messageId };
			});
		// The first seeded message, not a count: stream_reply's turn may still
		// be at the end of the chat.
		const firstId = `${LONG_CHAT_ID}_0000`;
		let prev = await state();
		let jumps = 0;
		let worst = 0;
		let reachedTop = false;
		for (let i = 0; i < 500 && !reachedTop; i++) {
			await page.mouse.wheel(0, -WHEEL_STEP);
			await page.waitForTimeout(120);
			const now = await state();
			const refNow = await page.evaluate((id) => document.querySelector(`[data-message-id="${id}"]`)?.getBoundingClientRect().top ?? null, prev.ref);
			// Near the top the wheel scrolls only what is left, unless rows were
			// just drawn above.
			const expected = now.drawn !== prev.drawn ? WHEEL_STEP : Math.min(WHEEL_STEP, prev.scrollTop);
			const off = refNow === null ? Infinity : Math.abs(refNow - prev.refTop - expected);
			if (off > 40) jumps++;
			if (Number.isFinite(off)) worst = Math.max(worst, off);
			// WebKit's scrollTop is fractional.
			reachedTop = now.scrollTop < 1 && now.first === firstId;
			prev = now;
		}
		await context.close();
		return { scroll_jumps: reachedTop ? jumps : -1, scroll_worst_px: Math.round(worst) };
	} finally {
		await browser.close().catch(() => {});
	}
}

async function longChat(browser) {
	const { context, page, cdp } = await newPage(browser);
	const last = `${LONG_CHAT_ID}_${String(LONG_CHAT_MESSAGES - 1).padStart(4, '0')}`;
	const t0 = Date.now();
	await page.goto(`${ORIGIN}/chat/${LONG_CHAT_ID}`);
	await waitOrExplain(page, page.locator(`[data-message-id="${last}"]`), 'attached', 60_000);
	const renderedMs = Date.now() - t0;
	await page.waitForLoadState('networkidle');
	const longTasks = await page.evaluate(() => window.__longTasks);

	// Layout work a long transcript costs after it is drawn: narrowing the
	// window lays every message out again, and scrolling to the top brings
	// each one through the viewport. Rows the browser can skip off screen
	// show up here, not in the time to draw.
	await twoFrames(page);
	// A fresh context starts the counter at zero, so this is all the layout
	// and style work of opening the chat.
	const openLayoutMs = await layoutWorkMs(cdp);
	let before = openLayoutMs;
	await page.setViewportSize({ width: 1100, height: 900 });
	await twoFrames(page);
	const resizeLayoutMs = (await layoutWorkMs(cdp)) - before;
	before = await layoutWorkMs(cdp);
	await page.evaluate(async () => {
		const row = document.querySelector('[data-message-id]');
		let el = row?.parentElement ?? null;
		while (el && !(el.scrollHeight > el.clientHeight + 10 && /(auto|scroll)/.test(getComputedStyle(el).overflowY))) el = el.parentElement;
		if (!el) return;
		const steps = 20;
		const start = el.scrollTop;
		for (let i = 1; i <= steps; i++) {
			el.scrollTop = start * (1 - i / steps);
			await new Promise((r) => requestAnimationFrame(r));
		}
	});
	await twoFrames(page);
	const scrollLayoutMs = (await layoutWorkMs(cdp)) - before;

	const m = await settledMetrics(cdp, page);
	await context.close();
	return {
		rendered_ms: renderedMs,
		open_layout_ms: Math.round(openLayoutMs),
		resize_layout_ms: Math.round(resizeLayoutMs),
		scroll_layout_ms: Math.round(scrollLayoutMs),
		long_task_ms: Math.round(longTasks.reduce((a, b) => a + b, 0)),
		longest_task_ms: Math.round(Math.max(0, ...longTasks)),
		...m
	};
}

/**
 * Open and close the sidebar beside the long chat, through the header's ∴
 * button, and record the main thread's layout, style and script time per
 * toggle and any frame over 20 ms. The first two toggles warm up and are
 * left out. The glass's blur is redrawn on the GPU, which headless Chromium
 * does in software, so that half is not measured here.
 */
async function sidebarToggle(browser) {
	const { context, page, cdp } = await newPage(browser);
	await page.goto(`${ORIGIN}/chat/${LONG_CHAT_ID}`);
	await waitOrExplain(page, page.locator('.chat-input').first(), 'visible', 30_000);
	await page.waitForLoadState('networkidle');
	await sleep(1500);
	const metrics = async () => {
		const { metrics } = await cdp.send('Performance.getMetrics');
		return Object.fromEntries(metrics.map((x) => [x.name, x.value]));
	};
	const button = page.locator('button[aria-label="Hide the sidebar"], button[aria-label="Show the sidebar"]').first();
	const rows = [];
	for (let i = 0; i < 6; i++) {
		const m0 = await metrics();
		const frames = page.evaluate(
			() =>
				new Promise((done) => {
					const ts = [];
					const t0 = performance.now();
					const tick = (t) => {
						ts.push(t);
						if (t - t0 < 900) requestAnimationFrame(tick);
						else done(ts);
					};
					requestAnimationFrame(tick);
				})
		);
		await button.click();
		const ts = await frames;
		await sleep(300);
		const m1 = await metrics();
		if (i < 2) continue;
		const gaps = ts.slice(1).map((t, k) => t - ts[k]);
		rows.push({
			layout: (m1.LayoutDuration - m0.LayoutDuration) * 1000,
			style: (m1.RecalcStyleDuration - m0.RecalcStyleDuration) * 1000,
			script: (m1.ScriptDuration - m0.ScriptDuration) * 1000,
			slow: gaps.filter((g) => g > 20).length
		});
	}
	await context.close();
	const med = (k) => median(rows.map((r) => r[k]));
	return {
		toggle_layout_ms: +med('layout').toFixed(1),
		toggle_style_ms: +med('style').toFixed(1),
		toggle_script_ms: +med('script').toFixed(1),
		toggle_slow_frames: Math.max(...rows.map((r) => r.slow))
	};
}

/**
 * Send a message in the long chat and let fake-model.mjs stream its reply:
 * a fixed 242-token answer, one token every 15 ms. Records when the first
 * token shows and when the last does, how far the page's rendering falls
 * behind the stream's own pace (the extra beyond the stream's length), the
 * main thread's script, layout and style time, and frames over 20 ms. Each
 * run then removes the turn, so every run starts from the same 300 messages.
 */
/**
 * Remove any turn after the seeded 300, so every run streams into the same
 * chat. Run before each run as well as after, and after the box has had a
 * moment to write the reply's row: one cut short or still being saved would
 * otherwise carry into the next run.
 */
async function resetLongChat() {
	await sleep(500);
	psql(DB_URL, `DELETE FROM app_chat_messages WHERE chat_id = '${LONG_CHAT_ID}' AND sequence_num >= ${LONG_CHAT_MESSAGES}; UPDATE app_chats SET message_count = ${LONG_CHAT_MESSAGES} WHERE id = '${LONG_CHAT_ID}'`);
}

async function streamReply(browser) {
	await resetLongChat();
	const { context, page, cdp } = await newPage(browser);
	await page.goto(`${ORIGIN}/chat/${LONG_CHAT_ID}`);
	await waitOrExplain(page, page.locator('.chat-input').first(), 'visible', 30_000);
	await page.waitForLoadState('networkidle');
	await sleep(1000);
	const metrics = async () => {
		const { metrics } = await cdp.send('Performance.getMetrics');
		return Object.fromEntries(metrics.map((x) => [x.name, x.value]));
	};
	await page.locator('.chat-input').first().click();
	await page.keyboard.type('What should the weekend look like?');
	// Frame times, and when the reply's first and last words reach the page,
	// on the page's own clock. Watched with a MutationObserver on the reply
	// rows themselves: polling the page's text every frame would force a
	// layout per frame and count the lab's own work in the numbers.
	await page.evaluate(
		({ last }) => {
			window.__frames = [];
			window.__framesOn = true;
			const tick = (t) => {
				window.__frames.push(t);
				if (window.__framesOn) requestAnimationFrame(tick);
			};
			requestAnimationFrame(tick);
			const before = new Set([...document.querySelectorAll('[data-message-id]')].map((e) => e.dataset.messageId));
			window.__marks = {};
			const seen = new MutationObserver(() => {
				const fresh = [...document.querySelectorAll('[data-message-id][data-role="assistant"]')].filter((e) => !before.has(e.dataset.messageId));
				const text = fresh.map((e) => e.textContent ?? '').join('');
				if (!window.__marks.first && text.trim().length > 0) window.__marks.first = performance.timeOrigin + performance.now();
				if (!window.__marks.last && text.includes(last)) {
					window.__marks.last = performance.timeOrigin + performance.now();
					seen.disconnect();
				}
			});
			seen.observe(document.body, { childList: true, subtree: true, characterData: true });
		},
		{ last: 'until next weekend' }
	);
	const m0 = await metrics();
	const sentAt = await page.evaluate(() => performance.timeOrigin + performance.now());
	await page.keyboard.press('Enter');
	try {
		await page.waitForFunction(() => window.__marks.first, null, { timeout: 30_000, polling: 100 });
	} catch (e) {
		const shot = join(RESULTS, `failure-${Date.now()}.png`);
		await page.screenshot({ path: shot }).catch(() => {});
		const tail = await page.evaluate(() => document.body.innerText.slice(-400)).catch(() => '');
		throw new Error(`no streamed reply: ${e.message.split('\n')[0]}\n  page ends: ${JSON.stringify(tail)}\n  screenshot: ${shot}; box log: ${join(RESULTS, 'box.log')}`);
	}
	await page.waitForFunction(() => window.__marks.last, null, { timeout: 60_000, polling: 100 });
	const { first: firstAt, last: lastAt } = await page.evaluate(() => window.__marks);
	const m1 = await metrics();
	const frames = await page.evaluate(() => {
		window.__framesOn = false;
		return window.__frames;
	});
	await context.close();
	const gaps = frames.slice(1).map((t, k) => t - frames[k]);
	return {
		first_token_ms: Math.round(firstAt - sentAt),
		// From the model sending the last token to the page showing it: the
		// box's relay of the stream plus the page's rendering, on one clock.
		stream_lag_ms: Math.max(0, Math.round(lastAt - fakeModel.stats.lastSentAt)),
		stream_script_ms: Math.round((m1.ScriptDuration - m0.ScriptDuration) * 1000),
		stream_layout_ms: Math.round((m1.LayoutDuration - m0.LayoutDuration + m1.RecalcStyleDuration - m0.RecalcStyleDuration) * 1000),
		stream_slow_frames: gaps.filter((g) => g > 20).length,
		stream_longest_frame_ms: Math.round(Math.max(0, ...gaps))
	};
}

/** Move between views through the app's own router, and see what stays behind. */
async function viewCycle(browser) {
	const { context, page, cdp } = await newPage(browser);
	await page.goto(`${ORIGIN}/chat`);
	await waitOrExplain(page, page.locator('.chat-input').first(), 'visible', 30_000);
	const go = async (path) => {
		// A click on an in-app link is a client-side navigation, as a person makes.
		await page.evaluate((p) => {
			const a = document.createElement('a');
			a.href = p;
			document.body.appendChild(a);
			a.click();
			a.remove();
		}, path);
		await page.waitForURL((u) => u.pathname === path, { timeout: 15_000 });
		await page.waitForLoadState('networkidle');
		await sleep(300);
	};
	const laps = [];
	for (let lap = 1; lap <= LAPS; lap++) {
		for (const v of VIEWS) await go(v);
		if (lap === 2 || lap === LAPS) laps.push({ lap, ...(await settledMetrics(cdp, page)) });
	}
	await context.close();
	const [a, b] = laps;
	const per = (k) => +((b[k] - a[k]) / (LAPS - 2)).toFixed(3);
	return {
		laps: LAPS,
		views_per_lap: VIEWS.length,
		heap_mb_after: b.heap_mb,
		heap_mb_per_lap: per('heap_mb'),
		dom_nodes_per_lap: per('dom_nodes'),
		listeners_per_lap: per('listeners'),
		documents_per_lap: per('documents')
	};
}

function median(xs) {
	const s = [...xs].sort((a, b) => a - b);
	return s[Math.floor(s.length / 2)];
}

/** Run a scenario RUNS times and keep the median of every number, with the spread of the first. */
async function repeat(name, fn, browser, runs = RUNS) {
	const all = [];
	for (let i = 0; i < runs; i++) all.push(await fn(browser));
	const out = {};
	for (const k of Object.keys(all[0])) out[k] = median(all.map((r) => r[k]));
	const firstKey = Object.keys(all[0])[0];
	out._spread = { key: firstKey, min: Math.min(...all.map((r) => r[firstKey])), max: Math.max(...all.map((r) => r[firstKey])) };
	log(name, JSON.stringify(out));
	return out;
}

// How each number is held to its budget. Counts and per-lap growth are exact:
// they do not depend on how fast the machine is. Times get headroom, because
// they do. Two numbers are reported but never budgeted, because they move
// between runs of the same code: how many requests finished before the
// composer showed (a race), and the sum of all long tasks (the longest one is
// steady and is budgeted instead). The waterfall depth can be one off for the
// same race, so it gets one step of slack.
const RULES = {
	ready_ms: 'time', rendered_ms: 'time', composer_ms: 'time', sidebar_ms: 'time', open_layout_ms: 'layout', resize_layout_ms: 'layout', scroll_layout_ms: 'layout',
	first_token_ms: 'time', stream_lag_ms: 'time', stream_script_ms: 'time', stream_layout_ms: 'layout',
	stream_slow_frames: 'report', stream_longest_frame_ms: 'time',
	toggle_layout_ms: 'layout', toggle_style_ms: 'layout', toggle_script_ms: 'layout', toggle_slow_frames: 'approx', long_task_ms: 'report', longest_task_ms: 'time',
	api_before_ready: 'report', waterfall_depth: 'approx', api_total: 'count', preflights: 'approx', scroll_jumps: 'count', scroll_worst_px: 'report',
	js_transfer_kb: 'size', heap_mb: 'size', memory_mb: 'size', wasm_mb: 'size', heap_mb_after: 'size', dom_nodes: 'size',
	listeners: 'size', documents: 'count',
	heap_mb_per_lap: 'growth', dom_nodes_per_lap: 'growth', listeners_per_lap: 'growth', documents_per_lap: 'growth'
};
const TIME_HEADROOM = 1.3;
// A time measured as 0 (no long tasks) would otherwise budget 0, and the
// first 51 ms task anywhere would fail it.
const TIME_FLOOR_MS = 50;
// Layout work is a few to tens of milliseconds, and a 50 ms floor would let
// the long chat's relayout go back to what it cost before its rows could be
// skipped. Its floor is smaller.
const LAYOUT_FLOOR_MS = 10;
const SIZE_HEADROOM = 1.05;
const GROWTH_SLACK = { heap_mb_per_lap: 0.25, dom_nodes_per_lap: 2, listeners_per_lap: 0.5, documents_per_lap: 0 };

function toBudget(results) {
	const b = {};
	for (const [scenario, r] of Object.entries(results)) {
		b[scenario] = {};
		for (const [k, v] of Object.entries(r)) {
			const rule = RULES[k];
			if (!rule || rule === 'report') continue;
			if (rule === 'time') b[scenario][k] = Math.ceil(Math.max(v * TIME_HEADROOM, v + TIME_FLOOR_MS));
			else if (rule === 'layout') b[scenario][k] = Math.ceil(Math.max(v * TIME_HEADROOM, v + LAYOUT_FLOOR_MS));
			else if (rule === 'size') b[scenario][k] = +(v * SIZE_HEADROOM).toFixed(2);
			else if (rule === 'approx') b[scenario][k] = v + 1;
			else if (rule === 'growth') b[scenario][k] = +(Math.max(0, v) + GROWTH_SLACK[k]).toFixed(3);
			else b[scenario][k] = v;
		}
	}
	return b;
}

function compare(results, budgets) {
	const over = [];
	for (const [scenario, limits] of Object.entries(budgets)) {
		for (const [k, limit] of Object.entries(limits)) {
			const v = results[scenario]?.[k];
			// A number the run did not produce, or a -1 from a wait that timed
			// out, fails: a budget that skips them passes a broken scenario.
			if (v === undefined || v === null || Number.isNaN(v)) over.push(`${scenario}.${k} is missing`);
			else if (v < 0) over.push(`${scenario}.${k} = ${v}: the scenario never got there`);
			else if (v > limit) over.push(`${scenario}.${k} = ${v}, budget ${limit}`);
		}
	}
	return over;
}

async function main() {
	if (!existsSync(SERVER_BIN)) throw new Error(`no box binary at ${SERVER_BIN}; run \`cargo build -p virtues\``);
	takeLock();
	const keep = flag('--keep');
	let built = null;
	let model = null;
	let relay = null;
	let boxed = null;
	let browser = null;
	// The box is a plain child process: an exit or a crash here would leave it
	// running on the port, where the next run's health check would find it.
	const killBox = () => {
		if (boxed && !keep) boxed.box.kill('SIGTERM');
	};
	process.on('exit', killBox);
	let results;
	try {
		built = buildWeb();
		log(`web build: ${built.webBuild}`);
		model = await startFakeModel();
		fakeModel = model;
		boxed = await startBox(built.webBuild, model.url);
		relay = await startRelayProxy();
		log(`box on ${ORIGIN}, database ${DB}, seeded; relay proxy ${relayUrl}`);
		browser = await chromium.launch({ executablePath: chromiumPath(), headless: true });
		// One untimed load warms the box (prepared statements, the first
		// query plans) so the first measured run is not an outlier.
		await coldLoad(browser);
		results = {
			cold_load: await repeat('cold_load', coldLoad, browser),
			relay_load: await repeat('relay_load', relayLoad, browser),
			long_chat: await repeat('long_chat', longChat, browser),
			view_cycle: await repeat('view_cycle', viewCycle, browser, Math.min(RUNS, 3)),
			sidebar_toggle: await repeat('sidebar_toggle', sidebarToggle, browser),
			stream_reply: await repeat('stream_reply', streamReply, browser),
			// Deterministic: the same wheel steps over the same rows.
			webkit_scroll: await repeat('webkit_scroll', webkitScroll, null, 1)
		};
		await resetLongChat();
	} finally {
		await browser?.close().catch(() => {});
		relay?.close();
		if (!keep) {
			boxed?.box.kill('SIGTERM');
			model?.server.close();
			await sleep(500);
			try {
				psql(ADMIN_URL, `DROP DATABASE IF EXISTS ${DB} WITH (FORCE)`);
			} catch {
				/* the box may still hold it for a moment */
			}
			if (boxed) rmSync(boxed.state, { recursive: true, force: true });
			if (built?.clone) rmSync(built.clone, { recursive: true, force: true });
		} else if (boxed) {
			boxed.box.unref();
			log(`kept: box pid ${boxed.box.pid} on ${ORIGIN}, database ${DB}, log ${join(RESULTS, 'box.log')}. This runner keeps serving the fake model for it; stop both with Ctrl-C, then kill ${boxed.box.pid}`);
		}
		releaseLock();
	}

	const commit = execFileSync('git', ['-C', repo, 'rev-parse', '--short', opt('--ref', 'HEAD')]).toString().trim();
	const record = { commit, at: new Date().toISOString(), runs: RUNS, results };
	mkdirSync(RESULTS, { recursive: true });
	const file = join(RESULTS, `${record.at.replace(/[:.]/g, '-')}-${commit}.json`);
	writeFileSync(file, JSON.stringify(record, null, 2) + '\n');
	log(`results: ${file}`);

	if (flag('--update')) {
		writeFileSync(BUDGETS, JSON.stringify(toBudget(results), null, 2) + '\n');
		log(`wrote ${BUDGETS}`);
		return;
	}
	if (!existsSync(BUDGETS)) {
		log('no budgets.json yet; run with --update to record one');
		return;
	}
	const over = compare(results, JSON.parse(readFileSync(BUDGETS, 'utf8')));
	if (over.length) {
		console.error('[lab] over budget:\n  ' + over.join('\n  '));
		process.exitCode = 1;
		return;
	}
	log('within budget');
}

main().catch((e) => {
	console.error('[lab]', e.message ?? e);
	process.exit(1);
});
