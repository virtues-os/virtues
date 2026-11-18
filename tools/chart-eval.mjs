#!/usr/bin/env node
// A fixed set of chart questions, asked of a running core, each in a new chat.
// For each answer it reports whether the model loaded the charts skill, how
// many show calls it made and how many were refused, whether the first call
// drew, and whether the drawn chart serves the purpose the question expects.
// This is the check agents/plan/chart-purpose-plan.md, "How to tell it
// worked", asks for: a drawn chart in one call, or two at most after a refusal.
//
// The questions read pages, location visits and calendar events, so run it on
// a server whose record has those. It writes nothing but the chats it starts.
//
//   node tools/chart-eval.mjs                      # http://localhost:8000
//   node tools/chart-eval.mjs --base http://box:8000 --only 3,7

const args = process.argv.slice(2);
const flag = (name) => {
	const i = args.indexOf(name);
	return i >= 0 ? args[i + 1] : undefined;
};
const BASE = flag('--base') ?? 'http://localhost:8000';
const ONLY = flag('--only')?.split(',').map(Number);

// Each question, the purposes a drawn chart may serve, and whether the
// question asks for a figure outright. A question that does not ask may be
// answered in words; one that asks and gets no figure is a miss.
const QUESTIONS = [
	{ q: 'How has the number of pages I make each week changed over time?', purposes: ['change_over_time'], asks: false },
	{ q: 'Is my weekly page count normal for me lately? Show it against my usual range', purposes: ['usual_range'], asks: true },
	{ q: 'When in the day do I usually make pages, weekdays against weekends?', purposes: ['daily_rhythm', 'relationship'], asks: false },
	{ q: 'Which days of the week do I make the most pages on? Draw it', purposes: ['compare_groups', 'relationship'], asks: true },
	{ q: "What's coming up on my calendar in the next two weeks? Show it", purposes: ['events'], asks: true },
	{ q: 'Where did my time go in October? Show it on a map', purposes: ['place'], asks: true },
	{ q: 'Show where I was on my most recent day of location visits, in order', purposes: ['events', 'place'], asks: true },
	{ q: 'Draw my days in October by the place I was at, one row per day', purposes: ['daily_rhythm'], asks: true },
	{ q: 'Did I make more pages a week after October 19 than before? Show the change', purposes: ['before_and_after'], asks: true },
	{ q: 'On average, how many pages a day did I make in November? Put the figure in your sentence with the days behind it', purposes: ['in_sentence'], asks: true },
	{ q: 'How is this month going against last month for pages made, as a running total?', purposes: ['change_over_time'], asks: true },
	{ q: 'Do I make more pages on days I visit more places? Plot one against the other', purposes: ['relationship'], asks: true },
	{ q: 'Draw the route I took on my most recent day of location visits', purposes: ['place'], asks: true },
	{ q: 'How do my weekday and weekend page counts compare, then against now?', purposes: ['compare_groups', 'change_over_time'], asks: false },
	{ q: 'Show me my calendar events and location visits over time, one lane each', purposes: ['events'], asks: true },
];

const id = (prefix) => `${prefix}_eval_${Math.random().toString(16).slice(2, 14)}`;

async function ask(question) {
	const chatId = id('chat');
	const res = await fetch(`${BASE}/api/chat`, {
		method: 'POST',
		headers: { 'content-type': 'application/json' },
		body: JSON.stringify({
			chatId,
			messageId: id('msg'),
			messages: [{ id: id('msg'), role: 'user', parts: [{ type: 'text', text: question }] }],
		}),
	});
	if (!res.ok) throw new Error(`POST /api/chat: ${res.status} ${await res.text()}`);
	// The reply streams; reading it to the end waits for the turn to finish.
	const reader = res.body.getReader();
	while (!(await reader.read()).done);
	const chat = await (await fetch(`${BASE}/api/chats/${chatId}`)).json();
	return { chatId, messages: chat.messages ?? [] };
}

function score(item, messages) {
	const parts = messages.filter((m) => m.role === 'assistant').flatMap((m) => m.parts ?? []);
	const tools = parts.filter((p) => typeof p.type === 'string' && p.type.startsWith('tool-'));
	const loaded = tools.some((p) => p.type === 'tool-use_applet' && p.input?.name === 'charts');
	const shows = tools.filter((p) => p.type === 'tool-show' && p.input?.chart !== 'choices');
	const drawn = shows.filter((p) => p.state === 'output-available' && !p.errorText);
	const refused = shows.length - drawn.length;
	const firstDrew = shows.length > 0 && !shows[0].errorText && shows[0].state === 'output-available';
	const marks = drawn.map((p) => p.output?.mark ?? p.output?.kind);
	const purposes = drawn.map((p) => p.output?.purpose).filter(Boolean);
	const onPurpose = drawn.length === 0 || purposes.length === 0 || purposes.some((p) => item.purposes.includes(p));
	let verdict;
	if (drawn.length === 0) verdict = item.asks ? 'MISS: no figure' : 'words';
	else if (drawn.length > 1) verdict = 'drew twice';
	else if (refused > 1) verdict = 'drew after 2+ refusals';
	else if (!onPurpose) verdict = 'off purpose';
	else verdict = firstDrew ? 'one call' : 'after one refusal';
	return { loaded, shows: shows.length, refused, marks, purposes, verdict, refusals: shows.filter((p) => p.errorText).map((p) => p.errorText.slice(0, 120)) };
}

const rows = [];
for (const [i, item] of QUESTIONS.entries()) {
	const n = i + 1;
	if (ONLY && !ONLY.includes(n)) continue;
	try {
		const { chatId, messages } = await ask(item.q);
		const s = score(item, messages);
		rows.push({ n, ...s });
		console.log(`${String(n).padStart(2)}. ${s.verdict.padEnd(22)} skill=${s.loaded ? 'yes' : 'no '} shows=${s.shows} refused=${s.refused} drew=${s.marks.join(',') || '-'}  ${chatId}`);
		for (const r of s.refusals) console.log(`      refused: ${r}`);
	} catch (e) {
		rows.push({ n, verdict: 'error' });
		console.log(`${String(n).padStart(2)}. error: ${e.message}`);
	}
}

const count = (v) => rows.filter((r) => r.verdict === v).length;
console.log(
	`\n${rows.length} questions: ${count('one call')} drew in one call, ${count('after one refusal')} after one refusal, ` +
		`${count('words')} answered in words (not asked for a figure), ${rows.filter((r) => r.verdict?.startsWith('MISS')).length} missed, ` +
		`${count('drew twice')} drew twice, ${count('off purpose')} off purpose. Skill loaded in ${rows.filter((r) => r.loaded).length}.`,
);
