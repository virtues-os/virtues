/**
 * What excites you most about Virtues.
 * Setup's Names step asks it and Settings · Profile changes or clears it.
 * The ids are the server's `api::profile::StartWith`.
 */
import type { Profile } from '$lib/api/client';

export type StartWith = NonNullable<Profile['start_with']>;

export interface StartWithChoice {
	id: StartWith;
	/** The answer, starting with what you would be doing. */
	title: string;
	/** One line on what that means in Virtues. */
	line: string;
}

export const START_WITH: readonly StartWithChoice[] = [
	{
		id: 'days',
		title: 'Reading my life as a story',
		line: 'A page for every day, written from what actually happened.',
	},
	{
		id: 'patterns',
		title: 'Seeing patterns in my sleep, health and time',
		line: 'Charts from my own data, set against what is usual for me.',
	},
	{
		id: 'memory',
		title: 'Remembering everything',
		line: 'Ask about anything I did, said or saw, and get the moment back.',
	},
	{
		id: 'habit',
		title: 'Breaking a vice, or building a virtue',
		line: 'See what tends to come before it, without streaks or scores.',
	},
	{
		id: 'ownership',
		title: 'Taking my life back from big tech',
		line: 'My data on a computer I own, with no ads, feeds or selling.',
	},
];
