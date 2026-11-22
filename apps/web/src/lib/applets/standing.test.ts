// Where an applet stands, in words. Every applet, time and error here is invented.
import { describe, expect, it } from 'vitest';
import type { Applet } from '$lib/api/client';
import { whenInWords } from '$lib/utils/dateUtils';
import { actionOf, appletLine, appletsLead, groupServerTasks, needsYou, problemOf, troubleOf } from './standing';

const NOW = new Date('2026-10-06T12:00:00').getTime();
const at = (local: string) => new Date(local).toISOString();
const clock = (local: string) => new Date(local).toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' });

const applet = (over: Partial<Applet> = {}): Applet =>
	({
		id: 'applet_a',
		name: 'Morning examen',
		origin: 'user',
		schedule: '0 7 * * *',
		watch: null,
		enabled: true,
		archived_at: null,
		next_due_at: null,
		triggers: ['cron', 'manual'],
		last_run: { status: 'success', started_at: at('2026-10-06T07:00:00'), records_processed: null, error: null },
		...over,
	}) as Applet;

const failed = (error: string | null, over: Partial<Applet> = {}) =>
	applet({ last_run: { status: 'error', started_at: at('2026-10-06T06:00:00'), records_processed: null, error }, ...over });

describe('whenInWords', () => {
	it('says minutes up to an hour, then the day and the time', () => {
		expect(whenInWords(at('2026-10-06T11:56:00'), NOW)).toBe('4 minutes ago');
		expect(whenInWords(at('2026-10-06T12:20:00'), NOW)).toBe('in 20 minutes');
		expect(whenInWords(at('2026-10-06T07:00:00'), NOW)).toBe(`today at ${clock('2026-10-06T07:00:00')}`);
		expect(whenInWords(at('2026-10-05T23:00:00'), NOW)).toBe(`yesterday at ${clock('2026-10-05T23:00:00')}`);
		expect(whenInWords(at('2026-10-07T09:00:00'), NOW)).toMatch(/^tomorrow at /);
		expect(whenInWords(at('2026-10-02T09:00:00'), NOW)).toMatch(/^on \S+ at /);
		expect(whenInWords(at('2026-09-12T09:00:00'), NOW)).toMatch(/^on /);
		expect(whenInWords(at('2026-09-12T09:00:00'), NOW)).not.toMatch(/ at /);
	});
});

describe('needsYou', () => {
	const failedRun = { status: 'error', started_at: null, records_processed: null, error: 'x' } as Applet['last_run'];
	const bare = (over: Partial<Applet> = {}) => ({ id: 'applet_a', enabled: true, archived_at: null, next_due_at: null, ...over }) as Applet;

	it('is true for a failed last run and for a run more than an hour late', () => {
		expect(needsYou(bare({ last_run: failedRun }), NOW)).toBe(true);
		expect(needsYou(bare({ next_due_at: '2026-10-06T10:00:00' }), NOW)).toBe(true);
		expect(needsYou(bare({ next_due_at: '2026-10-06T11:30:00' }), NOW)).toBe(false);
	});

	it('leaves applets that are off or finished alone', () => {
		expect(needsYou(bare({ last_run: failedRun, enabled: false }), NOW)).toBe(false);
		expect(needsYou(bare({ last_run: failedRun, archived_at: '2026-10-01T00:00:00' }), NOW)).toBe(false);
	});
});

describe('troubleOf', () => {
	it("gives a failed run's own reason when it has one", () => {
		expect(problemOf(failed('The weather service did not answer'), NOW)).toBe('The weather service did not answer');
		expect(problemOf(failed('failed to spawn action command: No such file or directory (os error 2)'), NOW)).toBe("Its program couldn't start");
		expect(problemOf(failed(null), NOW)).toBe(`Its last run didn't finish today at ${clock('2026-10-06T06:00:00')}`);
	});

	it('tells a lapsed sign-in from other failures', () => {
		expect(troubleOf(failed('Gmail is disconnected'), NOW)?.kind).toBe('signin');
		expect(troubleOf(failed('timed out'), NOW)?.kind).toBe('failed');
	});

	it('says a late run as what happened, not as overdue', () => {
		const late = applet({ next_due_at: at('2026-10-06T10:00:00'), last_run: { status: 'success', started_at: at('2026-10-05T07:00:00'), records_processed: null, error: null } });
		expect(problemOf(late, NOW)).toBe(`Hasn't run since yesterday at ${clock('2026-10-05T07:00:00')}`);
		expect(problemOf(applet({ next_due_at: at('2026-10-06T10:00:00'), last_run: null }), NOW)).toBe("Hasn't run yet");
	});

	it('leaves an applet that is off, finished, or stopped at its limit alone', () => {
		expect(troubleOf(failed('timed out', { enabled: false }), NOW)).toBeNull();
		expect(troubleOf(failed('timed out', { archived_at: at('2026-10-01T09:00:00') }), NOW)).toBeNull();
		expect(troubleOf(applet({ last_run: { status: 'budget_exceeded', started_at: at('2026-10-06T07:00:00'), records_processed: null, error: null } }), NOW)).toBeNull();
	});
});

