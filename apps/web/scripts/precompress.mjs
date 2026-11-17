#!/usr/bin/env node
/**
 * Write `.br` and `.gz` siblings next to every compressible built asset.
 *
 * The box serves the SPA with tower-http's `ServeDir` and its precompressed
 * brotli and gzip siblings: a client that accepts brotli gets the `.br`, one
 * that accepts only gzip the `.gz`, and everyone else the original. Brotli is
 * about a fifth smaller than gzip on this JavaScript. Compressing once here, at build, means the box never spends
 * CPU on it and the Mac — a thin client that pulls the SPA from the box on
 * every cold start — moves ~2.6 MB of initial JavaScript as ~0.8 MB.
 *
 * Only the content-hashed tree under `_app/immutable/` and the fonts are
 * compressed. Documents (`200.html`, `index.html`) are `no-store` on the box
 * and tiny; images and woff2 are already compressed; anything under 1 KB
 * costs more in a round trip than it saves.
 *
 * Four consumers of `build/` must NOT see these siblings, and each handles
 * it on its own side, for both extensions:
 *   - the OTA tarball the box hands to phones skips them
 *     (virtues-core/src/api/web_bundle.rs), or every phone update would triple;
 *   - the bundle content hash skips them (write-bundle-manifest.mjs, and the
 *     shell's own in src-tauri/src/web_bundle.rs), so the hash still describes
 *     content, not encoding;
 *   - the Mac and iPhone builds delete them before baking `build/` into the
 *     app (scripts/bake-desktop-ui.sh).
 *
 * Runs as part of `pnpm build`, after vite and before the manifest.
 */

import { readdirSync, readFileSync, statSync, writeFileSync, existsSync } from 'node:fs';
import { dirname, extname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { brotliCompressSync, gzipSync, constants } from 'node:zlib';

const WEB_ROOT = dirname(dirname(fileURLToPath(import.meta.url)));
const BUILD_DIR = join(WEB_ROOT, 'build');

const ROOTS = ['_app/immutable', 'fonts'];
const COMPRESSIBLE = new Set(['.js', '.css', '.json', '.svg', '.map', '.txt', '.ttf', '.otf', '.wasm']);
const MIN_BYTES = 1024;

function walk(dir, out) {
	for (const entry of readdirSync(dir)) {
		const p = join(dir, entry);
		if (statSync(p).isDirectory()) walk(p, out);
		else out.push(p);
	}
	return out;
}

/** The two encodings, each written only where it saves at least a tenth. */
const ENCODINGS = [
	{ ext: '.br', compress: (b) => brotliCompressSync(b, { params: { [constants.BROTLI_PARAM_QUALITY]: 11, [constants.BROTLI_PARAM_SIZE_HINT]: b.length } }) },
	{ ext: '.gz', compress: (b) => gzipSync(b, { level: constants.Z_BEST_COMPRESSION }) }
];

let files = 0;
let rawBytes = 0;
const encodedBytes = { '.br': 0, '.gz': 0 };
for (const root of ROOTS) {
	const dir = join(BUILD_DIR, root);
	if (!existsSync(dir)) continue;
	for (const p of walk(dir, [])) {
		if (!COMPRESSIBLE.has(extname(p))) continue;
		const st = statSync(p);
		if (st.size < MIN_BYTES) continue;
		files++;
		rawBytes += st.size;
		let source = null;
		for (const { ext, compress } of ENCODINGS) {
			const out = p + ext;
			// Content-hashed paths never change in place, so an existing sibling
			// that is newer than its source is still right.
			if (existsSync(out) && statSync(out).mtimeMs >= st.mtimeMs) {
				encodedBytes[ext] += statSync(out).size;
				continue;
			}
			source ??= readFileSync(p);
			const encoded = compress(source);
			// A sibling that does not actually save anything is pure overhead.
			if (encoded.length >= st.size * 0.9) {
				encodedBytes[ext] += st.size;
				continue;
			}
			writeFileSync(out, encoded);
			encodedBytes[ext] += encoded.length;
		}
	}
}

const mb = (n) => (n / 1024 / 1024).toFixed(1);
console.log(`∴ precompressed ${files} files · ${mb(rawBytes)} MB → ${mb(encodedBytes['.br'])} MB brotli, ${mb(encodedBytes['.gz'])} MB gzip`);
