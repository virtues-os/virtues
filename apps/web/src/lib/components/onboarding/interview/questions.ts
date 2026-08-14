/**
 * "In your own words" — the interview.
 *
 * The document this produces is the one thing on the box nobody can observe.
 * Everything else is derived from the record; this is authored, and it is what
 * lets the box know what someone is FOR rather than merely what they did.
 *
 * PROSE, NOT FIELDS. An earlier design decomposed a life into a schema —
 * entries with kinds and facets and salience. Two things killed it: the
 * consumer is a language model, which reads paragraphs better than it reads
 * JSON containing paragraphs; and empty fields nag, so a half-filled identity
 * schema reads as personal failure. The structure lives in the ASKING. What
 * gets stored is what the person wrote.
 *
 * ORDERED BY RISING EXPOSURE. The first questions are enumerable and cost
 * nothing — where you lived, what you were into. That is not throat-clearing:
 * momentum is what makes question five answerable. Loss comes directly after
 * the people question so it arrives as the natural next thing rather than an
 * ambush, and the standing-orders question comes LAST, where "what of this
 * should stay unspoken?" reads as protection instead of interrogation.
 *
 * GUIDANCE IS PER-QUESTION AND NEVER BLOCKS. A word target belongs on "list
 * every hobby you've had". It does not belong on "who did you lose" — a red
 * light there would make grief a quota, and that single detail would define
 * this product for the person it happened to.
 */

export interface Question {
	/** Stable across rewordings and reorderings — answers are filed by this. */
	id: string;
	/** The interrogative it covers, shown as a quiet marker. */
	facet: string;
	prompt: string;
	/** Why it is asked — shown to the person, because being told what a question
	 *  is for makes it easier to answer well and harder to feel examined. */
	purpose: string;
	/** Optional nudge under the field. Never a rule. */
	hint?: string;
	/** Soft word target. `null` means this question is never counted. */
	target: number | null;
	/** Which movement of the document it feeds. */
	tense: 'past' | 'present' | 'future' | 'standing';
}

export const QUESTIONS: Question[] = [
	{
		id: 'places',
		facet: 'WHERE',
		prompt: 'Where have you lived, and when?',
		purpose:
			'The timeline everything else hangs on. Once the box knows your eras by place, every later answer becomes datable.',
		hint: 'A list is fine. City, roughly when, and how you ended up there if it matters.',
		target: 200,
		tense: 'past'
	},
	{
		id: 'chapters',
		facet: 'WHEN',
		prompt: 'What were the chapters of your life — and what changed between them?',
		purpose:
			'Your own periodization, which is more useful than any calendar. The changepoints matter as much as the chapters.',
		hint: 'Name them however you would in conversation. The turns are the interesting part.',
		target: 400,
		tense: 'past'
	},
	{
		id: 'interests',
		facet: 'WHAT',
		prompt: 'Every hobby, obsession and interest you have ever had — and when.',
		purpose:
			'What you reach for, and what you keep. The pattern of what you abandon is as telling as what stuck.',
		hint: 'Go long and go trivial. The abandoned ones count.',
		target: 300,
		tense: 'past'
	},
	{
		id: 'people',
		facet: 'WHO',
		prompt: 'In each era, who were the most significant people?',
		purpose:
			'The relational spine. These become the people your box knows by name, because you named them — never because it guessed.',
		hint: 'Who they were to you, and what they were like.',
		target: 400,
		tense: 'past'
	},
	{
		id: 'loss',
		facet: 'WHO',
		prompt: 'Who, or what, have you lost?',
		purpose:
			'Grief shapes more than almost anything and no amount of data will ever reveal it. Written here so the box understands, and so it knows what to step around.',
		hint: 'As much or as little as you want. This one has no length worth aiming for.',
		target: null,
		tense: 'past'
	},
	{
		id: 'telos',
		facet: 'WHY',
		prompt:
			'What worldview or faith do you hold, reject, or find yourself still working out?',
		purpose:
			'The why underneath the rest. Not to be argued with or optimized — to be understood, so nothing it says cuts across the grain of what you believe.',
		hint: 'Including "I genuinely do not know" — that is an answer, and a useful one.',
		target: 300,
		tense: 'present'
	},
	{
		id: 'pride',
		facet: 'HOW',
		prompt: 'What are you proud of in yourself — in how you go about things?',
		purpose:
			'Virtues, asked in the only way anyone can answer. "List your virtues" gets nothing; this gets the truth.',
		hint: 'How you do things, not only what you have done.',
		target: 300,
		tense: 'present'
	},
	{
		id: 'now',
		facet: 'NOW',
		prompt: 'What are you working on at the moment? Five or ten things.',
		purpose:
			'What is live. The box will infer plenty about your days, but not which of them you actually care about.',
		hint: 'Work, body, house, people, projects, the thing you keep meaning to start.',
		target: 200,
		tense: 'present'
	},
	{
		id: 'vices',
		facet: 'VICES',
		prompt:
			'What do you keep starting and not finishing? And which pull is strongest for you — money, power, pleasure, or fame?',
		purpose:
			'The pattern you are up against. Written by you, it is self-knowledge; guessed by a machine, it would be an accusation — which is why it is asked rather than derived.',
		hint: 'The four are Arthur Brooks’. Most people know their answer immediately.',
		target: 300,
		tense: 'present'
	},
	{
		id: 'ambitions',
		facet: 'FUTURE',
		prompt: 'A month from now, three years, ten years — what do you want out of life?',
		purpose:
			'Direction. This is the part that lets the box be useful about decisions rather than only accurate about the past.',
		hint: 'The one-month horizon is the one that changes what next week looks like.',
		target: 400,
		tense: 'future'
	},
	{
		id: 'novelty',
		facet: 'DELTA',
		prompt: 'What makes you different from everyone else you have met?',
		purpose:
			'An AI assumes you are the average person until told otherwise — everything it does not know about you, it fills in with the mean. This question is that gap, which makes it the most valuable one here.',
		hint: 'The things people find surprising, or that you have stopped mentioning because nobody relates.',
		target: 300,
		tense: 'present'
	},
	{
		id: 'standing_orders',
		facet: 'RULES',
		prompt: 'Of everything you have written — what should it never bring up?',
		purpose:
			'Some things matter enormously and should still never be raised unprompted. A lost family member, an addiction in recovery, a pet. These become rules, not suggestions.',
		hint: 'Plainly: "never suggest bars", "do not mention my father unless I do".',
		target: null,
		tense: 'standing'
	}
];

/** Words, counted the way a person would count them. */
export function wordCount(s: string): number {
	const t = s.trim();
	return t ? t.split(/\s+/).length : 0;
}
