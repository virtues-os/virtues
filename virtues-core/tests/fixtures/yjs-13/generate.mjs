// Page states as the browser writes them, for `server/yjs.rs`'s tests.
//
//   node virtues-core/tests/fixtures/yjs-13/generate.mjs
//
// Uses apps/web's own `yjs` (run `pnpm install` there first). The editor binds
// a Y.Text named "content", counts positions in UTF-16 code units, and saves a
// version as `Y.encodeStateAsUpdate(ydoc)` (`apps/web/src/lib/yjs/versions.ts`),
// which the server stores as given in `app_page_versions.yjs_snapshot`. Its
// edits reach `app_pages.yjs_state` through the server's doc. Each fixture is
// `<name>.bin` (the bytes) and `<name>.txt` (the text Yjs reads from them).
// Every input is fixed, so a run writes the same files.

import { writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';

const require = createRequire(new URL('../../../../apps/web/package.json', import.meta.url));
const Y = require('yjs');
const out = (name) => new URL(`./${name}`, import.meta.url);

function save(name, bytes, text) {
	writeFileSync(out(`${name}.bin`), bytes);
	writeFileSync(out(`${name}.txt`), text);
}

function docAs(clientID) {
	const doc = new Y.Doc();
	doc.clientID = clientID;
	return doc;
}

/** Type `text` at `at` one character at a time, as keystrokes arrive. */
function type(ytext, at, text) {
	let i = at;
	for (const ch of text) {
		ytext.insert(i, ch);
		i += ch.length;
	}
}

const WAVE = '\u{1F44B}';
const MEDIUM = '\u{1F3FD}';
const FAMILY = '\u{1F468}‍\u{1F469}‍\u{1F467}‍\u{1F466}';
const RAINBOW = '\u{1F3F3}️‍\u{1F308}';

// typed: a version snapshot of a page typed key by key, around and inside
// two-, three- and four-byte characters, then edited at UTF-16 positions: the
// skin tone deleted off a wave, a word inserted inside a CJK run.
{
	const doc = docAs(101);
	const t = doc.getText('content');
	type(t, 0, `Café crème at São Paulo.\n東京の朝は静かだ。\nWave ${WAVE}${MEDIUM} then ${FAMILY} and ${RAINBOW} end.\n`);
	const s = t.toString();
	t.delete(s.indexOf(MEDIUM), MEDIUM.length);
	type(t, t.toString().indexOf('の朝'), '駅');
	type(t, t.length, `Fin ${WAVE}.\n`);
	save('typed', Y.encodeStateAsUpdate(doc), t.toString());
}

// split-surrogate: a deletion of one UTF-16 unit, half of an emoji's
// surrogate pair. Yjs puts U+FFFD where the broken half was.
{
	const doc = docAs(102);
	const t = doc.getText('content');
	type(t, 0, `Before ${WAVE} after.\n`);
	t.delete('Before '.length, 1);
	type(t, 0, '> ');
	save('split-surrogate', Y.encodeStateAsUpdate(doc), t.toString());
}

// concurrent: two editors making 200 edits each around emoji and CJK, synced
// every few steps, from a fixed seed.
{
	let seed = 7;
	const random = () => {
		seed = (seed * 1103515245 + 12345) % 2147483648;
		return seed / 2147483648;
	};
	const pieces = ['a', ' ', WAVE, MEDIUM, '東', '京', FAMILY, 'é', '\n', RAINBOW];
	const a = docAs(201);
	const b = docAs(202);
	const sync = () => {
		Y.applyUpdate(b, Y.encodeStateAsUpdate(a, Y.encodeStateVector(b)));
		Y.applyUpdate(a, Y.encodeStateAsUpdate(b, Y.encodeStateVector(a)));
	};
	type(a.getText('content'), 0, `Start ${WAVE} 東京.\n`);
	sync();
	for (let step = 0; step < 400; step++) {
		const t = (step % 2 ? a : b).getText('content');
		// Code points, so an edit never lands inside a pair.
		const points = [...t.toString()];
		const at = Math.floor(random() * (points.length + 1));
		const offset = points.slice(0, at).join('').length;
		if (random() < 0.3 && at < points.length) {
			t.delete(offset, points[at].length);
		} else {
			t.insert(offset, pieces[Math.floor(random() * pieces.length)]);
		}
		if (step % 7 === 0) sync();
	}
	sync();
	if (a.getText('content').toString() !== b.getText('content').toString()) {
		throw new Error('the replicas did not converge');
	}
	save('concurrent', Y.encodeStateAsUpdate(a), a.getText('content').toString());
}

// update: what an open editor sends the server after its owner types, on a
// page whose state is `typed`: an insert in the middle, a deletion, an
// append. The text is what the page reads after it.
{
	const doc = docAs(103);
	Y.applyUpdate(doc, Y.encodeStateAsUpdate((() => {
		const base = docAs(101);
		const t = base.getText('content');
		type(t, 0, `Café crème at São Paulo.\n東京の朝は静かだ。\nWave ${WAVE}${MEDIUM} then ${FAMILY} and ${RAINBOW} end.\n`);
		const s = t.toString();
		t.delete(s.indexOf(MEDIUM), MEDIUM.length);
		type(t, t.toString().indexOf('の朝'), '駅');
		type(t, t.length, `Fin ${WAVE}.\n`);
		return base;
	})()));
	const before = Y.encodeStateVector(doc);
	const t = doc.getText('content');
	type(t, t.toString().indexOf('São'), 'lovely ');
	t.delete(t.toString().indexOf('静か'), '静か'.length);
	type(t, t.length, `한국어 ${FAMILY}\n`);
	save('update', Y.encodeStateAsUpdate(doc, before), t.toString());
}
