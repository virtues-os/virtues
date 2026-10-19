/**
 * CI runs every vitest file. Its vitest step once named seven folders, and
 * the tests of how another device draws an upload and of the reload guard
 * for files on their way sat outside all of them, run by nobody.
 */

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const WEB = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const CI = path.resolve(WEB, '../../.github/workflows/ci.yml');

function testFiles(dir: string): string[] {
	return fs.readdirSync(dir, { withFileTypes: true }).flatMap((e) => {
		const at = path.join(dir, e.name);
		if (e.isDirectory()) return testFiles(at);
		return e.name.endsWith('.test.ts') ? [path.relative(WEB, at)] : [];
	});
}

describe('CI', () => {
	it('runs every vitest file', () => {
		const steps = fs
			.readFileSync(CI, 'utf8')
			.split('\n')
			// Steps, not the comments about them.
			.filter((line) => /\bpnpm vitest run\b/.test(line) && !line.trim().startsWith('#'));
		expect(steps.length).toBeGreaterThan(0);
		const files = testFiles(path.join(WEB, 'src'));
		expect(files).toContain('src/lib/components/pages/nodes/MediaNode.svelte.test.ts');
		for (const step of steps) {
			// A path after `run` narrows the files vitest runs to those under it.
			const filters = step
				.split(/\bpnpm vitest run\b/)[1]
				.trim()
				.split(/\s+/)
				.filter((arg) => arg && !arg.startsWith('-'));
			const missed = filters.length ? files.filter((f) => !filters.some((only) => f.includes(only))) : [];
			expect(missed, step.trim()).toEqual([]);
		}
	});
});
