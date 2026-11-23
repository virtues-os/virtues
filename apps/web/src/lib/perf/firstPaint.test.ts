/**
 * The `$lib` barrel (src/lib/index.ts) re-exports components the first
 * paint does not need, Markdown among them. The build keeps a re-exported
 * component even when nothing uses it, so one `from '$lib'` in first-paint
 * code puts every one of them, and the libraries they import, in front of
 * every launch: the reply renderer alone is about 140 KB of JavaScript.
 *
 * Code the first paint loads imports each component from its own file.
 * The barrel stays for views that load later.
 */

import { fileURLToPath } from 'node:url';
import { relative } from 'node:path';
import { describe, expect, it } from 'vitest';
// A plain script, shared with scripts/myna-icons.mjs; its JSDoc gives the types.
import { firstPaint } from '../../../scripts/first-paint.mjs';

const SRC = fileURLToPath(new URL('../..', import.meta.url));

describe('first-paint code', () => {
	const { files, importers } = firstPaint(SRC);

	it('is found', () => {
		// A walk that stopped matching imports would pass the check below on nothing.
		expect(files.size).toBeGreaterThan(200);
		expect([...files].some((f) => f.endsWith('components/tabs/views/ChatView.svelte'))).toBe(true);
	});

	it('does not import the $lib barrel', () => {
		const offenders = [...(importers.get('$lib') ?? [])].map((f) => relative(SRC, f)).sort();
		expect(offenders, "Import each component from its own file, not from '$lib'").toEqual([]);
	});
});
