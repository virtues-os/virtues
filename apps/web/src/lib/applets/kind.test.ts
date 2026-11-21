// What an applet is to the person who meets it. The same table as the
// server's `a_kind_follows_from_the_parts_in_order`. Every name is invented.
import { describe, expect, it } from 'vitest';
import { homeOf, inSitesDoor, kindOf, optionsOf, pageOf } from './kind';

const parts = (over: Record<string, unknown> = {}) =>
	({ schedule: null, watch: null, triggers: ['manual', 'message'], command: null, has_face: false, ...over }) as Parameters<typeof kindOf>[0];

describe('the kind of an applet', () => {
	it('takes the server\'s word when it gives one', () => {
		expect(kindOf(parts({ kind: 'site' }))).toBe('site');
		expect(kindOf(parts({ kind: 'skill', has_face: true }))).toBe('skill');
	});

	it('follows from the parts, in order, for a server that does not say', () => {
		expect(kindOf(parts())).toBe('skill');
		expect(kindOf(parts({ schedule: '0 0 7 * * *' }))).toBe('assistant_task');
		expect(kindOf(parts({ watch: { table: 'data_health_workout' } }))).toBe('assistant_task');
		expect(kindOf(parts({ triggers: ['webhook'] }))).toBe('assistant_task');
		expect(kindOf(parts({ schedule: '  ' }))).toBe('skill');
		expect(kindOf(parts({ command: ['embedding_index'] }))).toBe('server_task');
		expect(kindOf(parts({ command: ['embedding_index'], schedule: '0 */15 * * * *' }))).toBe('server_task');
		expect(kindOf(parts({ has_face: true, command: ['x'], schedule: '0 0 6 * * *' }))).toBe('site');
	});

	it('opens each kind at an address that says the word', () => {
		expect(homeOf({ ...parts({ kind: 'site' }), id: 'applet_user__water_log' })).toBe('/site/water_log');
		expect(homeOf({ ...parts({ kind: 'site' }), id: 'applet_dot_cloud' })).toBe('/site/dot_cloud');
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
});
