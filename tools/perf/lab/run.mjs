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
import { existsSync, mkdtempSync, readdirSync, readFileSync, writeFileSync, mkdirSync, symlinkSync, rmSync } from 'node:fs';
import { tmpdir, homedir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright-core';
import { seedSql, LONG_CHAT_ID, LONG_CHAT_MESSAGES } from './seed.mjs';

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

/** Build apps/web from a commit in a scratch clone, so the measurement is of committed code. */
function buildWeb() {
	const given = opt('--web-build');
	if (given) return resolve(given);
	const ref = opt('--ref', 'HEAD');
	const dir = mkdtempSync(join(tmpdir(), 'virtues-perf-'));
	log(`building ${ref} in ${dir}`);
	execFileSync('git', ['clone', '--quiet', '--local', '--no-checkout', repo, dir]);
	execFileSync('git', ['-C', dir, 'checkout', '--quiet', execFileSync('git', ['-C', repo, 'rev-parse', ref]).toString().trim()]);
	symlinkSync(join(repo, 'apps/web/node_modules'), join(dir, 'apps/web/node_modules'));
	execFileSync('pnpm', ['build'], { cwd: join(dir, 'apps/web'), stdio: ['ignore', 'ignore', 'inherit'] });
	return join(dir, 'apps/web/build');
}

function psql(url, sql) {
	return execFileSync('psql', [url, '-v', 'ON_ERROR_STOP=1', '-q', '-c', sql]).toString();
}

async function startBox(webBuild) {
	psql(ADMIN_URL, `DROP DATABASE IF EXISTS ${DB} WITH (FORCE)`);
	psql(ADMIN_URL, `CREATE DATABASE ${DB}`);
	const state = mkdtempSync(join(tmpdir(), 'virtues-perf-state-'));
	const env = {
		...process.env,
		DATABASE_URL: DB_URL,
		NOMAD_PORT_http: String(PORT),
		ENVIRONMENT: 'dev',
		VIRTUES_DEV_SKIP_SETUP: '1',
		VIRTUES_HTTPS_PORT: '0',
		VIRTUES_PUBLIC_ORIGINS: `${ORIGIN},http://localhost:${PORT}`,
		// No AI: nothing in the scenarios asks a model, and a dead address
		// keeps one from being asked by accident.
		VIRTUES_API_URL: 'http://127.0.0.1:9',
		VIRTUES_API_KEY: 'dev-local-key',
		STATIC_DIR: webBuild,
		STORAGE_PATH: join(state, 'storage'),
		VIRTUES_APPLET_STATE_DIR: join(state, 'applets'),
		RUST_LOG: 'warn'
	};
	const box = spawn(SERVER_BIN, ['server'], { env, stdio: ['ignore', 'ignore', 'pipe'] });
	let stderr = '';
	box.stderr.on('data', (d) => (stderr = (stderr + d).slice(-4000)));
	for (let i = 0; i < 120; i++) {
		if (box.exitCode !== null) throw new Error(`box exited early:\n${stderr}`);
		try {
			if ((await fetch(`${ORIGIN}/health`)).ok) break;
		} catch {
			/* not up yet */
		}
		await sleep(500);
		if (i === 119) throw new Error(`box never answered /health:\n${stderr}`);
	}
	// Migrations have run by the time /health answers.
	const seedFile = join(state, 'seed.sql');
	writeFileSync(seedFile, seedSql());
	execFileSync('psql', [DB_URL, '-v', 'ON_ERROR_STOP=1', '-q', '-f', seedFile]);
	return { box, state };
}

/** Counters from Chromium itself, after collecting garbage twice. */
async function settledMetrics(cdp) {
	await cdp.send('HeapProfiler.collectGarbage');
	await cdp.send('HeapProfiler.collectGarbage');
	const { metrics } = await cdp.send('Performance.getMetrics');
	const m = Object.fromEntries(metrics.map((x) => [x.name, x.value]));
	return { heap_mb: +(m.JSHeapUsedSize / 1e6).toFixed(2), dom_nodes: m.Nodes, listeners: m.JSEventListeners, documents: m.Documents };
}

async function newPage(browser) {
	const context = await browser.newContext({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 1 });
	const page = await context.newPage();
	const cdp = await context.newCDPSession(page);
	await cdp.send('Performance.enable');
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
	const m = await settledMetrics(cdp);
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

async function longChat(browser) {
	const { context, page, cdp } = await newPage(browser);
	const last = `${LONG_CHAT_ID}_${String(LONG_CHAT_MESSAGES - 1).padStart(4, '0')}`;
	const t0 = Date.now();
	await page.goto(`${ORIGIN}/chat/${LONG_CHAT_ID}`);
	await waitOrExplain(page, page.locator(`[data-message-id="${last}"]`), 'attached', 60_000);
	const renderedMs = Date.now() - t0;
	await page.waitForLoadState('networkidle');
	const longTasks = await page.evaluate(() => window.__longTasks);
	const m = await settledMetrics(cdp);
	await context.close();
	return {
		rendered_ms: renderedMs,
		long_task_ms: Math.round(longTasks.reduce((a, b) => a + b, 0)),
		longest_task_ms: Math.round(Math.max(0, ...longTasks)),
		...m
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
		if (lap === 2 || lap === LAPS) laps.push({ lap, ...(await settledMetrics(cdp)) });
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
	ready_ms: 'time', rendered_ms: 'time', long_task_ms: 'report', longest_task_ms: 'time',
	api_before_ready: 'report', waterfall_depth: 'approx', api_total: 'count',
	js_transfer_kb: 'size', heap_mb: 'size', heap_mb_after: 'size', dom_nodes: 'size',
	listeners: 'size', documents: 'count',
	heap_mb_per_lap: 'growth', dom_nodes_per_lap: 'growth', listeners_per_lap: 'growth', documents_per_lap: 'growth'
};
const TIME_HEADROOM = 1.3;
// A time measured as 0 (no long tasks) would otherwise budget 0, and the
// first 51 ms task anywhere would fail it.
const TIME_FLOOR_MS = 50;
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
			if (v === undefined) continue;
			if (v > limit) over.push(`${scenario}.${k} = ${v}, budget ${limit}`);
		}
	}
	return over;
}

