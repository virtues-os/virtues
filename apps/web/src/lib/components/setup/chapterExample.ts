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
	{ title: 'Prep school', from: SEED_AGE, to: 18 },
	{ title: 'Wellington', from: 18, to: 25 },
	{ title: 'Life with Maren', from: 25, to: 32 },
	{ title: 'The cabin', from: 32, to: SAMPLE_SPAN },
];
