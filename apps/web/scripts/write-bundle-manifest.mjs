#!/usr/bin/env node
/**
 * Stamp `build/.virtues-bundle.json` — the bundle's self-description.
 *
 * The box serves the static build from disk and hands this file out at
 * `GET /api/web-bundle/version`, so a client can ask "is there newer UI, and
 * can my shell run it?" without downloading anything first.
 *
 * The bundle describes ITSELF rather than inheriting the box's version. That
 * matters: once a client caches a bundle, the box can be upgraded while the
 * client still runs the previous one, and reporting the box's number would
 * assert something the client never loaded. See docs/spa-delivery-plan.md and
 * `mac-plan.md` invariant 4 ("no component asserts a fact it didn't observe").
 *
 * Runs as part of `pnpm build`, after vite, so it lands in the adapter's
 * output directory.
 */

import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync, readdirSync, statSync } from 'node:fs';
import { join, dirname, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const WEB_ROOT = dirname(dirname(fileURLToPath(import.meta.url)));
const BUILD_DIR = join(WEB_ROOT, 'build');
const MANIFEST = join(BUILD_DIR, '.virtues-bundle.json');

/** Same channel rules as $lib/build.ts and the box's codename::channel(). */
function deriveChannel(raw) {
	if (!raw || raw === 'dev') return 'dev';
	if (raw.includes('staging')) return 'staging';
	if (raw.startsWith('edge')) return 'edge';
	if (raw.includes('-')) return 'dev';
	return 'stable';
}

/**
 * Content hash over the built files, sorted for determinism.
 *
 * The version tag alone cannot answer "is this the same bundle?" — local and
 * dev builds all report `dev`, and two builds of one tag can differ. The hash
 * is what a client actually compares, so a same-tag rebuild still updates and
 * an identical rebuild does not.
 */
function hashTree(dir) {
	const files = [];
	(function walk(d) {
		for (const entry of readdirSync(d)) {
			const p = join(d, entry);
			if (statSync(p).isDirectory()) walk(p);
			else files.push(p);
		}
	})(dir);

	const hash = createHash('sha256');
	for (const p of files.sort()) {
		hash.update(relative(dir, p));
		hash.update(readFileSync(p));
	}
	return hash.digest('hex').slice(0, 16);
}

const contract = JSON.parse(readFileSync(join(WEB_ROOT, 'bundle-contract.json'), 'utf8'));
const rawVersion = process.env.GIT_DESCRIBE || process.env.VIRTUES_BUILD_VERSION || 'dev';
const rawSha = process.env.GIT_COMMIT || 'dev';

const manifest = {
	version: rawVersion.replace(/^v/, ''),
	sha: rawSha === 'dev' ? 'dev' : rawSha.slice(0, 7),
	channel: deriveChannel(rawVersion),
	minShellVersion: contract.minShellVersion,
	// Written last so it covers every other emitted file but not itself.
	contentHash: hashTree(BUILD_DIR)
};

writeFileSync(MANIFEST, JSON.stringify(manifest, null, 2) + '\n');
console.log(
	`∴ bundle manifest ${manifest.version} (${manifest.contentHash}) · ` +
		`minShell ${manifest.minShellVersion}`
);
