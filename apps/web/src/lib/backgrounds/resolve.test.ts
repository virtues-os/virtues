import { describe, expect, it } from 'vitest';
import { dayNumber, find, parseSetting, pickForDay, pool, srcset, type StyleManifest, type Variant } from './resolve';
import { narrowest, stepFrom } from './browse';

function variant(name: string): Variant {
	return {
		source: name,
		title: name,
		luminance: 0.5,
		focus: [0, 0, 1, 1],
		fade: 'top',
		width: 1792,
		height: 768,
		files: {
			avif: { '1792': `c/light/${name}-1792.avif`, '896': `c/light/${name}-896.avif` },
			webp: { '1792': `c/light/${name}-1792.webp`, '896': `c/light/${name}-896.webp` },
		},
	};
}

const styles: StyleManifest[] = [
	{
		version: 1,
		style: 'paintings',
		title: 'Paintings',
		renderer: 'image',
		collections: [
			{ id: 'forest', title: 'Forest', pictures: ['oak', 'birch', 'pine'].map((id) => ({ id, light: variant(id), dark: variant(`${id}-night`) })) },
			{ id: 'beach', title: 'Beach', pictures: ['dune', 'cove'].map((id) => ({ id, light: variant(id), dark: variant(`${id}-night`) })) },
		],
	},
];

describe('parseSetting', () => {
	it('reads each depth', () => {
		expect(parseSetting('')).toEqual({ kind: 'default' });
		expect(parseSetting(undefined)).toEqual({ kind: 'default' });
		expect(parseSetting('plain')).toEqual({ kind: 'plain' });
		expect(parseSetting('paintings')).toEqual({ kind: 'path', style: 'paintings', collection: undefined, picture: undefined });
		expect(parseSetting('paintings/forest/oak')).toEqual({ kind: 'path', style: 'paintings', collection: 'forest', picture: 'oak' });
	});

	it('treats a malformed setting as the default', () => {
		for (const bad of ['a/b/c/d', '../x', 'Paintings', 'a//b', 42, { style: 'x' }]) {
			expect(parseSetting(bad)).toEqual({ kind: 'default' });
		}
	});
});

describe('pool', () => {
	it('covers everything under the path', () => {
		expect(pool(styles, parseSetting('paintings'))).toHaveLength(5);
		expect(pool(styles, parseSetting('paintings/beach')).map((a) => a.picture)).toEqual(['dune', 'cove']);
		expect(pool(styles, parseSetting('paintings/forest/pine'))).toEqual([{ style: 'paintings', collection: 'forest', picture: 'pine' }]);
	});

	it('is empty for a style that is not installed, plain, or the default', () => {
		expect(pool(styles, parseSetting('shaders'))).toEqual([]);
		expect(pool(styles, parseSetting('paintings/forest/gone'))).toEqual([]);
		expect(pool(styles, parseSetting('plain'))).toEqual([]);
		expect(pool(styles, parseSetting(''))).toEqual([]);
	});

	it('finds the picture an address names', () => {
		expect(find(styles, { style: 'paintings', collection: 'beach', picture: 'cove' })?.dark.source).toBe('cove-night');
	});
});

describe('pickForDay', () => {
	const items = ['a', 'b', 'c', 'd', 'e', 'f', 'g'];

	it('shows each picture once per cycle', () => {
		for (const start of [0, 7, 20_412]) {
			const cycleStart = start - (start % items.length);
			const seen = Array.from({ length: items.length }, (_, i) => pickForDay(items, 'k', cycleStart + i));
			expect(new Set(seen).size).toBe(items.length);
		}
	});

	it('keeps the same picture all day and differs between settings', () => {
		expect(pickForDay(items, 'k', 100)).toBe(pickForDay(items, 'k', 100));
		const a = Array.from({ length: 14 }, (_, d) => pickForDay(items, 'one', d)).join('');
		const b = Array.from({ length: 14 }, (_, d) => pickForDay(items, 'two', d)).join('');
		expect(a).not.toBe(b);
	});

	it('handles empty and single pools', () => {
		expect(pickForDay([], 'k', 3)).toBeUndefined();
		expect(pickForDay(['only'], 'k', 3)).toBe('only');
	});
});

describe('dayNumber', () => {
	it('changes at local midnight', () => {
		expect(dayNumber(new Date(2026, 10, 20, 0, 1))).toBe(dayNumber(new Date(2026, 10, 20, 23, 59)));
		expect(dayNumber(new Date(2026, 10, 21, 0, 0))).toBe(dayNumber(new Date(2026, 10, 20, 12)) + 1);
	});
});

describe('srcset', () => {
	it('lists widths narrowest first', () => {
		expect(srcset(variant('oak'), 'avif', (r) => `/f/${r}`)).toBe('/f/c/light/oak-896.avif 896w, /f/c/light/oak-1792.avif 1792w');
	});
});

describe('stepFrom', () => {
	const forest = pool(styles, parseSetting('paintings/forest'));

	it('steps forward and back, wrapping at the ends', () => {
		expect(stepFrom(forest, 'paintings/forest/oak', 1)?.picture).toBe('birch');
		expect(stepFrom(forest, 'paintings/forest/pine', 1)?.picture).toBe('oak');
		expect(stepFrom(forest, 'paintings/forest/oak', -1)?.picture).toBe('pine');
	});

	it('starts at an end from a picture outside the pool', () => {
		expect(stepFrom(forest, 'paintings/beach/dune', 1)?.picture).toBe('oak');
		expect(stepFrom(forest, 'paintings/beach/dune', -1)?.picture).toBe('pine');
		expect(stepFrom([], 'paintings/forest/oak', 1)).toBeUndefined();
	});
});

describe('narrowest', () => {
	it('picks the smallest WebP', () => {
		expect(narrowest(variant('oak'), (rel) => `/f/${rel}`)).toBe('/f/c/light/oak-896.webp');
	});
});
