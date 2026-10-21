/**
 * One dot per day, or per week, for an applet's recent runs.
 *
 * An applet that runs daily or more often gets a dot a day. One that runs
 * once a week or less gets a dot a week, because seven days of a weekly
 * applet hold at most one run, and its failures fell outside the window.
 * Each dot takes the worst thing that happened in it: one failure among 24
 * hourly runs still makes that day red.
 */
import type { Applet, RunDay } from '$lib/api/client';

/** What a day's dot says. `none` means no run started that day. */
export type DayState = 'failed' | 'stopped' | 'ran' | 'quiet' | 'missed' | 'none';

/** What each dot means, for its tooltip. */
export const DAY_LABEL: Record<DayState, string> = {
	failed: 'failed',
	stopped: 'stopped at its spending limit',
	ran: 'ran',
	quiet: 'skipped',
	missed: "was due and didn't run",
	none: 'no runs'
};

/** Run counts by applet, then day, then status. */
export type DayIndex = Map<string, Map<string, Map<string, number>>>;

/** How far past its due time a run can be before the day counts as missed.
 *  The same hour the needs-attention strip allows. */
const OVERDUE_GRACE_MS = 60 * 60 * 1000;

/** `YYYY-MM-DD` for a date in the viewer's local timezone. */
export function dayKey(d: Date): string {
	const y = d.getFullYear();
	const m = String(d.getMonth() + 1).padStart(2, '0');
	const day = String(d.getDate()).padStart(2, '0');
	return `${y}-${m}-${day}`;
}

/** The last `count` days, oldest first, ending today. */
export function lastDays(count: number, today = new Date()): string[] {
	const out: string[] = [];
	for (let i = count - 1; i >= 0; i--) {
		const d = new Date(today);
		d.setDate(d.getDate() - i);
		out.push(dayKey(d));
	}
	return out;
}

/** How many days one dot covers for this schedule: 7 for an applet that runs
 *  on one day of the week or of the month, 1 for everything else. */
export function spanDays(schedule: string | null): 1 | 7 {
	const fields = schedule?.trim().split(/\s+/) ?? [];
	if (fields.length !== 5 && fields.length !== 6) return 1;
	const [day, , dow] = fields.slice(-3);
	const single = (f: string) => /^[0-9A-Za-z]+$/.test(f);
	return (day === '*' && single(dow)) || (day !== '*' && single(day)) ? 7 : 1;
}

/** The days behind each of an applet's 7 recent dots, oldest first. */
export function recentSpans(schedule: string | null, today = new Date()): string[][] {
	const span = spanDays(schedule);
	const days = lastDays(7 * span, today);
	return Array.from({ length: 7 }, (_, i) => days.slice(i * span, (i + 1) * span));
}

/** Group the per-day rows by applet, so each applet's lookup is cheap. */
export function indexRunDays(rows: RunDay[]): DayIndex {
	const byApplet: DayIndex = new Map();
	for (const r of rows) {
		const days = byApplet.get(r.applet_id) ?? new Map<string, Map<string, number>>();
		const statuses = days.get(r.day) ?? new Map<string, number>();
		statuses.set(r.status, (statuses.get(r.status) ?? 0) + r.runs);
		days.set(r.day, statuses);
		byApplet.set(r.applet_id, days);
	}
	return byApplet;
}

/** The state of one day, from that day's run counts by status. */
export function stateOf(statuses: Map<string, number> | undefined): DayState {
	if (!statuses || statuses.size === 0) return 'none';
	if (statuses.has('error')) return 'failed';
	// Stopped at a spending limit the owner set: working as configured, but
	// worth seeing, so it isn't drawn as a plain success.
	if (statuses.has('budget_exceeded')) return 'stopped';
	if (statuses.has('success') || statuses.has('running')) return 'ran';
	return 'quiet';
}

/**
 * The dots for one applet, oldest first, one per span of days. The newest
 * becomes `missed` when the scheduler says a run was due more than an hour ago
 * and nothing ran in it. Earlier missed spans can't be told apart from
 * unscheduled ones: the box keeps only the latest slot (`next_due_at`), not a
 * history of slots.
 */
export function appletSpans(
	applet: Applet,
	index: DayIndex,
	spans: string[][],
	now = Date.now()
): DayState[] {
	const mine = index.get(applet.id);
	const states = spans.map((span) => {
		const merged = new Map<string, number>();
		for (const d of span) for (const [status, n] of mine?.get(d) ?? []) merged.set(status, (merged.get(status) ?? 0) + n);
		return stateOf(merged);
	});
	const last = states.length - 1;
	if (
		last >= 0 &&
		states[last] === 'none' &&
		applet.enabled &&
		!applet.archived_at &&
		applet.next_due_at &&
		now - new Date(applet.next_due_at).getTime() > OVERDUE_GRACE_MS
	) {
		states[last] = 'missed';
	}
	return states;
}

/** One dot per day: `appletSpans` with a span of one. */
export function appletDays(applet: Applet, index: DayIndex, days: string[], now = Date.now()): DayState[] {
	return appletSpans(applet, index, days.map((d) => [d]), now);
}

/**
 * Whether an applet needs you now: it's on, and its last run failed or a due
 * run is more than an hour late. Stopping at a spending limit doesn't count:
 * the applet did what it was told.
 */
export function needsYou(applet: Applet, now = Date.now()): boolean {
	if (!applet.enabled || applet.archived_at) return false;
	if (applet.last_run?.status === 'error') return true;
	return Boolean(applet.next_due_at && now - new Date(applet.next_due_at).getTime() > OVERDUE_GRACE_MS);
}

/** A score for sorting by reliability: higher is worse. */
export function reliabilityScore(states: DayState[]): number {
	const weight: Record<DayState, number> = { failed: 3, missed: 3, stopped: 1, ran: 0, quiet: 0, none: 0 };
	return states.reduce((sum, s) => sum + weight[s], 0);
}

/** How many of an applet's runs ended in each status over the given days. */
export function countRuns(index: DayIndex, appletId: string, days: string[]): Map<string, number> {
	const out = new Map<string, number>();
	const mine = index.get(appletId);
	for (const d of days) {
		for (const [status, n] of mine?.get(d) ?? []) out.set(status, (out.get(status) ?? 0) + n);
	}
	return out;
}

/** The most runs an applet started on any one day. One or fewer reads as a
 *  calendar; more needs a row per day. */
export function busiestDay(index: DayIndex, appletId: string): number {
	let most = 0;
	for (const statuses of index.get(appletId)?.values() ?? []) {
		let n = 0;
		for (const runs of statuses.values()) n += runs;
		most = Math.max(most, n);
	}
	return most;
}

/** One sentence about the dots shown, worst news first. */
export function recentSummary(states: DayState[], counts: Map<string, number>, unit: 'days' | 'weeks' = 'days'): string {
	const span = `in the last ${states.length} ${unit}`;
	const failed = counts.get('error') ?? 0;
	if (failed > 0) return `${failed} failed ${span}`;
	if (states.at(-1) === 'missed') return unit === 'days' ? "Didn't run today when it was due" : "Didn't run when it was due";
	const stopped = counts.get('budget_exceeded') ?? 0;
	if (stopped > 0) {
		return `Stopped at its spending limit ${stopped === 1 ? 'once' : `${stopped} times`} ${span}`;
	}
	let total = 0;
	for (const n of counts.values()) total += n;
	return total > 0 ? `No problems ${span}` : `No runs ${span}`;
}
