#!/usr/bin/env node
/**
 * Write src/lib/myna-icons.json: the Myna icons the app names, out of the
 * full set in @iconify-json/mynaui.
 *
 * The full set is 2,600 icons and about 240KB gzipped. The app names a few
 * hundred, so it bundles only those. Without a bundled icon, iconify fetches
 * the name from its public API, which a box may not reach.
 *
 * An icon is "named" when the string `mynaui:<name>` appears anywhere under
 * src/. A name built at runtime (`mynaui:heading-${n}`) is not found, so
 * write such names out in full.
 *
 * Myna draws a 1.5 line on a 24-unit grid, which is 1px at 16px. The shell's
 * line is 1.25px at 16px (design.md, "Lines and glyphs"), so the stroke is
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

const named = new Set();
for (const file of sourceFiles(SRC)) {
	for (const match of readFileSync(file, 'utf8').matchAll(/mynaui:([a-z0-9-]+)/g)) named.add(match[1]);
}

const icons = {};
const unknown = [];
for (const name of [...named].sort()) {
	const parent = set.icons[name] ? name : set.aliases?.[name]?.parent;
	const icon = parent && set.icons[parent];
	if (!icon) {
		unknown.push(name);
		continue;
	}
	icons[name] = { ...icon, body: icon.body.replaceAll('stroke-width="1.5"', `stroke-width="${STROKE}"`) };
}

if (unknown.length) {
	console.error(`myna-icons: not in @iconify-json/mynaui: ${unknown.join(', ')}`);
	process.exit(1);
}

const json = `${JSON.stringify({ prefix: 'mynaui', width: 24, height: 24, icons }, null, '\t')}\n`;

if (process.argv.includes('--check')) {
	if (!existsSync(OUT) || readFileSync(OUT, 'utf8') !== json) {
		console.error('myna-icons: src/lib/myna-icons.json is out of date. Run `node scripts/myna-icons.mjs`.');
		process.exit(1);
	}
} else {
	writeFileSync(OUT, json);
}
