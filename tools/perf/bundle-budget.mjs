#!/usr/bin/env node
// What the app's main route loads before its first paint, against a budget.
//
// The chat home is where every launch lands, so the JavaScript and CSS it
// pulls in statically is a cost every user pays on every cold start. Heavy
// libraries reached it by accident three times in one day (yjs through a
// plugin key, Shiki through a static import, motion through the lightbox), and
// nothing noticed until someone measured by hand. This measures it from the
// build's own manifest and fails when it grows.
//
// A RATCHET, like tools/design-lint.sh: the baseline may only go down. When a
// change genuinely needs more, update the baseline in the same commit and say
// why in its message. Measure it from a scratch clone's build, never from the
// shared checkout (CLAUDE.md, "Never cut a ratchet baseline from this
// checkout"); agents/build/performance.md has the commands.
//
// Usage, after `pnpm build` in apps/web:
//     node tools/perf/bundle-budget.mjs            check against the baseline
//     node tools/perf/bundle-budget.mjs --update   rewrite the baseline
//     node tools/perf/bundle-budget.mjs --json     print the measurement
//     node tools/perf/bundle-budget.mjs --top 20   also list the largest chunks

import { readFileSync, writeFileSync, existsSync } from 'node:fs';
import { gzipSync } from 'node:zlib';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '../..');
const web = process.env.WEB_DIR ? resolve(process.env.WEB_DIR) : join(repo, 'apps/web');
const baselinePath = join(here, 'bundle-budget.baseline');
const args = process.argv.slice(2);

const MANIFEST = join(web, '.svelte-kit/output/client/.vite/manifest.json');
const APP_JS = join(web, '.svelte-kit/generated/client-optimized/app.js');
const CLIENT = join(web, '.svelte-kit/output/client');

// The route every launch lands on: the (app) group's catch-all.
const ROUTE = '/(app)/[...path]';

// A build of the same code compresses to the same bytes, so the tolerance only
// absorbs a hash renaming a chunk. Anything larger is a real change.
const TOLERANCE_BYTES = 512;

function fail(msg) {
	console.error(`bundle-budget: ${msg}`);
	process.exit(1);
}

if (!existsSync(MANIFEST) || !existsSync(APP_JS)) {
	fail(`no build under ${web}. Run \`pnpm build\` in apps/web first.`);
}

const manifest = JSON.parse(readFileSync(MANIFEST, 'utf8'));
const appJs = readFileSync(APP_JS, 'utf8');

// `dictionary` maps each route to [page node, [layout nodes]]; node 0 is the
// root layout every route has. Parsed from the generated file so a new route
// group cannot silently move the measurement onto the wrong page.
const entry = appJs.match(new RegExp(`"${ROUTE.replace(/[[\]().]/g, '\\$&')}":\\s*\\[(\\d+),\\s*\\[([\\d,\\s]*)\\]`));
if (!entry) fail(`route ${ROUTE} not found in ${APP_JS}`);
const nodes = [0, Number(entry[1]), ...entry[2].split(',').filter(Boolean).map(Number)];

const startKey = Object.keys(manifest).find((k) => k.endsWith('kit/src/runtime/client/entry.js'));
const appKey = Object.keys(manifest).find((k) => k.endsWith('generated/client-optimized/app.js'));
if (!startKey || !appKey) fail('the manifest has no SvelteKit entry; the build layout changed');
const roots = [
	startKey,
	appKey,
	...nodes.map((n) => {
		const k = `.svelte-kit/generated/client-optimized/nodes/${n}.js`;
		if (!manifest[k]) fail(`node ${n} missing from the manifest`);
		return k;
	})
];

// Static imports only: a dynamic `import()` is exactly what keeps a library
// off this path, so following them would measure the wrong thing.
const js = new Set();
const css = new Set();
const stack = [...roots];
while (stack.length) {
	const k = stack.pop();
	if (js.has(k)) continue;
	js.add(k);
	for (const c of manifest[k].css ?? []) css.add(c);
	stack.push(...(manifest[k].imports ?? []));
}

