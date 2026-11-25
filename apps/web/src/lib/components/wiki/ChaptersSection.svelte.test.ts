/**
 * The Chapters page shows the owner's words without repeating them: a
 * sentence that only restates the title is left out, as is a "How it ended"
 * that restates the sentence, and typed lowercase reads with a capital.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, tick, unmount } from 'svelte';
import type { ChapterApi } from '$lib/wiki/api';

const api = vi.hoisted(() => ({
	getChapters: vi.fn(),
	listYears: vi.fn(),
	updateChapter: vi.fn(),
	deleteChapter: vi.fn(),
}));
vi.mock('$lib/wiki/api', () => api);

const { default: ChaptersSection } = await import('./ChaptersSection.svelte');

function chapter(over: Partial<ChapterApi>): ChapterApi {
	return {
		id: 'chapter_a',
		kind: 'chapter',
		title: null,
		started_at: '1990-01-01',
		ended_at: '2000-01-01',
		is_current: false,
		changepoint: null,
		summary: null,
		...over,
	};
}

let mounted: ReturnType<typeof mount> | null = null;
let target: HTMLElement;

beforeEach(() => {
	api.listYears.mockResolvedValue([{ year: 2030, days_recorded: 12 }]);
	target = document.createElement('div');
	document.body.appendChild(target);
});

afterEach(() => {
	if (mounted) unmount(mounted);
	mounted = null;
	document.body.innerHTML = '';
});

async function draw(chapters: ChapterApi[]) {
	api.getChapters.mockResolvedValue(chapters);
	mounted = mount(ChaptersSection, { target });
	for (let i = 0; i < 5; i++) await tick();
	flushSync();
}

describe('ChaptersSection', () => {
	it('leaves out a sentence that repeats the title, and an ending that repeats the sentence', async () => {
		await draw([
			chapter({ id: 'c1', title: 'the river town', summary: 'The river town.', changepoint: 'moved inland' }),
			chapter({ id: 'c2', title: 'the boat years', started_at: '2000-01-01', ended_at: '2010-01-01', summary: 'sailing every summer', changepoint: 'Sailing every summer.' }),
		]);
		const items = [...target.querySelectorAll('li.chapter')];
		expect(items).toHaveLength(2);
		expect(items[0].querySelector('.title')?.textContent?.trim()).toBe('The river town');
		expect(items[0].querySelector('.summary')).toBeNull();
		expect(items[0].querySelector('.ended')?.textContent).toContain('Moved inland');
		expect(items[1].querySelector('.summary')?.textContent).toBe('Sailing every summer');
		expect(items[1].querySelector('.ended')).toBeNull();
	});

	it('numbers chapters and gives each its years and length', async () => {
		await draw([chapter({ id: 'c1', title: 'early days' })]);
		const item = target.querySelector('li.chapter')!;
		expect(item.querySelector('.numeral')?.textContent).toBe('I.');
		expect(item.querySelector('.years')?.textContent).toBe('1990 - 2000');
		expect(item.querySelector('.length')?.textContent).toBe('10 years');
		expect(item.querySelector('.note')?.textContent).toContain('Before your server started recording');
		expect(target.textContent).not.toContain('Unname');
	});
});
