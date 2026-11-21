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
import { isYours, kindOf, startsItself } from './kind';
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

/** The server's tasks that one connected account or device brought with it. */
export interface AccountTasks {
	kind: 'credential' | 'device';
	/** The credential's or the device's id. */
	id: string;
	tasks: Applet[];
}

export interface ServerTaskGroups {
	/** One entry per connected account or device, in the order first met. */
	accounts: AccountTasks[];
	/** The server's own tasks that start themselves. */
	server: Applet[];
	/** The server's own tasks that nothing starts: they run when asked. */
	whenAsked: Applet[];
	/** The skills that came with the server. */
	skills: Applet[];
	/** One-time ones that already ran, yours included. */
	finished: Applet[];
	/** Things of yours in Sites that need you. Your tasks that repeat say so in Tasks. */
	yoursNeedingYou: Applet[];
}

/**
 * Whether the Server tasks page lists it: one of the server's own tasks, or
 * a task of the assistant's that came with the server and has no row in
 * Tasks. That is one that is off (the morning examen until you turn it on),
 * and this page is where you turn it on. Once on, it shows in Tasks.
 */
function isServerTask(a: Applet): boolean {
	const kind = kindOf(a);
	return kind === 'server_task' || (kind === 'assistant_task' && !isYours(a) && !a.task_id);
}

/** The server's tasks that still run. */
export function serverTasks(applets: Applet[]): Applet[] {
	return applets.filter((a) => !a.archived_at && isServerTask(a));
}

/**
 * The groups the Server tasks page lists, each sorted by name. A task that
 * needs you stays in its group, so the list keeps its order when something
 * breaks.
 */
export function groupServerTasks(applets: Applet[], now = Date.now()): ServerTaskGroups {
	const byName = (list: Applet[]) => [...list].sort((a, b) => a.name.localeCompare(b.name));
	const accounts: AccountTasks[] = [];
	const own: Applet[] = [];
	for (const a of byName(serverTasks(applets))) {
		const id = a.credential_id ?? a.device_id;
		if (!id) {
			own.push(a);
			continue;
		}
		const kind = a.credential_id ? 'credential' : 'device';
		const group = accounts.find((g) => g.kind === kind && g.id === id);
		if (group) group.tasks.push(a);
		else accounts.push({ kind, id, tasks: [a] });
	}
	return {
		accounts,
		server: own.filter(startsItself),
		whenAsked: own.filter((a) => !startsItself(a)),
		skills: byName(applets.filter((a) => !a.archived_at && !isYours(a) && kindOf(a) === 'skill')),
		finished: byName(applets.filter((a) => a.archived_at)),
		yoursNeedingYou: byName(applets.filter((a) => !a.archived_at && isYours(a) && !isServerTask(a) && !a.task_id && needsYou(a, now))),
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

/** The sentence the Jobs page opens with, and the one under it. */
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
				: { sentence: 'Nothing runs on your server yet.', sub: 'Its tasks appear here once it has run one.' };
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

	return { sentence: 'Some of what your server runs needs you.', sub: `Each one below says what happened.${others}` };
}
