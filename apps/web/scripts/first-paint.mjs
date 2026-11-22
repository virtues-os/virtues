/**
 * The app's first-paint code, read from the source: every file reached from
 * the shell's entry files by static imports. A dynamic `import()` is where it
 * stops, as it is for the bundler, so this is the code the chat home loads
 * before it can draw (tools/perf/bundle-budget.mjs measures the same set from
 * a build).
 *
 * scripts/myna-icons.mjs uses it to keep the icons this code names in the
 * first paint, and lib/perf/firstPaint.test.ts uses it to keep the `$lib`
 * barrel out of it.
 */

import { existsSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';

/** The shell's entry files, relative to src/. */
export const ROOTS = [
	'hooks.client.ts',
	'routes/+layout.js',
	'routes/+layout.svelte',
	'routes/(app)/+layout.ts',
	'routes/(app)/+layout.svelte',
	'routes/(app)/[...path]/+page.svelte',
];

// Static imports: `import x from '…'`, `import '…'`, `export … from '…'`.
// `import type` names no code, and `import('…')` loads later.
const IMPORT = /(?:^|[\s;])(?:import|export)\s+(?!type\s)(?:[^'"`;]*?\sfrom\s*)?['"]([^'"]+)['"]/g;
const EXTENSIONS = ['', '.ts', '.js', '.svelte.ts', '.svelte.js', '/index.ts', '/index.js'];

/**
 * The file an import names, or null for a package or an alias with no source here.
 * @param {string} src
 * @param {string} from
 * @param {string} spec
 * @returns {string | null}
 */
function resolveImport(src, from, spec) {
	/** @type {string} */
	let base;
	if (spec === '$lib') base = join(src, 'lib', 'index');
	else if (spec.startsWith('$lib/')) base = join(src, 'lib', spec.slice('$lib/'.length));
	else if (spec.startsWith('.')) base = join(dirname(from), spec);
	else return null;
	for (const ext of EXTENSIONS) {
		const path = base + ext;
		if (existsSync(path) && /\.(svelte|ts|js)$/.test(path)) return path;
	}
	return null;
}

/**
 * Walk the first paint's code under `src`.
 *
 * `files` is every file reached. `importers` maps each import specifier
 * named by a reached file to the files that name it, so a check can ask
 * who imports a given module.
 * @param {string} src
 * @returns {{ files: Set<string>, importers: Map<string, Set<string>> }}
 */
export function firstPaint(src) {
	const roots = ROOTS.map((f) => join(src, f));
	const missing = roots.filter((f) => !existsSync(f));
	if (missing.length) throw new Error(`first-paint: an entry file is missing: ${missing.join(', ')}`);

	/** @type {Set<string>} */
	const files = new Set();
	/** @type {Map<string, Set<string>>} */
	const importers = new Map();
	const stack = [...roots];
	for (let file = stack.pop(); file !== undefined; file = stack.pop()) {
		if (files.has(file)) continue;
		files.add(file);
		for (const m of readFileSync(file, 'utf8').matchAll(IMPORT)) {
			const spec = m[1];
			const named = importers.get(spec) ?? new Set();
			named.add(file);
			importers.set(spec, named);
			const next = resolveImport(src, file, spec);
			if (next && !files.has(next)) stack.push(next);
		}
	}
	return { files, importers };
}
