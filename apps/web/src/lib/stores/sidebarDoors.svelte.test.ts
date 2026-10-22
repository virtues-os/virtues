import { describe, expect, it, vi } from 'vitest';

const saved: unknown[] = [];
vi.mock('$lib/api/client', () => ({
	getProfile: vi.fn(async () => ({ sidebar_doors: null })),
	updateProfile: vi.fn(async (p: unknown) => {
		saved.push(p);
		return {};
	}),
}));

const { arrange, DOORS, sidebarDoors } = await import('./sidebarDoors.svelte');

const ids = (doors: { id: string }[]) => doors.map((d) => d.id);

describe('arranging the doors', () => {
	it('keeps the saved order, skips ids this build does not know, and puts a missing door at its shipped place', () => {
		expect(ids(arrange(['wiki', 'gone', 'new-chat', 'pages', 'wiki']))).toEqual([
			'wiki',
			'new-chat',
			'pages',
			'tasks',
			'projects',
			'applets',
		]);
	});

	it('is the shipped arrangement when nothing was saved', () => {
		expect(ids(arrange([]))).toEqual(ids([...DOORS]));
	});

	it('moves among the shown doors and leaves a hidden door where it was', async () => {
		await sidebarDoors.reset();
		await sidebarDoors.setHidden('tasks', true);
		// Shown: new-chat, pages, projects, applets, wiki. Wiki to the top.
		await sidebarDoors.move(4, 0);
		expect(ids(sidebarDoors.visible)).toEqual(['wiki', 'new-chat', 'pages', 'projects', 'applets']);
		expect(ids(sidebarDoors.all)).toEqual(['wiki', 'new-chat', 'tasks', 'pages', 'projects', 'applets']);
		await sidebarDoors.setHidden('tasks', false);
		expect(ids(sidebarDoors.visible)).toEqual(['wiki', 'new-chat', 'tasks', 'pages', 'projects', 'applets']);
		expect(saved.at(-1)).toEqual({
			sidebar_doors: { order: ['wiki', 'new-chat', 'tasks', 'pages', 'projects', 'applets'], hidden: [] },
		});
	});
});
