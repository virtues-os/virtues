#!/usr/bin/env node
/**
 * Build the editor's markdown converter: crates/virtues-document-wasm for
 * wasm32-unknown-unknown, copied to src/lib/document/converter.wasm, where
 * `converter.ts` loads it from.
 *
 * The module is built here, from the workspace's own source and Cargo.lock,
 * every time the app is built, served or tested, and never committed: a
 * committed module would stay behind when the crate moves on, and be a
 * second converter. Cargo rebuilds only what changed, so a run with nothing
 * new takes about a second.
 *
 * Runs before `pnpm build` and `pnpm dev`, and as vitest's global setup
 * (scripts/vitest-setup.mjs). Needs Rust and its wasm32-unknown-unknown
 * target.
 *
 * It builds into a target directory of its own (`.converter-target` at the
 * repo root, or VIRTUES_CONVERTER_TARGET_DIR), never the workspace's: the
 * workspace's is shared with the server's build and `make dev` holds its
 * lock while the server rebuilds, which would stall every test run and dev
 * start behind it.
 */

import { spawnSync } from 'node:child_process';
import { copyFileSync, existsSync, statSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const WEB_ROOT = dirname(dirname(fileURLToPath(import.meta.url)));
const REPO_ROOT = dirname(dirname(WEB_ROOT));
const OUT = join(WEB_ROOT, 'src/lib/document/converter.wasm');
const TARGET = 'wasm32-unknown-unknown';
const PROFILE = 'wasm';
const TARGET_DIR = process.env.VIRTUES_CONVERTER_TARGET_DIR || join(REPO_ROOT, '.converter-target');
const CRATE = 'virtues-document-wasm';

/** Build the module and copy it into place; exits the process on failure. */
export function buildConverter() {
	const build = spawnSync('cargo', ['build', '-p', CRATE, '--target', TARGET, '--profile', PROFILE, '--locked'], {
		cwd: REPO_ROOT,
		env: { ...process.env, CARGO_TARGET_DIR: TARGET_DIR },
		stdio: ['ignore', 'inherit', 'pipe'],
		encoding: 'utf8',
	});
	if (build.error) {
		console.error(`✖ the editor's converter needs Rust to build: ${build.error.message}`);
		process.exit(1);
	}
	if (build.status !== 0) {
		process.stderr.write(build.stderr);
		if (/target may not be installed|can't find crate for `(core|std)`/.test(build.stderr)) {
			console.error(`\n✖ Rust lacks the WebAssembly target. Add it, then build again:\n\n    rustup target add ${TARGET}\n`);
		}
		process.exit(build.status ?? 1);
	}
	const built = join(TARGET_DIR, TARGET, PROFILE, `${CRATE.replaceAll('-', '_')}.wasm`);
	if (!existsSync(built)) {
		console.error(`✖ cargo built the converter, but ${built} is not there`);
		process.exit(1);
	}
	copyFileSync(built, OUT);
	console.log(`∴ converter · ${(statSync(OUT).size / 1024).toFixed(0)} KiB → src/lib/document/converter.wasm`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) buildConverter();
