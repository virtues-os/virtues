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
		// 'tasks' was a door until Tasks joined Projects; a profile saved then still says it.
		// Storage is missing from that save, so it lands after Wiki, the door
		// before it in the shipped order.
		expect(ids(arrange(['wiki', 'tasks', 'new-chat', 'pages', 'wiki']))).toEqual([
			'wiki',
			'storage',
			'new-chat',
			'pages',
			'threads',
			'applets',
		]);
	});

	it('is the shipped arrangement when nothing was saved', () => {
		expect(ids(arrange([]))).toEqual(ids([...DOORS]));
	});

	it('moves among the shown doors and leaves a hidden door where it was', async () => {
		await sidebarDoors.reset();
		await sidebarDoors.setHidden('threads', true);
		// Shown: new-chat, pages, applets, wiki, storage. Wiki to the top.
		await sidebarDoors.move(3, 0);
		expect(ids(sidebarDoors.visible)).toEqual(['wiki', 'new-chat', 'pages', 'applets', 'storage']);
		expect(ids(sidebarDoors.all)).toEqual(['wiki', 'new-chat', 'threads', 'pages', 'applets', 'storage']);
		await sidebarDoors.setHidden('threads', false);
		expect(ids(sidebarDoors.visible)).toEqual(['wiki', 'new-chat', 'threads', 'pages', 'applets', 'storage']);
		expect(saved.at(-1)).toEqual({
			sidebar_doors: { order: ['wiki', 'new-chat', 'threads', 'pages', 'applets', 'storage'], hidden: [] },
		});
	});
});
