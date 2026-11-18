import { describe, expect, it } from 'vitest';
import { slug, specimens, splitDoc } from './specimens';
import { designDoc } from './doc';

describe('splitDoc', () => {
	it('splits prose from the specimens placed in it', () => {
		const parts = splitDoc('# Title\n\nSome words.\n\n```specimen Button\n```\n\nMore words.\n');
		expect(parts).toEqual([
			{ kind: 'markdown', text: '# Title\n\nSome words.\n\n' },
			{ kind: 'specimen', name: 'Button' },
			{ kind: 'markdown', text: '\n\nMore words.\n' }
		]);
	});

	it('leaves an ordinary code block alone', () => {
		const parts = splitDoc('```ts\nconst a = 1;\n```\n');
		expect(parts).toEqual([{ kind: 'markdown', text: '```ts\nconst a = 1;\n```\n' }]);
	});
});

describe('slug', () => {
	it('matches the anchors design.md links to', () => {
		expect(slug('Drag and drop')).toBe('drag-and-drop');
		expect(slug('How /design works')).toBe('how-design-works');
		expect(slug('The serif has one weight')).toBe('the-serif-has-one-weight');
	});
});

describe('design.md', () => {
	it('places only specimens that exist', () => {
		const placed = splitDoc(designDoc).flatMap((p) => (p.kind === 'specimen' ? [p.name] : []));
		expect(placed.length).toBeGreaterThan(0);
		expect(placed.filter((name) => !specimens.has(name))).toEqual([]);
	});

	it('links every #section to a heading it has', () => {
		const headings = new Set(
			[...designDoc.matchAll(/^#{1,4} (.+)$/gm)].map((m) => slug(m[1]))
		);
		const anchors = [...designDoc.matchAll(/\]\(#([^)]+)\)/g)].map((m) => m[1]);
		expect(anchors.filter((a) => !headings.has(a))).toEqual([]);
	});
});
