// The lab box's data: invented, and the same on every run.
//
// Every word here is made up for this file (.claude/rules/fixtures.md). The
// shape is what matters: enough chats to fill the list, and one long chat whose
// replies carry what makes a transcript expensive to render — headings, lists,
// code blocks, tables, reasoning, and tool results with many rows. A seeded
// generator keeps the bytes identical from run to run, so a change in the
// numbers is a change in the app, not in the data.

/** mulberry32: a tiny seeded PRNG, so the "random" choices never vary. */
function rng(seed) {
	let a = seed >>> 0;
	return () => {
		a = (a + 0x6d2b79f5) >>> 0;
		let t = a;
		t = Math.imul(t ^ (t >>> 15), t | 1);
		t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
		return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
	};
}

const TOPICS = [
	'the garden beds', 'the bicycle chain', 'the book club list', 'a weekend in the hills',
	'the kitchen shelves', 'learning the cello', 'the reading nook', 'the bread starter',
	'the tide pool trip', 'the attic boxes', 'the chess openings', 'the tomato seedlings'
];
const SENTENCES = [
	'Nick moved the trellis to the sunny side of the fence.',
	'The second coat of paint dried faster than the first.',
	'A short walk after lunch made the afternoon easier.',
	'The library had two copies, so David Okafor borrowed one.',
	'Most of the seedlings came up within a week.',
	'The plan works better when the hard part comes first.',
	'Rain moved the picnic indoors, which nobody minded.',
	'The old shelf needed four new brackets and a level.',
	'Practice went well once the metronome slowed down.',
	'The list is shorter now, and the rest can wait.'
];

function para(r, n) {
	let s = '';
	for (let i = 0; i < n; i++) s += SENTENCES[Math.floor(r() * SENTENCES.length)] + ' ';
	return s.trim();
}

function assistantMarkdown(r, i) {
	const topic = TOPICS[i % TOPICS.length];
	let md = `## About ${topic}\n\n${para(r, 4)}\n\n`;
	md += `- ${para(r, 1)}\n- ${para(r, 1)}\n- ${para(r, 1)}\n\n`;
	if (i % 10 === 3) {
		md += '```ts\n' + `export function plan${i}(items: string[]): string[] {\n\treturn items.filter((x) => x.length > 3).sort();\n}\n` + '```\n\n';
	}
	if (i % 15 === 7) {
		md += '| Step | Who | Done |\n|---|---|---|\n';
		for (let k = 1; k <= 6; k++) md += `| Step ${k} | ${k % 2 ? 'Nick' : 'David Okafor'} | ${k % 3 ? 'yes' : 'no'} |\n`;
		md += '\n';
	}
	md += para(r, 3);
	return md;
}

function toolRows(r, n) {
	const rows = [];
	for (let k = 0; k < n; k++) {
		rows.push({ id: `row_${k}`, label: TOPICS[Math.floor(r() * TOPICS.length)], note: para(r, 1), count: Math.floor(r() * 20) });
	}
	return rows;
}

const q = (s) => `'${String(s).replace(/'/g, "''")}'`;
const BASE = Date.UTC(2026, 0, 5, 9, 0, 0); // an invented morning
const at = (minutes) => q(new Date(BASE + minutes * 60_000).toISOString());

export const LONG_CHAT_ID = 'chat_perf_long';
export const LONG_CHAT_MESSAGES = 300;
export const CHAT_COUNT = 40;

/** The whole seed as one SQL script. */
export function seedSql() {
	const r = rng(42);
	const out = ['BEGIN;'];

	// An owner who finished Setup. A fresh box opens Setup instead of the app,
	// and the lab measures the app. The box seeds this one profile row itself.
	out.push(`UPDATE app_user_profile SET onboarding_status = 'active';`);

	// The list: short chats, two messages each.
	for (let c = 0; c < CHAT_COUNT; c++) {
		const id = `chat_perf_${String(c).padStart(3, '0')}`;
		const t = c * 90;
		out.push(
			`INSERT INTO app_chats (id, title, message_count, created_at, updated_at) VALUES (${q(id)}, ${q('Planning ' + TOPICS[c % TOPICS.length])}, 2, ${at(t)}, ${at(t + 1)});`
		);
		const user = para(r, 1);
		const reply = assistantMarkdown(r, c);
		out.push(
			`INSERT INTO app_chat_messages (id, chat_id, role, content, parts, sequence_num, created_at) VALUES ` +
				`(${q(id + '_u')}, ${q(id)}, 'user', ${q(user)}, ${q(JSON.stringify([{ type: 'text', text: user }]))}::jsonb, 0, ${at(t)}), ` +
				`(${q(id + '_a')}, ${q(id)}, 'assistant', ${q(reply)}, ${q(JSON.stringify([{ type: 'text', text: reply }]))}::jsonb, 1, ${at(t + 1)});`
		);
	}

	// The long chat, newest of all so it leads the list.
	const lt = CHAT_COUNT * 90 + 10;
	out.push(
		`INSERT INTO app_chats (id, title, message_count, created_at, updated_at) VALUES (${q(LONG_CHAT_ID)}, 'A long planning thread', ${LONG_CHAT_MESSAGES}, ${at(lt)}, ${at(lt + LONG_CHAT_MESSAGES)});`
	);
	for (let i = 0; i < LONG_CHAT_MESSAGES; i++) {
		const role = i % 2 === 0 ? 'user' : 'assistant';
		const id = `${LONG_CHAT_ID}_${String(i).padStart(4, '0')}`;
		let content;
		let parts;
		if (role === 'user') {
			content = `What about ${TOPICS[i % TOPICS.length]}? ${para(r, 1)}`;
			parts = [{ type: 'text', text: content }];
		} else {
			content = assistantMarkdown(r, i);
			parts = [{ type: 'reasoning', text: para(r, 2) }];
			if (i % 5 === 1) {
				parts.push({
					type: 'tool-sql_query',
					toolCallId: `call_${i}`,
					input: { sql: 'SELECT id, label, note, count FROM notes LIMIT 50' },
					state: 'output-available',
					output: { rows: toolRows(r, 50) }
				});
			}
			parts.push({ type: 'text', text: content });
		}
		out.push(
			`INSERT INTO app_chat_messages (id, chat_id, role, content, parts, sequence_num, created_at) VALUES ` +
				`(${q(id)}, ${q(LONG_CHAT_ID)}, ${q(role)}, ${q(content)}, ${q(JSON.stringify(parts))}::jsonb, ${i}, ${at(lt + i)});`
		);
	}
	out.push('COMMIT;');
	return out.join('\n') + '\n';
}