describe('actionOf', () => {
	it('offers Reconnect for a lapsed sign-in, Retry for a failure, Run now for a late run', () => {
		expect(actionOf(failed('Gmail is disconnected'), NOW)).toEqual({ kind: 'fix', label: 'Reconnect Gmail', href: '/sources' });
		expect(actionOf(failed('timed out'), NOW)).toEqual({ kind: 'run', label: 'Retry' });
		expect(actionOf(applet({ next_due_at: at('2026-10-06T10:00:00') }), NOW)).toEqual({ kind: 'run', label: 'Run now' });
	});

	it('offers nothing to run when the applet has no manual trigger', () => {
		expect(actionOf(failed('timed out', { triggers: ['cron'] }), NOW)).toBeNull();
	});

	it('offers nothing for an applet that is fine', () => {
		expect(actionOf(applet(), NOW)).toBeNull();
	});
});

describe('appletLine', () => {
	it('says when it runs, then how the last run went', () => {
		expect(appletLine(applet(), NOW)).toBe(`Daily at 7am. Ran today at ${clock('2026-10-06T07:00:00')}.`);
		expect(appletLine(failed('The weather service did not answer.'), NOW)).toBe('Daily at 7am. The weather service did not answer.');
		expect(appletLine(applet({ last_run: { status: 'running', started_at: at('2026-10-06T11:59:00'), records_processed: null, error: null } }), NOW)).toBe('Daily at 7am. Running now.');
	});

	it('gives the next run when it has never run', () => {
		const monthly = applet({ schedule: '0 9 25 * *', last_run: null, next_due_at: at('2026-10-25T09:00:00') });
		expect(appletLine(monthly, NOW)).toMatch(/\. Next run on .+\.$/);
		expect(appletLine(applet({ last_run: null }), NOW)).toBe("Daily at 7am. Hasn't run yet.");
	});

	it('says Off and Finished without a schedule', () => {
		expect(appletLine(applet({ enabled: false }), NOW)).toBe('Off.');
		expect(appletLine(applet({ archived_at: at('2026-10-06T07:05:00') }), NOW)).toBe(`Finished today at ${clock('2026-10-06T07:05:00')}.`);
	});
});

describe('groupServerTasks', () => {
	it('lists the server\'s tasks by account, and leaves skills and what you set up to their own pages', () => {
		const program = { command: ['run'] };
		const groups = groupServerTasks(
			[
				applet({ id: 'a', name: 'Rent reminder' }),
				applet({ id: 'b', name: 'Garden watering', ...failedRun('timed out') }),
				applet({ id: 'c', name: 'Backup', origin: 'system', schedule: '0 0 4 * * *', ...program }),
				applet({ id: 'd', name: 'Mail sync', origin: 'source', credential_id: 'cred_1', schedule: '0 */15 * * * *', ...program }),
				applet({ id: 'e', name: 'Calendar sync', origin: 'source', credential_id: 'cred_1', ...program, ...failedRun('timed out') }),
				applet({ id: 'f', name: 'Phone', origin: 'source', device_id: 'dev_1', schedule: null, triggers: ['webhook'], ...program }),
				applet({ id: 'g', name: 'Trip planner', origin: 'system', schedule: null }),
				applet({ id: 'h', name: 'Trip reminder', archived_at: at('2026-10-01T09:00:00') }),
				applet({ id: 'i', name: 'Chat import', origin: 'system', schedule: null, ...program }),
				applet({ id: 'j', name: 'Morning examen', origin: 'system' }),
			],
			NOW,
		);
		expect(groups.accounts.map((g) => [g.kind, g.id, g.tasks.map((a) => a.id)])).toEqual([
			['credential', 'cred_1', ['e', 'd']],
			['device', 'dev_1', ['f']],
		]);
		// The shipped task of the assistant's sits here until Tasks can show it.
		expect(groups.server.map((a) => a.id)).toEqual(['c', 'j']);
		expect(groups.whenAsked.map((a) => a.id)).toEqual(['i']);
		expect(groups.finished.map((a) => a.id)).toEqual(['h']);
		expect(groups.yoursNeedingYou.map((a) => a.id)).toEqual(['b']);
	});
});

function failedRun(error: string): Partial<Applet> {
	return { last_run: { status: 'error', started_at: at('2026-10-06T06:00:00'), records_processed: null, error } };
}

describe('appletsLead', () => {
	const fine = applet({ id: 'a', name: 'Search index', origin: 'system', last_run: { status: 'success', started_at: at('2026-10-06T11:56:00'), records_processed: null, error: null } });

	it('names the last run when nothing needs you', () => {
		expect(appletsLead([fine, applet({ id: 'b' })], NOW)).toEqual({
			sentence: 'Everything is running on schedule.',
			sub: 'The last run was Search index, 4 minutes ago.',
		});
	});

	it('names one or two applets that need you, and why', () => {
		const garden = applet({ id: 'g', name: 'Garden watering', ...failedRun('timed out') });
		const invoices = applet({ id: 'i', name: 'Studio invoices', ...failedRun('Gmail is disconnected') });
		expect(appletsLead([garden, fine], NOW)).toEqual({
			sentence: 'Garden watering needs you.',
			sub: "Garden watering didn't finish its last run. Everything else is running on schedule.",
		});
		expect(appletsLead([invoices, garden, fine], NOW)).toEqual({
			sentence: 'Garden watering and Studio invoices need you.',
			sub: "Garden watering didn't finish its last run, and Studio invoices needs you to reconnect Gmail. Everything else is running on schedule.",
		});
	});

	it('stops naming them at three', () => {
		const three = ['A', 'B', 'C'].map((n) => applet({ id: n, name: n, ...failedRun('timed out') }));
		expect(appletsLead(three, NOW).sentence).toBe('Some of what your server runs needs you.');
		expect(appletsLead(three, NOW).sub).not.toMatch(/Everything else/);
	});
});