const sizes = (file) => {
	const bytes = readFileSync(join(CLIENT, file));
	return { raw: bytes.length, gzip: gzipSync(bytes, { level: 6 }).length };
};
const sum = (files) =>
	[...files].reduce(
		(acc, f) => {
			const s = sizes(f);
			acc.raw += s.raw;
			acc.gzip += s.gzip;
			return acc;
		},
		{ raw: 0, gzip: 0 }
	);

const jsFiles = [...js].map((k) => manifest[k].file);
const jsSize = sum(jsFiles);
const cssSize = sum(css);
const measured = {
	route: ROUTE,
	js_files: jsFiles.length,
	js_raw_bytes: jsSize.raw,
	js_gzip_bytes: jsSize.gzip,
	css_files: css.size,
	css_raw_bytes: cssSize.raw,
	css_gzip_bytes: cssSize.gzip
};

if (args.includes('--json')) {
	console.log(JSON.stringify(measured, null, 2));
	process.exit(0);
}

const kb = (n) => `${(n / 1024).toFixed(1)} KB`;
console.log(
	`${ROUTE} before first paint: ${measured.js_files} JS files, ${kb(measured.js_raw_bytes)} raw / ${kb(measured.js_gzip_bytes)} gzip; ` +
		`${measured.css_files} CSS files, ${kb(measured.css_raw_bytes)} raw / ${kb(measured.css_gzip_bytes)} gzip`
);

const topIdx = args.indexOf('--top');
if (topIdx >= 0) {
	const n = Number(args[topIdx + 1] ?? 15);
	const ranked = [...js]
		.map((k) => ({ k, ...sizes(manifest[k].file) }))
		.sort((a, b) => b.gzip - a.gzip)
		.slice(0, n);
	for (const r of ranked) console.log(`  ${kb(r.gzip).padStart(10)} gzip  ${manifest[r.k].name ?? r.k}`);
}

// What the budget holds. Raw bytes follow gzip closely and add nothing.
const KEYS = ['js_gzip_bytes', 'css_gzip_bytes'];

if (args.includes('--update')) {
	const lines = [
		'# What /(app)/[...path] loads before first paint (tools/perf/bundle-budget.mjs).',
		'# A ratchet: these may only go down. Update in a scratch clone, never in the',
		'# shared checkout, and say in the commit message why a number went up.',
		...KEYS.map((k) => `${measured[k]} ${k}`)
	];
	writeFileSync(baselinePath, lines.join('\n') + '\n');
	console.log(`bundle-budget: wrote ${baselinePath}`);
	process.exit(0);
}

if (!existsSync(baselinePath)) fail(`no baseline at ${baselinePath}; run with --update in a scratch clone`);
const baseline = Object.fromEntries(
	readFileSync(baselinePath, 'utf8')
		.split('\n')
		.filter((l) => l && !l.startsWith('#'))
		.map((l) => {
			const [n, k] = l.trim().split(/\s+/);
			return [k, Number(n)];
		})
);

let over = false;
for (const k of KEYS) {
	const allowed = baseline[k];
	if (allowed === undefined) fail(`baseline has no ${k}`);
	const delta = measured[k] - allowed;
	if (delta > TOLERANCE_BYTES) {
		over = true;
		console.error(`bundle-budget: ${k} is ${measured[k]}, over the budget of ${allowed} by ${kb(delta)}.`);
	} else if (delta < -TOLERANCE_BYTES) {
		console.log(`bundle-budget: ${k} is ${kb(-delta)} under budget. Lower the baseline to keep the gain.`);
	}
}
if (over) {
	console.error(
		'bundle-budget: something new is loaded before the first paint. Run with --top 20 to see the largest chunks, ' +
			'and load the new code with a dynamic import() where it is first used.'
	);
	process.exit(1);
}
console.log('bundle-budget: within budget.');
