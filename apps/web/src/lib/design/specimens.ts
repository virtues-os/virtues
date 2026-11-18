/**
 * The specimens the /design page can place (agents/build/design.md).
 *
 * A specimen is a `*.specimen.svelte` file anywhere under `src/lib`, kept
 * beside the component it shows: `Button.specimen.svelte` next to
 * `Button.svelte`. Its name is the file name before `.specimen.svelte`, and
 * design.md places it with a fenced block:
 *
 *     ```specimen Button
 *     ```
 *
 * The glob finds every specimen, so adding one needs no list. Each loads
 * only when the page that places it opens.
 */

import type { Component } from 'svelte';

const modules = import.meta.glob<{ default: Component }>('/src/lib/**/*.specimen.svelte');

export type SpecimenEntry = { name: string; path: string; load: () => Promise<{ default: Component }> };

export const specimens: Map<string, SpecimenEntry> = new Map(
	Object.entries(modules).map(([path, load]) => {
		const name = path.split('/').pop()!.replace(/\.specimen\.svelte$/, '');
		return [name, { name, path: path.replace(/^\/src\//, 'apps/web/src/'), load }];
	})
);

/** The fence that places a specimen: ```specimen Name``` with nothing inside. */
export const SPECIMEN_FENCE = /^```specimen[ \t]+([\w-]+)[ \t]*\n```[ \t]*$/gm;

export type DocPart = { kind: 'markdown'; text: string } | { kind: 'specimen'; name: string };

/** Split a markdown document into prose and the specimens placed in it. */
export function splitDoc(source: string): DocPart[] {
	const parts: DocPart[] = [];
	let last = 0;
	for (const m of source.matchAll(SPECIMEN_FENCE)) {
		const text = source.slice(last, m.index);
		if (text.trim()) parts.push({ kind: 'markdown', text });
		parts.push({ kind: 'specimen', name: m[1] });
		last = m.index! + m[0].length;
	}
	const tail = source.slice(last);
	if (tail.trim()) parts.push({ kind: 'markdown', text: tail });
	return parts;
}

/** GitHub's heading slug, so the doc's own `#section` links resolve on the page. */
export function slug(text: string): string {
	return text
		.trim()
		.toLowerCase()
		.replace(/[^\p{L}\p{N}\s-]/gu, '')
		.replace(/\s/g, '-');
}
