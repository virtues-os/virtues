/**
 * Where an applet stands, in words: whether it needs you and why, what fixes
 * it, and the line Settings · Applets writes under its name. The Applets page
 * in Home uses the same problem and fix, so the two pages say the same thing.
 *
 * An applet needs you when it's on and its last run failed, or a due run is
 * more than an hour late (`needsYou`). A late run is said as what happened
 * ("Hasn't run since yesterday at 6:00 AM"), never as being overdue.
 */
import type { Applet } from '$lib/api/client';
import { whenInWords } from '$lib/utils/dateUtils';
import { needsYou } from './days';
import { describeTriggers, errorHeadline, runFix } from './palette';

export interface Trouble {
	/** `signin`: the run failed because an account's sign-in lapsed. */
	kind: 'failed' | 'signin' | 'late';
	/** What happened, without a subject or a final period. */
	text: string;
}

/** Why an applet needs you, or null when it doesn't. */
export function troubleOf(a: Applet, now = Date.now()): Trouble | null {
	if (!needsYou(a, now)) return null;
	const run = a.last_run;
	if (run?.status === 'error') {
		// The run's own reason ("Gmail is disconnected") says more than when.
		const reason = run.error ? errorHeadline(run.error, 120) : null;
		const when = run.started_at ? ` ${whenInWords(run.started_at, now)}` : '';
		return { kind: fixOf(a) ? 'signin' : 'failed', text: reason ?? `Its last run didn't finish${when}` };
	}
	const last = run?.started_at;
	return { kind: 'late', text: last ? `Hasn't run since ${whenInWords(last, now)}` : "Hasn't run yet" };
}

/** What went wrong, in a sentence, or null when nothing did. */
export function problemOf(a: Applet, now = Date.now()): string | null {
	return troubleOf(a, now)?.text ?? null;
}

/** The fix when running again can't help: reconnecting an account whose sign-in lapsed. */
export function fixOf(a: Applet): { label: string; href: string } | null {
	return a.last_run?.status === 'error' ? runFix(a.last_run.error ?? null) : null;
}

/** Whether "Run now" can work. The server refuses it unless the applet lists the manual trigger. */
export function canRun(a: Applet): boolean {
	return a.enabled && !a.archived_at && a.triggers.includes('manual');
}

export type AppletAction = { kind: 'fix'; label: string; href: string } | { kind: 'run'; label: 'Retry' | 'Run now' };

/** The button beside an applet that needs you, or null when there is nothing to press. */
export function actionOf(a: Applet, now = Date.now()): AppletAction | null {
	const trouble = troubleOf(a, now);
	if (!trouble) return null;
	const fix = fixOf(a);
	if (fix) return { kind: 'fix', ...fix };
	if (!canRun(a)) return null;
	return { kind: 'run', label: trouble.kind === 'late' ? 'Run now' : 'Retry' };
}

/** A sentence ends in one period, even when the text it ends with brought its own. */
function sentence(text: string): string {
	return /[.!?…]$/.test(text) ? text : `${text}.`;
}

/**
 * The line under an applet's name: when it runs, then how its last run went.
 * "Daily at 7am. Ran today at 7:00 AM."
 */
export function appletLine(a: Applet, now = Date.now()): string {
	if (a.archived_at) return `Finished ${whenInWords(a.archived_at, now)}.`;
	if (!a.enabled) return 'Off.';
	const when = describeTriggers(a);
	const trouble = troubleOf(a, now);
	if (trouble) return `${when}. ${sentence(trouble.text)}`;
	const run = a.last_run;
	if (run?.status === 'running') return `${when}. Running now.`;
	if (run?.started_at) {
		const at = whenInWords(run.started_at, now);
		if (run.status === 'budget_exceeded') return `${when}. Stopped at its spending limit ${at}.`;
		return `${when}. Ran ${at}.`;
	}
	if (a.next_due_at && new Date(a.next_due_at).getTime() > now) {
		return `${when}. Next run ${whenInWords(a.next_due_at, now)}.`;
	}
	return `${when}. Hasn't run yet.`;
}

export interface AppletGroups {
	needsYou: Applet[];
	/** Made by you or by your assistant. */
	yours: Applet[];
	/** Shipped with the server. */
	server: Applet[];
	/** Came with an account or a device you connected. */
	connected: Applet[];
	/** One-time applets that already ran. */
	finished: Applet[];
}

/** The groups Settings · Applets lists, each sorted by name. An applet that needs you is listed there only. */
export function groupApplets(applets: Applet[], now = Date.now()): AppletGroups {
	const byName = (list: Applet[]) => [...list].sort((a, b) => a.name.localeCompare(b.name));
	const living = applets.filter((a) => !a.archived_at);
	const rest = living.filter((a) => !needsYou(a, now));
	return {
		needsYou: byName(living.filter((a) => needsYou(a, now))),
		yours: byName(rest.filter((a) => a.origin === 'user' || a.origin === 'ai')),
		server: byName(rest.filter((a) => a.origin === 'system')),
		connected: byName(rest.filter((a) => a.origin === 'source')),
		finished: byName(applets.filter((a) => a.archived_at)),
	};
}

/** One applet's trouble as a clause with the applet as its subject. */
function clause(a: Applet, trouble: Trouble, now: number): string {
	switch (trouble.kind) {
		case 'signin': {
			const account = fixOf(a)?.label.replace(/^Reconnect\s*/, '') || 'its account';
			return `${a.name} needs you to reconnect ${account}`;
		}
		case 'failed':
			return `${a.name} didn't finish its last run`;
		case 'late': {
			const last = a.last_run?.started_at;
			return last ? `${a.name} hasn't run since ${whenInWords(last, now)}` : `${a.name} hasn't run yet`;
		}
	}
}

/** The sentence Settings · Applets opens with, and the one under it. */
export function appletsLead(applets: Applet[], now = Date.now()): { sentence: string; sub: string } {
	const living = applets.filter((a) => !a.archived_at);
	const needing = living
		.map((a) => ({ a, trouble: troubleOf(a, now) }))
		.filter((n): n is { a: Applet; trouble: Trouble } => n.trouble !== null)
		.sort((x, y) => x.a.name.localeCompare(y.a.name));
	const others = living.length > needing.length ? ' Everything else is running on schedule.' : '';

	if (needing.length === 0) {
		const last = living
			.filter((a) => a.last_run?.started_at)
			.reduce<Applet | null>((newest, a) => (!newest || a.last_run!.started_at! > newest.last_run!.started_at! ? a : newest), null);
		if (!last) {
			return living.length
				? { sentence: 'Everything is running on schedule.', sub: 'Nothing has run yet.' }
				: { sentence: 'Nothing runs on your server yet.', sub: 'Applets you make in chat appear here.' };
		}
		return {
			sentence: 'Everything is running on schedule.',
			sub: `The last run was ${last.name}, ${whenInWords(last.last_run!.started_at!, now)}.`,
		};
	}

	if (needing.length === 1) {
		const [{ a, trouble }] = needing;
		return { sentence: `${a.name} needs you.`, sub: `${clause(a, trouble, now)}.${others}` };
	}

	if (needing.length === 2) {
		const [x, y] = needing;
		return {
			sentence: `${x.a.name} and ${y.a.name} need you.`,
			sub: `${clause(x.a, x.trouble, now)}, and ${clause(y.a, y.trouble, now)}.${others}`,
		};
	}

	return { sentence: 'Some of your applets need you.', sub: `Each one under Needs you says what happened.${others}` };
}
