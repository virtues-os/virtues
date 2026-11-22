/**
 * What the Inbox shows: one list of the things that ask something of you,
 * built from what the app already has. That is your open tasks, what your
 * server noticed (notes) and what your server needs from you (attention).
 * Each gets a ring, which says how soon it matters, and a kind, which picks
 * its mark. Every view of the Inbox reads this one list.
 */
import type { AttentionItem, Note, Task } from '$lib/api/client';
import { dueGroup, dueLabel } from '$lib/tasks/due';
import type { Question } from './questions';

/** How soon a thing matters, nearest first. */
export const RINGS = ['Now', 'Today', 'This week', 'Later'] as const;
export type Ring = 0 | 1 | 2 | 3;

/**
 * How much a thing asks of you, as one of six levels, strongest first. Two
 * facts set it: the tier of the thread it sits in, and how near its day is.
 *
 * A thread you list is the first tier. An unlisted thread is the second: a
 * lesser concern your server keeps for you, whose things stay toward the
 * noise end until their day comes. A thing in no thread counts as second
 * tier too. Once a thing is due, its tier stops mattering.
 *   0  a result that waits for your answer, or something your server
 *      noticed outside an unlisted thread
 *   1  anything due today or earlier
 *   2  first tier, due this week
 *   3  first tier, due later or with no day
 *   4  second tier, due this week
 *   5  second tier, due later or with no day, and also what your server
 *      needs and a request that is still running
 *
 * Your server stores no judgment of how much a thing matters, so the
 * levels rest only on facts it does store.
 */
type Level = 0 | 1 | 2 | 3 | 4 | 5;

/** The level of a thing that is nobody's result to accept: by its ring, then its tier. */
function levelOf(ring: Ring, firstTier: boolean): Level {
	if (ring <= 1) return 1;
	return firstTier ? (ring === 2 ? 2 : 3) : ring === 2 ? 4 : 5;
}

/**
 * What kind of thing it is:
 *   needs     it waits on you: your server needs something, or a request
 *             has a result for you to accept
 *   insight   your server noticed something
 *   task      a task of yours
 *   waiting   a request the assistant has not finished
 */
export type ItemKind = 'needs' | 'insight' | 'task' | 'waiting';

export interface InboxItem {
	key: string;
	kind: ItemKind;
	ring: Ring;
	/**
	 * Where it falls from signal (0) to noise (1). Everything at level 0 is
	 * at 0. The rest are spread evenly up to 1, by level and then nearest
	 * first, so a control moved toward noise brings them in one at a time.
	 */
	noise: number;
	title: string;
	/** When it is for, or where it came from, in a few words. */
	meta: string;
	/** The active thread it sits in, or null. */
	threadId: string | null;
	task?: Task;
	note?: Note;
	attention?: AttentionItem;
	question?: Question;
}

/**
 * A question from your server as a thing in the list. It waits on you, so
 * it wears that mark. The signal control thins what your threads ask of
 * you, and a question is not one of those, so the list always shows it.
 * The other views do not draw it.
 */
export function questionItem(q: Question): InboxItem {
	return { key: `ask:${q.key}`, kind: 'needs', ring: 0, noise: 0, title: q.text, meta: q.about, threadId: null, question: q };
}

/** Each kind's mark, in px across. */
export const MARK_SIZE: Record<ItemKind, number> = { needs: 14, insight: 22, task: 14, waiting: 12 };

const DAY = 86_400_000;
const KIND_ORDER: Record<ItemKind, number> = { needs: 0, insight: 1, task: 2, waiting: 3 };

/** A result to accept is for now. Otherwise a task sits on the ring of its due day. */
export function taskRing(t: Pick<Task, 'status' | 'due_on'>, now: Date): Ring {
	if (t.status === 'ready') return 0;
	const group = dueGroup(t.due_on, now);
	return group === 'today' ? 1 : group === 'tomorrow' || group === 'week' ? 2 : 3;
}

/** A note sits on the ring of the day it stops mattering. */
export function noteRing(n: Pick<Note, 'until'>, now: Date): Ring {
	const until = new Date(n.until).getTime();
	if (Number.isNaN(until)) return 2;
	const endOfToday = new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1).getTime();
	return until <= endOfToday ? 1 : until <= endOfToday + 6 * DAY ? 2 : 3;
}

/** The thread a note is listed under: its first thread, or its first task's thread. */
function noteThread(n: Note, tasks: Task[]): string | null {
	for (const subject of n.subjects) {
		const thread = subject.match(/^\/thread\/([^/?#]+)/);
		if (thread) return thread[1];
		const task = subject.match(/^\/task\/([^/?#]+)/);
		const home = task && tasks.find((t) => t.id === task[1])?.thread_id;
		if (home) return home;
	}
	return null;
}

export function buildItems(src: {
	tasks: Task[];
	notes: Note[];
	attention: AttentionItem[];
	/** Ids of the active threads. A thing in any other thread shows with no thread. */
	active: ReadonlySet<string>;
	/** Ids of the active threads that are unlisted. */
	unlisted?: ReadonlySet<string>;
	now?: Date;
}): InboxItem[] {
	const now = src.now ?? new Date();
	const home = (id: string | null) => (id && src.active.has(id) ? id : null);
	const firstTier = (id: string | null) => !!id && !src.unlisted?.has(id);
	const items: InboxItem[] = [];
	const level = new Map<string, Level>();
	const add = (it: InboxItem, l: Level) => {
		items.push(it);
		level.set(it.key, l);
	};
	for (const a of src.attention) {
		add({ key: `attention:${a.key}`, kind: 'needs', ring: 0, noise: 0, title: a.title, meta: 'Your server', threadId: null, attention: a }, 5);
	}
	for (const n of src.notes) {
		const threadId = home(noteThread(n, src.tasks));
		const ring = noteRing(n, now);
		add(
			{ key: `note:${n.key}`, kind: 'insight', ring, noise: 0, title: n.text, meta: n.when, threadId, note: n },
			threadId && src.unlisted?.has(threadId) ? levelOf(ring, false) : 0,
		);
	}
	for (const t of src.tasks) {
		const ready = t.status === 'ready';
		// An open task for the assistant is a request. `isRequest` in
		// tasks/requests says the same, but that module loads the toasts,
		// and this one stays free of components so it can be unit tested.
		const request = t.assignee === 'assistant';
		const ring = taskRing(t, now);
		const threadId = home(t.thread_id);
		add(
			{
				key: `task:${t.id}`,
				kind: ready ? 'needs' : request ? 'waiting' : 'task',
				ring,
				noise: 0,
				title: t.title,
				meta: ready ? 'Result ready' : t.due_on ? dueLabel(t.due_on, t.due_at, now) : 'No date',
				threadId,
				task: t,
			},
			ready ? 0 : request ? 5 : levelOf(ring, firstTier(threadId)),
		);
	}
	items.sort(
		(a, b) =>
			a.ring - b.ring ||
			KIND_ORDER[a.kind] - KIND_ORDER[b.kind] ||
			(a.task?.due_on ?? '9').localeCompare(b.task?.due_on ?? '9') ||
			a.title.localeCompare(b.title),
	);
	// The sort is stable, so within a level the order above stands: nearest first.
	const rest = items.filter((it) => level.get(it.key) !== 0).sort((a, b) => level.get(a.key)! - level.get(b.key)!);
	rest.forEach((it, i) => (it.noise = (i + 1) / rest.length));
	return items;
}