async function main() {
	if (!existsSync(SERVER_BIN)) throw new Error(`no box binary at ${SERVER_BIN}; run \`cargo build -p virtues\``);
	const webBuild = buildWeb();
	log(`web build: ${webBuild}`);
	const { box, state } = await startBox(webBuild);
	log(`box on ${ORIGIN}, database ${DB}, seeded`);
	const browser = await chromium.launch({ executablePath: chromiumPath(), headless: true });
	let results;
	try {
		// One untimed load warms the box (prepared statements, the first
		// query plans) so the first measured run is not an outlier.
		await coldLoad(browser);
		results = {
			cold_load: await repeat('cold_load', coldLoad, browser),
			long_chat: await repeat('long_chat', longChat, browser),
			view_cycle: await repeat('view_cycle', viewCycle, browser, Math.min(RUNS, 3))
		};
	} finally {
		await browser.close();
		if (!flag('--keep')) {
			box.kill('SIGTERM');
			await sleep(500);
			try {
				psql(ADMIN_URL, `DROP DATABASE IF EXISTS ${DB} WITH (FORCE)`);
			} catch {
				/* the box may still hold it for a moment */
			}
			rmSync(state, { recursive: true, force: true });
		} else {
			log(`kept: box pid ${box.pid} on ${ORIGIN}, database ${DB}`);
		}
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
		process.exit(1);
	}
	log('within budget');
}

main().catch((e) => {
	console.error('[lab]', e.message ?? e);
	process.exit(1);
});
