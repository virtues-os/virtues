import { describe, expect, it } from 'vitest';
import { mountedTabs, touch } from './keepAlive';

const tab = (id: string, route = `/page/${id}`) => ({ id, route });

describe('keepAlive', () => {
	it('puts the visible tabs first and remembers the rest in order', () => {
		expect(touch(['a', 'b', 'c'], ['c'])).toEqual(['c', 'a', 'b']);
		expect(touch([], ['x', 'y'])).toEqual(['x', 'y']);
	});

	it('keeps the visible tabs and the few most recent hidden ones', () => {
		const tabs = ['a', 'b', 'c', 'd', 'e', 'f', 'g'].map((id) => tab(id));
		const kept = mountedTabs(tabs, ['a'], ['a', 'b', 'c', 'd', 'e', 'f', 'g'], 2);
		expect([...kept].sort()).toEqual(['a', 'b', 'c']);
	});

	it('never mounts a tab that is not open, and never mounts an unvisited one', () => {
		const kept = mountedTabs([tab('a'), tab('b')], ['a'], ['gone', 'a']);
		expect([...kept]).toEqual(['a']);
	});

	it('keeps a temporary chat mounted however long ago it was seen', () => {
		const tabs = [tab('a'), tab('ghost', '/chat?temporary=1'), tab('b')];
		const kept = mountedTabs(tabs, ['a'], ['a', 'b'], 0);
		expect(kept.has('ghost')).toBe(true);
		expect(kept.has('b')).toBe(false);
	});
});
