/**
 * Where a task stands, as a sentence with someone doing something in it,
 * for the top of the task beside a list. The words a row shows are in
 * `requests.ts`; these are the longer form a reader starts from.
 */
import type { Task } from '$lib/api/client';
import { dueLabel, fromIsoDay, isoDay } from '$lib/tasks/due';
import { isRepeating } from '$lib/tasks/requests';

/** A start this far past its time has not happened, rather than being about to. */
const LATE_MS = 5 * 60_000;

function clock(at: Date): string {
	return at.toLocaleTimeString('en-US', { hour: 'numeric', minute: '2-digit' });
}

/**
 * The task's state in one sentence, or null for a task that repeats, whose
 * applet says when it runs.
 */
export function standing(t: Task, now: number = Date.now()): string | null {
	if (t.status === 'done') return 'You finished this.';
	if (t.status === 'dismissed') return 'You put this away.';
	if (isRepeating(t)) return null;
	if (t.assignee !== 'assistant') return 'This is yours.';
	if (t.status === 'ready') return 'The assistant finished this. Read what it left, then accept it or reply below.';
	if (t.status === 'active') return 'The assistant is working on this.';
	if (t.next_run_at) {
		const at = new Date(t.next_run_at);
		const ahead = at.getTime() - now;
		if (ahead > 60_000) {
			// "Today" and "Tomorrow" sit mid-sentence; a weekday keeps its capital.
			const when = dueLabel(isoDay(at), at.toISOString(), new Date(now)).replace(/^(Today|Tomorrow)/, (w) => w.toLowerCase());
			return `The assistant has this and starts ${when}.`;
		}
		if (ahead < -LATE_MS) return `The assistant has this. It was set to start at ${clock(at)} and hasn't started yet.`;
		return 'The assistant has this and starts in a moment.';
	}
	return "The assistant has this but hasn't started it.";
}

/** What the box under a task asks for, by what writing there does. */
export function chatPrompt(t: Task): string {
	if (t.assignee === 'assistant' && t.status === 'ready') return 'Reply to the assistant. It works on your answer next.';
	if (t.assignee === 'assistant' && !isRepeating(t) && t.status !== 'done' && t.status !== 'dismissed') {
		return 'Tell the assistant what it should know. It reads this before it starts.';
	}
	return 'Write a note about this task';
}

/**
 * The due day as a fact you can change: "Due Friday", or for a day gone by
 * the date itself, "Was due Thu, Nov 20". A count of days since reads as
 * an alarm, and someone in recovery reads this page too.
 */
export function dueFact(due_on: string, due_at: string | null, now: Date = new Date()): string {
	const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
	const day = fromIsoDay(due_on);
	if (day.getTime() >= today.getTime()) return `Due ${dueLabel(due_on, due_at, now)}`;
	const date = day.toLocaleDateString('en-US', {
		weekday: 'short',
		month: 'short',
		day: 'numeric',
		year: day.getFullYear() !== today.getFullYear() ? 'numeric' : undefined,
	});
	return `Was due ${date}`;
}
