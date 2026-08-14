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
	/** One line under the prompt — what it is for. */
	purpose: string;
	/** The longer answer to "why are you asking me this", behind a disclosure.
	 *
	 *  Worth the words. These questions ask for grief, vice and faith, and the
	 *  difference between answering them well and skipping them is usually
	 *  whether the person believes the asking is principled. Explaining the
	 *  MECHANISM — what the box actually does with it, or why the question
	 *  works — earns more than any amount of reassurance. */
	why: string;
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
		purpose: 'The timeline everything else hangs on. Once the box knows your eras by place, every later answer becomes datable.',
		why: 'Timestamps are not memory. The box will hold hundreds of thousands of dated events and understand none of them until it knows that 2019 to 2023 was Austin. Then a photograph, a message and a receipt from that stretch belong to the same chapter of your life rather than the same quarter of a year. Moves are also one of the few universally legible turning points — where you were usually explains who you knew.',
		hint: 'A list is fine. City, roughly when, and how you ended up there if it matters.',
		target: 200,
		tense: 'past'
	},
	{
		id: 'chapters',
		facet: 'WHEN',
		prompt: 'What were the chapters of your life — and what changed between them?',
		purpose: 'Your own periodization, which is more useful than any calendar. The changepoints matter as much as the chapters.',
		why: 'Everyone privately divides their life into periods, and nobody\'s match the calendar. A box that knows yours can say “that was during the Boston years” instead of “in 2017” — the difference between a filing system and a memory. The changepoints carry more than the chapters: the moment a period ended is usually the moment something in you did.',
		hint: 'Name them however you would in conversation. The turns are the interesting part.',
		target: 400,
		tense: 'past'
	},
	{
		id: 'interests',
		facet: 'WHAT',
		prompt: 'Every hobby, obsession and interest you have ever had — and when.',
		purpose: 'What you reach for, and what you keep. The pattern of what you abandon is as telling as what stuck.',
		why: 'Appetite is steadier than opinion. What you reach for again and again describes your temperament more accurately than any self-assessment, because a list is hard to flatter yourself with. The abandoned ones matter as much as the ones that stuck — how you leave things is itself a pattern, and it shows up in your work and your friendships too.',
		hint: 'Go long and go trivial. The abandoned ones count.',
		target: 300,
		tense: 'past'
	},
	{
		id: 'people',
		facet: 'WHO',
		prompt: 'In each era, who were the most significant people?',
		purpose: 'The relational spine. These become the people your box knows by name, because you named them — never because it guessed.',
		why: 'The box will see thousands of names and know nothing about which mattered. Frequency is not significance: most people message a landlord more often than a brother. This is the only door significance can come through, and it has to be you who opens it — a machine ranking your loved ones by message volume would be both wrong and insulting.',
		hint: 'Who they were to you, and what they were like.',
		target: 400,
		tense: 'past'
	},
	{
		id: 'loss',
		facet: 'WHO',
		prompt: 'Who, or what, have you lost?',
		purpose: 'Grief shapes more than almost anything and no amount of data will ever reveal it. Written here so the box understands, and so it knows what to step around.',
		why: 'Grief is invisible to data. Someone who died leaves a thinning trace and then none, which is precisely the shape a record reads as “stopped mattering”. No amount of observation will ever recover it, and almost nothing shapes a person more. Written here, it becomes something the box understands — and something it knows to step around rather than walk into.',
		hint: 'As much or as little as you want. This one has no length worth aiming for.',
		target: null,
		tense: 'past'
	},
	{
		id: 'telos',
		facet: 'WHY',
		prompt: 'What worldview or faith do you hold, reject, or find yourself still working out?',
		purpose: 'The why underneath the rest. Not to be argued with or optimized — to be understood, so nothing it says cuts across the grain of what you believe.',
		why: 'An assistant told nothing about what you believe falls back on a bland average: agreeable, agnostic, faintly therapeutic. That voice grates on nearly everyone eventually, from every direction. This is not here to be argued with, optimized, or gently corrected. It is here so that what the box says stops cutting across the grain of what you actually hold.',
		hint: 'Including “I genuinely do not know” — that is an answer, and a useful one.',
		target: 300,
		tense: 'present'
	},
	{
		id: 'pride',
		facet: 'HOW',
		prompt: 'What are you proud of in yourself — in how you go about things?',
		purpose: 'Virtues, asked in the only way anyone can answer. “List your virtues” gets nothing; this gets the truth.',
		why: 'Nobody can answer “what are your virtues” — it comes out as false modesty or a résumé. Everyone can say what they are quietly proud of, and what surfaces is usually a HOW rather than a WHAT: how you treat people, how you handle being wrong, what you do when it stops being fun. The gap between what you are proud of and what you get praised for is where a lot of a person lives.',
		hint: 'How you do things, not only what you have done.',
		target: 300,
		tense: 'present'
	},
	{
		id: 'now',
		facet: 'NOW',
		prompt: 'What are you working on at the moment? Five or ten things.',
		purpose: 'What is live. The box will infer plenty about your days, but not which of them you actually care about.',
		why: 'The box will work out plenty about your days — where you went, who you saw, what you opened. None of it says which of it you CHOSE. A week of activity looks identical whether you are building something or avoiding something, and this is the only way to tell the two apart.',
		hint: 'Work, body, house, people, projects, the thing you keep meaning to start.',
		target: 200,
		tense: 'present'
	},
	{
		id: 'vices',
		facet: 'VICES',
		prompt: 'What do you keep starting and not finishing? And which pull is strongest for you — money, power, pleasure, or fame?',
		purpose: 'The pattern you are up against. Written by you it is self-knowledge; guessed by a machine it would be an accusation — which is why it is asked rather than derived.',
		why: 'The thing you are up against, in your own hand. Written by you it is self-knowledge; derived by a machine it would be an accusation, which is exactly why it is asked and never inferred. The four pulls are Arthur Brooks’: money, power, pleasure, fame. They work because they are a menu rather than a blank page, and most people know their answer in about a second — which is usually a sign that it is the true one.',
		hint: 'Most people know their answer immediately.',
		target: 300,
		tense: 'present'
	},
	{
		id: 'ambitions',
		facet: 'FUTURE',
		prompt: 'A month from now, three years, ten years — what do you want out of life?',
		purpose: 'Direction. This is the part that lets the box be useful about decisions rather than only accurate about the past.',
		why: 'Without direction, an assistant can only ever be accurate about your past. Three horizons because they do different jobs: ten years is identity, three years is strategy, one month is the only one that changes what next week looks like. Most people can answer the ten and stall on the one, and noticing that about yourself is worth as much as the answer.',
		hint: 'The one-month horizon is the one that changes what next week looks like.',
		target: 400,
		tense: 'future'
	},
	{
		id: 'novelty',
		facet: 'DELTA',
		prompt: 'What makes you different from everyone else you have met?',
		purpose: 'An AI assumes you are the average person until told otherwise. This question is that gap — which makes it the most valuable one here.',
		why: 'A language model\'s default assumption about you is the average human. Everything it has not been told, it fills in from the population mean. So this question asks, quite literally, for the difference between you and that prior — which makes it the highest-information thing you can write here. It is also the one people skip because it feels like bragging. It is not bragging; it is calibration.',
		hint: 'The things people find surprising, or that you have stopped mentioning because nobody relates.',
		target: 300,
		tense: 'present'
	},
	{
		id: 'standing_orders',
		facet: 'RULES',
		prompt: 'Of everything you have written — what should it never bring up?',
		purpose: 'Some things matter enormously and should still never be raised unprompted. These become rules, not suggestions.',
		why: 'Everything above is for understanding. This is for enforcement. Some things matter enormously and must still never be raised unprompted — a person you lost, an addiction in recovery, a marriage that ended. Prose cannot guarantee that: a model reading a paragraph might honour it nine times and miss the tenth, and the tenth is the one that would matter. What you write here becomes a rule instead.',
		hint: 'Plainly: “never suggest bars”, “do not mention my father unless I do”.',
		target: null,
		tense: 'standing'
	},
];

/** Words, counted the way a person would count them. */
export function wordCount(s: string): number {
	const t = s.trim();
	return t ? t.split(/\s+/).length : 0;
}
