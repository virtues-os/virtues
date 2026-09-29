/**
 * The example life the Chapters step shows: in its intro, drawn on the
 * line, and later in "See an example", for whoever is stuck on their own.
 * Ages, 0 = birth. Fictional, and personal in the way theirs will be: a
 * school, a place, a person, a home. The last one is named, never "Now",
 * which is the line's end, not a chapter.
 */

/** Where Chapters seeds "Childhood" to, and where the example's first ends. */
export const SEED_AGE = 13;

export const SAMPLE_SPAN = 38;

export const SAMPLE: { title: string; from: number; to: number }[] = [
	{ title: 'Childhood', from: 0, to: SEED_AGE },
	{ title: 'The band years', from: SEED_AGE, to: 19 },
	{ title: 'Chicago', from: 19, to: 26 },
	{ title: 'Married', from: 26, to: 33 },
	{ title: 'The farm', from: 33, to: SAMPLE_SPAN },
];
