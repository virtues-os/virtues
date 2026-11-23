// Where an applet opens and where it is listed, from the kind the server
// gives it. Every name is invented.
import { describe, expect, it } from 'vitest';
import { homeOf, inSitesDoor, isPlainSite, optionsOf, pageOf } from './kind';

const parts = (over: Record<string, unknown> = {}) =>
	({ kind: 'skill', schedule: null, watch: null, triggers: ['manual', 'message'], command: null, agent: null, ...over }) as Parameters<typeof isPlainSite>[0];

describe('the kind of an applet', () => {
	it('opens each kind at an address that says the word', () => {
		expect(homeOf({ ...parts({ kind: 'site' }), id: 'applet_user__water_log' })).toBe('/site/water_log');
		expect(homeOf({ ...parts({ kind: 'site' }), id: 'applet_star_map' })).toBe('/site/star_map');
		expect(homeOf({ ...parts({ kind: 'skill' }), id: 'applet_meeting_prep' })).toBe('/skill/meeting_prep');
		expect(homeOf({ ...parts({ kind: 'server_task' }), id: 'applet_embedding_index' })).toBe('/server-task/embedding_index');
		expect(optionsOf({ ...parts({ kind: 'skill' }), id: 'applet_user__packing_list' }, 'history')).toBe(
			'/skill/packing_list/options?panel=history'
		);
	});

	it('lists only your sites in the Sites door', () => {
		const yours = (over: Record<string, unknown>) => ({ ...parts(over), origin: 'ai', archived_at: null }) as Parameters<typeof inSitesDoor>[0];
		expect(inSitesDoor(yours({ kind: 'site' }))).toBe(true);
		// A skill is under Skills, and a task with its row is in Tasks.
		expect(inSitesDoor(yours({ kind: 'skill' }))).toBe(false);
		expect(inSitesDoor({ ...yours({ kind: 'assistant_task' }), task_id: 'task_1' })).toBe(false);
		// On a server from before task rows, a task of yours is still somewhere.
		expect(inSitesDoor(yours({ kind: 'assistant_task' }))).toBe(true);
		// What came with the server, and what has finished, are not listed.
		expect(inSitesDoor({ ...yours({ kind: 'site' }), origin: 'system' })).toBe(false);
		expect(inSitesDoor({ ...yours({ kind: 'site' }), archived_at: '2026-11-01T09:00:00Z' })).toBe(false);
	});

	it('opens a task of the assistant\'s as its task once it has a row, and keeps its own page for its options', () => {
		const review = { ...parts({ kind: 'assistant_task' }), id: 'applet_user__week_in_review' };
		expect(homeOf(review)).toBe('/site/week_in_review');
		expect(homeOf({ ...review, task_id: 'task_1' })).toBe('/task/task_1');
		expect(pageOf({ ...review, task_id: 'task_1' })).toBe('/site/week_in_review');
		expect(optionsOf({ ...review, task_id: 'task_1' })).toBe('/site/week_in_review/options');
		// A site that refreshes keeps its own page even with a row.
		expect(homeOf({ ...parts({ kind: 'site' }), id: 'applet_user__water_log', task_id: 'task_2' })).toBe('/site/water_log');
	});

	it('calls a site plain only when it has nothing to run', () => {
		expect(isPlainSite(parts({ kind: 'site' }))).toBe(true);
		expect(isPlainSite(parts({ kind: 'site', schedule: '0 0 6 * * *' }))).toBe(false);
		expect(isPlainSite(parts({ kind: 'site', agent: 'Add a glass.' }))).toBe(false);
		expect(isPlainSite(parts({ kind: 'site', command: ['refresh'] }))).toBe(false);
		expect(isPlainSite(parts({ kind: 'site', triggers: ['webhook'] }))).toBe(false);
		expect(isPlainSite(parts({ kind: 'skill' }))).toBe(false);
	});
});
