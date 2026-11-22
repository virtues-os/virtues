#!/usr/bin/env node
/**
 * Write the Myna icons the app names, out of the full set in
 * @iconify-json/mynaui, as two files:
 *
 * - src/lib/myna-icons.json: the icons named by the code the app's first
 *   paint loads. They are registered before anything draws.
 * - src/lib/myna-icons-later.json: the rest, loaded just after the first
 *   paint (lib/icons.ts, `loadLaterMyna`). An icon from it drawn sooner
 *   holds its space, empty, until it arrives (components/Icon.svelte).
 *
 * The full set is 2,600 icons and about 240KB gzipped. The app names a few
 * hundred, so it bundles only those. Without a bundled icon, iconify fetches
 * the name from its public API, which a box may not reach.
 *
 * An icon is "named" when the string `mynaui:<name>` appears anywhere under
 * src/. A name built at runtime (`mynaui:heading-${n}`) is not found, so
 * write such names out in full.
 *
 * The first paint's code is every file reached from the shell's entry files
 * (`ROOTS`) by static imports; a dynamic `import()` is where it stops, as it
 * is for the bundler. legacyIcons.ts is left out of that scan: it maps old
 * stored names onto nearly every Myna icon, and a stored old name is rare
 * enough to wait for the later file.
 *
 * Myna draws a 1.5 line on a 24-unit grid, which is 1px at 16px. The shell's
 * line is 1.25px at 16px (design.md, "Glyphs"), so the stroke is
 * rewritten to 1.875 here, the same weight lucide.ts gives Lucide.
 *
 * Runs before `pnpm dev` and `pnpm build`. `--check` writes nothing and
 * exits 1 when the committed file is out of date.
 */

import { readFileSync, readdirSync, writeFileSync, existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';

const WEB_ROOT = dirname(dirname(fileURLToPath(import.meta.url)));
const SRC = join(WEB_ROOT, 'src');
const OUT = join(SRC, 'lib', 'myna-icons.json');
const OUT_LATER = join(SRC, 'lib', 'myna-icons-later.json');
const ROOTS = [
	'hooks.client.ts',
	'routes/+layout.js',
	'routes/+layout.svelte',
	'routes/(app)/+layout.ts',
	'routes/(app)/+layout.svelte',
	'routes/(app)/[...path]/+page.svelte',
].map((f) => join(SRC, f));
const NOT_SCANNED = [join(SRC, 'lib', 'legacyIcons.ts')];
const STROKE = '1.875';

const require = createRequire(import.meta.url);
const set = JSON.parse(readFileSync(require.resolve('@iconify-json/mynaui/icons.json'), 'utf8'));

function* sourceFiles(dir) {
	for (const entry of readdirSync(dir, { withFileTypes: true })) {
		const path = join(dir, entry.name);
		if (entry.isDirectory()) yield* sourceFiles(path);
		else if (/\.(svelte|ts|js)$/.test(entry.name)) yield path;
	}
}

const namesIn = (file) => [...readFileSync(file, 'utf8').matchAll(/mynaui:([a-z0-9-]+)/g)].map((m) => m[1]);

const named = new Set();
for (const file of sourceFiles(SRC)) for (const name of namesIn(file)) named.add(name);

// Static imports: `import x from '…'`, `import '…'`, `export … from '…'`.
// `import type` names no code, and `import('…')` loads later.
const IMPORT = /(?:^|[\s;])(?:import|export)\s+(?!type\s)(?:[^'"`;]*?\sfrom\s*)?['"]([^'"]+)['"]/g;
const EXTENSIONS = ['', '.ts', '.js', '.svelte.ts', '.svelte.js', '/index.ts', '/index.js'];

function resolveImport(from, spec) {
	let base;
	if (spec === '$lib') base = join(SRC, 'lib', 'index');
	else if (spec.startsWith('$lib/')) base = join(SRC, 'lib', spec.slice('$lib/'.length));
	else if (spec.startsWith('.')) base = join(dirname(from), spec);
	else return null; // a package, or an alias with no icons in it
	for (const ext of EXTENSIONS) {
		const path = base + ext;
		if (existsSync(path) && /\.(svelte|ts|js)$/.test(path)) return path;
	}
	return null;
}

const firstPaint = new Set();
const stack = ROOTS.filter((f) => existsSync(f));
if (stack.length !== ROOTS.length) {
	console.error(`myna-icons: an entry file is missing: ${ROOTS.filter((f) => !existsSync(f)).join(', ')}`);
	process.exit(1);
}
while (stack.length) {
	const file = stack.pop();
	if (firstPaint.has(file)) continue;
	firstPaint.add(file);
	for (const m of readFileSync(file, 'utf8').matchAll(IMPORT)) {
		const next = resolveImport(file, m[1]);
		if (next && !firstPaint.has(next)) stack.push(next);
	}
}

const early = new Set();
for (const file of firstPaint) {
	if (NOT_SCANNED.includes(file)) continue;
	for (const name of namesIn(file)) early.add(name);
}

const icons = {};
const later = {};
const unknown = [];
for (const name of [...named].sort()) {
	const parent = set.icons[name] ? name : set.aliases?.[name]?.parent;
	const icon = parent && set.icons[parent];
	if (!icon) {
		unknown.push(name);
		continue;
	}
	const data = { ...icon, body: icon.body.replaceAll('stroke-width="1.5"', `stroke-width="${STROKE}"`) };
	if (early.has(name)) icons[name] = data;
	else later[name] = data;
}

if (unknown.length) {
	console.error(`myna-icons: not in @iconify-json/mynaui: ${unknown.join(', ')}`);
	process.exit(1);
}

const collection = (icons) => `${JSON.stringify({ prefix: 'mynaui', width: 24, height: 24, icons }, null, '\t')}\n`;
const outputs = [
	[OUT, collection(icons)],
	[OUT_LATER, collection(later)],
];

if (process.argv.includes('--check')) {
	for (const [path, json] of outputs) {
		if (!existsSync(path) || readFileSync(path, 'utf8') !== json) {
			console.error(`myna-icons: ${path.slice(WEB_ROOT.length + 1)} is out of date. Run \`node scripts/myna-icons.mjs\`.`);
			process.exit(1);
		}
	}
} else {
	for (const [path, json] of outputs) writeFileSync(path, json);
}
