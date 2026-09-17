#!/usr/bin/env node
/**
 * Write a `.gz` sibling next to every compressible built asset.
 *
 * The box serves the SPA with tower-http's `ServeDir::precompressed_gzip()`,
 * which hands a client that accepts gzip the sibling file and everyone else
 * the original. Compressing once here, at build, means the box never spends
 * CPU on it and the Mac — a thin client that pulls the SPA from the box on
 * every cold start — moves ~2.6 MB of initial JavaScript as ~0.8 MB.
 *
 * Only the content-hashed tree under `_app/immutable/` and the fonts are
 * compressed. Documents (`200.html`, `index.html`) are `no-store` on the box
 * and tiny; images and woff2 are already compressed; anything under 1 KB
 * costs more in a round trip than it saves.
 *
 * Three consumers of `build/` must NOT see these siblings, and each handles
 * it on its own side:
 *   - the OTA tarball the box hands to phones skips `*.gz`
 *     (virtues-core/src/api/web_bundle.rs), or every phone update would double;
 *   - the bundle content hash skips `*.gz` (write-bundle-manifest.mjs), so
 *     the hash still describes content, not encoding;
 *   - the iOS build deletes them before baking `build/` into the IPA
 *     (tauri.ios.conf.json `beforeBuildCommand`).
 *
 * Runs as part of `pnpm build`, after vite and before the manifest.
 */

import { readdirSync, readFileSync, statSync, writeFileSync, existsSync } from 'node:fs';
import { dirname, extname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { gzipSync, constants } from 'node:zlib';

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

let files = 0;
let rawBytes = 0;
let gzBytes = 0;
for (const root of ROOTS) {
	const dir = join(BUILD_DIR, root);
	if (!existsSync(dir)) continue;
	for (const p of walk(dir, [])) {
		if (!COMPRESSIBLE.has(extname(p))) continue;
		const st = statSync(p);
		if (st.size < MIN_BYTES) continue;
		const gzPath = p + '.gz';
		// Content-hashed paths never change in place, so an existing sibling
		// that is newer than its source is still right.
		if (existsSync(gzPath) && statSync(gzPath).mtimeMs >= st.mtimeMs) {
			files++;
			rawBytes += st.size;
			gzBytes += statSync(gzPath).size;
			continue;
		}
		const gz = gzipSync(readFileSync(p), { level: constants.Z_BEST_COMPRESSION });
		// A sibling that does not actually save anything is pure overhead.
		if (gz.length >= st.size * 0.9) continue;
		writeFileSync(gzPath, gz);
		files++;
		rawBytes += st.size;
		gzBytes += gz.length;
	}
}

const mb = (n) => (n / 1024 / 1024).toFixed(1);
console.log(`∴ precompressed ${files} files · ${mb(rawBytes)} MB → ${mb(gzBytes)} MB gzip`);
