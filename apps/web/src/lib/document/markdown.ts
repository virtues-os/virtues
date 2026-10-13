/**
 * A block page's markdown export, written in the browser: the crate's
 * `render::markdown` (crates/virtues-document/src/render.rs), rule for rule,
 * so text copied out of a block page pastes into the chat composer or a
 * markdown page as the markdown the page's `content` column holds. The
 * conformance corpus holds the two to the same bytes for every case
 * (`conformance.test.ts`).
 *
 * It writes markdown only: reading markdown stays the server's one
 * converter's (`paste.ts`).
 */

import type { Fragment, Mark, Node as PmNode, Slice } from '@tiptap/pm/model';
import { DOMSerializer } from '@tiptap/pm/model';
import { contract, isLabel, ITEM_NUMBER, normalizeUrl, typed } from './schema';

/** Where inline content is written: how a break and a pipe are written so the text reads back. */
type Lines = 'many' | 'one' | 'cell';

interface Prefix {
	first: string;
	rest: string;
	/** The container has had a line, so its next one takes `rest`. */
	used: boolean;
	/** Anything was written inside it, a line break included. */
	wrote: boolean;
}

/** Where the writing stands at the end of what was written (render.rs `Piece`). */
type Piece = 'fresh' | 'partial' | 'done' | 'blank';

interface Held {
	line: string;
	/** The outermost container (its index in `open`) whose markdown holds this line as an empty one. */
	blankFrom: number;
}

function nextPrefix(p: Prefix, empty: boolean): string {
	const s = p.used ? p.rest : p.first;
	if (empty) {
		const trimmed = s.trimEnd();
		if (trimmed.length < s.length) return trimmed;
	}
	return s;
}

/** Markdown written line by line, each line taking the prefixes of the containers around it (render.rs `Md`). */
class Md {
	out = '';
	open: Prefix[] = [];
	piece: Piece = 'fresh';
	ended: string[] = [];
	line = '';
	holds = false;
	held: Held[] = [];
	writes = 0;
	/** The next list written takes its kind's other marker (`continuesList`); read and cleared as it opens. */
	altList = false;

	private wrote(): void {
		this.writes++;
		const p = this.open.at(-1);
		if (p) p.wrote = true;
	}

	private release(): void {
		for (const h of this.held) this.out += `${h.line}\n`;
		this.held = [];
	}

	text(s: string): void {
		s.split('\n').forEach((piece, i) => {
			if (i > 0) this.newline();
			if (!piece) return;
			if (this.piece !== 'partial') {
				this.release();
				this.piece = 'partial';
			}
			this.line += piece;
			this.holds = true;
			this.wrote();
		});
	}

	private newline(): void {
		this.wrote();
		if (this.piece === 'done' || this.piece === 'blank') {
			this.piece = 'fresh';
			return;
		}
		let inner = this.ended.some((p) => p !== '');
		let blankFrom = this.open.length;
		const prefixes: string[] = [];
		for (let j = this.open.length - 1; j >= 0; j--) {
			const p = this.open[j];
			if (this.line.endsWith('\r')) this.line = this.line.slice(0, -1);
			const next = nextPrefix(p, !inner && this.line === '');
			inner ||= next !== '';
			if (!inner && this.line === '') blankFrom = j;
			prefixes.push(next);
			p.used = true;
		}
		const line = [...prefixes].reverse().join('') + [...this.ended].reverse().join('') + this.line;
		if (blankFrom < this.open.length) {
			this.held.push({ line, blankFrom });
		} else {
			this.release();
			this.out += `${line}\n`;
		}
		this.line = '';
		this.ended = [];
		this.holds = false;
		this.piece = 'fresh';
	}

	openContainer(first: string, rest: string): void {
		this.open.push({ first, rest, used: false, wrote: false });
	}

	close(): void {
		const p = this.open.pop();
		if (!p) return;
		const at = this.open.length;
		if (this.piece === 'done') {
			// Written out already.
		} else if (this.piece === 'partial') {
			const next = nextPrefix(p, !this.holds);
			if (next !== '') {
				this.holds = true;
				this.release();
			}
			this.ended.push(next);
		} else if (this.piece === 'fresh' && !p.wrote) {
			// It wrote nothing: it is its first prefix alone.
			const next = nextPrefix(p, true);
			this.piece = 'partial';
			if (next !== '') {
				this.holds = true;
				this.release();
				this.ended.push(next);
				this.wrote();
			}
		} else {
			if (this.piece === 'blank') this.held.pop();
			const h = this.held.at(-1);
			this.piece = h && at >= h.blankFrom ? 'blank' : 'done';
		}
		if (p.wrote) {
			const parent = this.open.at(-1);
			if (parent) parent.wrote = true;
		}
	}

	toString(): string {
		while (this.open.length) this.close();
		this.release();
		if (this.piece === 'partial') {
			this.out += [...this.ended].reverse().join('') + this.line;
		} else if (this.piece === 'done' || this.piece === 'blank') {
			this.out = this.out.slice(0, -1);
		}
		return this.out;
	}
}

const children = (n: PmNode): PmNode[] => {
	const out: PmNode[] = [];
	n.forEach((c) => out.push(c));
	return out;
};

const str = (v: unknown): string => (typeof v === 'string' ? v : '');

function blockString(n: PmNode, inTable: boolean): string {
	const w = new Md();
	blockMd(n, inTable, w);
	return w.toString();
}

/**
 * Whether `nodes[i]` is a list written straight after a list of its own
 * kind, which markdown would read as the same list, and so takes the other
 * marker (`*` for `-`, `)` for `.`) unless the list before it took that one
 * (render.rs `continues_list`). `before`: whether the block before it did.
 */
function continuesList(nodes: PmNode[], i: number, before: boolean): boolean {
	const name = nodes[i].type.name;
	const list = name === 'bulletList' || name === 'orderedList' || name === 'taskList';
	return i > 0 && list && nodes[i - 1].type.name === name && !before;
}

function childrenMd(n: PmNode, sep: string, w: Md): void {
	const kids = children(n);
	let alt = false;
	kids.forEach((c, i) => {
		if (i > 0) w.text(sep);
		alt = continuesList(kids, i, alt);
		w.altList = alt;
		blockMd(c, false, w);
	});
}

function blockMd(n: PmNode, inTable: boolean, w: Md): void {
	const name = n.type.name;
	if (name === 'blockquote') {
		w.openContainer('> ', '> ');
		childrenMd(n, '\n\n', w);
		w.close();
	} else if (name === 'callout') {
		w.text(`> [!${(str(n.attrs.tone) || 'note').toUpperCase()}]\n`);
		w.openContainer('> ', '> ');
		childrenMd(n, '\n\n', w);
		w.close();
	} else if (name === 'bulletList' || name === 'orderedList' || name === 'taskList') {
		const alt = w.altList;
		w.altList = false;
		const [bullet, stop] = alt ? ['*', ')'] : ['-', '.'];
		const items = children(n);
		const loose = items.some((item) => item.childCount > 1);
		// Each item's number, its own where it has one: markdown writes every
		// item's, and reads each back as written. A number below zero, which
		// no marker writes, gives way to the count.
		let next = typeof n.attrs.start === 'number' && n.attrs.start >= 0 ? n.attrs.start : 1;
		items.forEach((item, i) => {
			if (i > 0) w.text(loose ? '\n\n' : '\n');
			const own = item.attrs[ITEM_NUMBER];
			const number = typeof own === 'number' && own >= 0 ? own : next;
			next = number + 1;
			const marker =
				name === 'orderedList'
					? `${number}${stop} `
					: name === 'taskList'
						? `${bullet} [${item.attrs.checked === true ? 'x' : ' '}] `
						: `${bullet} `;
			w.openContainer(marker, ' '.repeat(name === 'taskList' ? 2 : marker.length));
			let firstEmpty = false;
			let altInside = false;
			const blocks = children(item);
			blocks.forEach((b, k) => {
				if (k > 0) {
					// After an empty first block, a blank line would end the item.
					const afterEmpty = k === 1 && firstEmpty;
					w.text(loose && !afterEmpty ? '\n\n' : '\n');
				}
				const before = w.writes;
				altInside = continuesList(blocks, k, altInside);
				w.altList = altInside;
				blockMd(b, false, w);
				if (k === 0) firstEmpty = w.writes === before;
			});
			w.close();
		});
	} else {
		w.text(leafMd(n, inTable));
	}
}

function lastSegment(url: string): string {
	const path = url.split(/[?#]/)[0];
	return [...path.split('/')].reverse().find((s) => s !== '') ?? '';
}

/** The longest run of `ch` in `text`. */
function longestRun(text: string, ch: string): number {
	let longest = 0;
	let run = 0;
	for (const c of text) {
		run = c === ch ? run + 1 : 0;
		longest = Math.max(longest, run);
	}
	return longest;
}

function leafMd(n: PmNode, inTable: boolean): string {
	const lines = (own: Lines): Lines => (inTable ? 'cell' : own);
	switch (n.type.name) {
		case 'paragraph':
			return inlineMd(children(n), lines('many'));
		case 'heading': {
			const level = Math.min(6, Math.max(1, typeof n.attrs.level === 'number' ? n.attrs.level : 1));
			let text = inlineMd(children(n), lines('one'));
			// A heading's closing `#`s are dropped on the way in.
			if (text.endsWith('#')) text = `${text.slice(0, -1)}\\#`;
			return `${'#'.repeat(level)} ${text}`;
		}
		case 'codeBlock': {
			const text = n.textContent;
			const lang = infoString(str(n.attrs.language));
			const mark = lang.includes('`') ? '~' : '`';
			// An article reads a fence of exactly three backticks in the
			// language `figure` as a figure: code that would read as one takes
			// a fence of four, and stays code.
			const least = lang === 'figure' && readsAsFigure(text) ? 4 : 3;
			const fence = mark.repeat(Math.max(longestRun(text, mark) + 1, least));
			return `${fence}${lang}\n${text}\n${fence}`;
		}
		case 'figure':
			return figureMd(n);
		case 'horizontalRule':
			return '---';
		case 'image': {
			let alt = escapeMd(str(n.attrs.alt).replace(/[\r\n]/g, ' '), 'cell', '', false);
			if (typeof n.attrs.width === 'number') alt += `|${n.attrs.width}`;
			else alt = alt.replaceAll('|', '&#124;');
			return `![${alt}](${destination(str(n.attrs.src))})`;
		}
		case 'audio':
		case 'video':
		case 'file': {
			const src = str(n.attrs.src);
			const own = str(n.attrs.name);
			const name = escapeMd((own || lastSegment(src)).replace(/[\r\n]/g, ' '), 'cell', '', false);
			return `![${name.replaceAll('|', '&#124;')}](${destination(src)})`;
		}
		case 'table':
			return tableMd(n);
		case 'applet': {
			const el = DOMSerializer.fromSchema(n.type.schema).serializeNode(n) as HTMLElement;
			el.removeAttribute('data-id');
			return el.outerHTML;
		}
		default:
			return `<!-- ${n.type.name} -->`;
	}
}

/** The figure's attributes, in the contract's order. */
const FIGURE_ATTRS = Object.entries(contract.nodes.figure?.attrs ?? {});

/**
 * A figure as the day page's writers write it (render.rs `figure_md`): a
 * fence of three backticks in the language `figure`, a `key: value` line for
 * each value it holds, in the contract's order.
 */
function figureMd(n: PmNode): string {
	let s = '```figure\n';
	for (const [key] of FIGURE_ATTRS) {
		const value = n.attrs[key];
		if (typeof value === 'string') s += `${key}: ${value.replace(/[\r\n]/g, ' ')}\n`;
	}
	return `${s}\`\`\``;
}

/** What Rust's `char::is_whitespace` reads as white space: JavaScript's `\s` but U+FEFF, and U+0085. */
const WHITE_SPACE = /[\t\n\v\f\r \u0085\u00a0\u1680\u2000-\u200a\u2028\u2029\u202f\u205f\u3000]+/;

/**
 * Whether a figure fence's body reads as a figure (migrate.rs `figure_html`):
 * every line `key: value`, each key one of the figure's and none twice, its
 * `kind` one the contract names and its `src` one a page may hold.
 */
export function readsAsFigure(body: string): boolean {
	const seen = new Map<string, string>();
	for (const line of body.split('\n')) {
		const at = line.indexOf(':');
		if (at < 0) return false;
		// ASCII case alone, as the crate folds it.
		const key = line.slice(0, at).split(WHITE_SPACE).filter(Boolean).join(' ').replace(/[A-Z]/g, (c) => c.toLowerCase());
		if (!FIGURE_ATTRS.some(([k]) => k === key) || seen.has(key)) return false;
		seen.set(key, line.slice(at + 1).split(WHITE_SPACE).filter(Boolean).join(' '));
	}
	return FIGURE_ATTRS.every(([key, spec]) => {
		const value = seen.get(key);
		if (value === undefined) return !spec.required;
		return typed(spec, spec.type === 'url' ? normalizeUrl(value) : value) !== null;
	});
}

function tableMd(n: PmNode): string {
	const rows = children(n).map((row) =>
		children(row).map((cell) =>
			children(cell)
				.map((b) => blockString(b, true).replace(/\n/g, ' '))
				.join('<br>')
				.replaceAll('|', '\\|'),
		),
	);
	const width = Math.max(0, ...rows.map((r) => r.length));
	const align: (string | null)[] = [];
	for (let col = 0; col < width; col++) {
		let found: string | null = null;
		for (const row of children(n)) {
			const cell = row.childCount > col ? row.child(col) : null;
			if (cell && typeof cell.attrs.align === 'string') {
				found = cell.attrs.align;
				break;
			}
		}
		align.push(found);
	}
	const line = (cells: string[]) => {
		const padded = [...cells, ...Array(Math.max(0, width - cells.length)).fill('')];
		return `| ${padded.join(' | ')} |`;
	};
	const first = n.firstChild;
	const headerRow = !!first && children(first).every((c) => c.type.name === 'tableHeader');
	const [head, body] = headerRow && rows.length ? [rows[0], rows.slice(1)] : [Array(width).fill(''), rows];
	const rule = align.map((a) => (a === 'left' ? ' :--- ' : a === 'center' ? ' :---: ' : a === 'right' ? ' ---: ' : ' --- '));
	return [line(head), `|${rule.join('|')}|`, ...body.map(line)].join('\n');
}

const isAsciiAlnum = (c: string | undefined) => !!c && /^[A-Za-z0-9]$/.test(c);
const isAsciiPunct = (c: string | undefined) => !!c && /^[!-/:-@[-`{-~]$/.test(c);

/** Text as markdown that reads back as the same text (render.rs `escape_md`). */
function escapeMd(s: string, lines: Lines, before: string, body: boolean): string {
	const chars = Array.from(s, (ch) => (ch === '\n' || ch === '\r' ? ' ' : ch));
	const last = Array.from(before);
	let tail: [string | undefined, string | undefined] = [last.at(-2), last.at(-1)];
	let out = '';
	chars.forEach((ch, i) => {
		const next = (k: number) => chars[i + k];
		const opens = (c: string) => next(1) === c && next(2) === c;
		let escape = false;
		switch (ch) {
			case '\\':
			case '*':
			case '_':
			case '`':
			case '[':
			case ']':
			case '<':
			case '~':
				escape = true;
				break;
			case '|':
				escape = lines !== 'cell';
				break;
			case '=':
				escape = tail[1] === '=';
				break;
			case '&':
				escape = isAsciiAlnum(next(1)) || next(1) === '#';
				break;
			case '{':
				escape = opens('-') || opens('+') || (body && opens('>'));
				break;
			case '}':
				escape = body && tail[0] === tail[1] && (tail[1] === '-' || tail[1] === '+' || tail[1] === '<');
				break;
		}
		if (escape) out += '\\';
		out += ch;
		tail = [tail[1], ch];
	});
	return out;
}

/** What would make the start of a line block syntax, escaped (render.rs `escape_line_start`). */
function escapeLineStart(text: string, blockStart: boolean): string {
	const spaces = text.length - text.replace(/^ +/, '').length;
	if (spaces >= 4) return blockStart ? `&#32;${text.slice(1)}` : text;
	const rest = text.slice(spaces);
	const lead = text.slice(0, spaces);
	const first = rest[0];
	if (first === '#' || first === '>' || first === '-' || first === '+' || first === '=') return `${lead}\\${rest}`;
	if (first && /[0-9]/.test(first)) {
		const digits = rest.length - rest.replace(/^[0-9]+/, '').length;
		const after = rest[digits];
		if ((after === '.' || after === ')') && digits <= 9) return `${lead}${rest.slice(0, digits)}\\${rest.slice(digits)}`;
	}
	return lead + rest;
}

/** A code block's language as a fence's info string (render.rs `info_string`). */
function infoString(lang: string): string {
	const chars = Array.from(lang);
	let out = '';
	chars.forEach((ch, i) => {
		const next = chars[i + 1];
		const escape = (ch === '\\' && isAsciiPunct(next)) || (ch === '&' && (isAsciiAlnum(next) || next === '#'));
		if (escape) out += '\\';
		out += ch === '\n' || ch === '\r' ? ' ' : ch;
	});
	return out;
}

// eslint-disable-next-line no-control-regex
const CONTROL = /[\u0000-\u001f\u007f-\u009f]/;

/** A link destination that reads back as `url` (render.rs `destination`). */
function destination(raw: string): string {
	const chars = Array.from(raw.replace(/[\t\n\r]/g, ''));
	const pointy = chars.some((ch) => ch === ' ' || ch === '(' || ch === ')' || ch === '<' || ch === '>' || CONTROL.test(ch));
	let out = pointy ? '<' : '';
	chars.forEach((ch, i) => {
		const next = chars[i + 1];
		const escape =
			(ch === '\\' && (isAsciiPunct(next) || pointy))
			|| ((ch === '<' || ch === '>') && pointy)
			|| (ch === '&' && (isAsciiAlnum(next) || next === '#'));
		if (escape) out += '\\';
		out += ch;
	});
	return pointy ? `${out}>` : out;
}

/** A bare `!` before a link would read as an image's. */
function escapeBang(out: string): string {
	return out.endsWith('!') ? `${out.slice(0, -1)}\\!` : out;
}

function delim(kind: string): [string, string] {
	switch (kind) {
		case 'bold':
			return ['**', '**'];
		case 'italic':
			return ['*', '*'];
		case 'strike':
			return ['~~', '~~'];
		case 'highlight':
			return ['==', '=='];
		case 'underline':
			return ['<u>', '</u>'];
		case 'veil':
			return VEIL;
		default:
			return ['', ''];
	}
}

/**
 * The veil's delimiters (render.rs `VEIL`). Innermost of all, so a veil
 * inside a link or around bold reads back as the same marks; written around
 * each run, with no whitespace moved outside them.
 */
const VEIL: [string, string] = ['⟦', '⟧'];

/** A veil's own text with its brackets written as character references. */
function veiled(text: string): string {
	return text.replaceAll(VEIL[0], '&#10214;').replaceAll(VEIL[1], '&#10215;');
}

const PROPOSALS: Record<string, [string, string]> = {
	proposedDeletion: ['{--', '--}'],
	proposedInsertion: ['{++', '++}'],
};

const linkOf = (n: PmNode): Mark | null => n.marks.find((m) => m.type.name === 'link') ?? null;
const sameMark = (a: Mark | null, b: Mark | null) => (a === null ? b === null : b !== null && a.eq(b));

function proposalOf(n: PmNode): Mark | null {
	if (!n.isText) return null;
	return n.marks.find((m) => m.type.name in PROPOSALS) ?? null;
}

/** Where the proposal that `nodes[i]` starts ends; an atom between two runs of it is inside it. */
function proposalEnd(nodes: PmNode[], i: number, p: Mark): number {
	let j = i + 1;
	for (;;) {
		while (j < nodes.length && sameMark(proposalOf(nodes[j]), p)) j++;
		let k = j;
		while (k < nodes.length && !nodes[k].isText) k++;
		if (k > j && k < nodes.length && sameMark(proposalOf(nodes[k]), p)) {
			j = k;
			continue;
		}
		return j;
	}
}

function inlineMd(nodes: PmNode[], lines: Lines): string {
	let out = '';
	let i = 0;
	while (i < nodes.length) {
		const proposal = proposalOf(nodes[i]);
		let j: number;
		if (proposal) {
			j = proposalEnd(nodes, i, proposal);
		} else {
			j = i + 1;
			while (j < nodes.length && proposalOf(nodes[j]) === null) j++;
		}
		const last = j === nodes.length;
		const critic = proposal ? PROPOSALS[proposal.type.name] : undefined;
		if (critic) {
			out += critic[0] + linkedMd(nodes.slice(i, j), lines, false, last, true) + critic[1];
		} else {
			const lineStart = out === '' || out.endsWith('\n');
			out += linkedMd(nodes.slice(i, j), lines, lineStart, last, false);
		}
		i = j;
	}
	return out;
}

function linkedMd(nodes: PmNode[], lines: Lines, lineStart: boolean, endsBlock: boolean, body: boolean): string {
	let out = '';
	let i = 0;
	while (i < nodes.length) {
		const link = linkOf(nodes[i]);
		let j = i + 1;
		while (j < nodes.length && sameMark(linkOf(nodes[j]), link)) j++;
		const last = endsBlock && j === nodes.length;
		if (link) {
			const text = delimitedMd(nodes.slice(i, j), lines, false, last, body);
			out = escapeBang(out);
			out += `[${text}](${destination(str(link.attrs.href))})`;
		} else {
			const atLineStart = out === '' ? lineStart : out.endsWith('\n');
			out += delimitedMd(nodes.slice(i, j), lines, atLineStart, last, body);
		}
		i = j;
	}
	return out;
}

function delimitedMd(nodes: PmNode[], lines: Lines, lineStartIn: boolean, endsBlock: boolean, body: boolean): string {
	let lastNotBreak = -1;
	nodes.forEach((n, i) => {
		if (n.type.name !== 'hardBreak') lastNotBreak = i;
	});
	let out = '';
	const active: string[] = [];
	const blockStart = lineStartIn;
	let lineStart = lineStartIn;
	// Where the last footnote's marker ends in `out`, and whether it began a line.
	let marker: { end: number; began: boolean } | null = null;
	const closeTo = (keep: number) => {
		// The veil's brackets keep their whitespace inside.
		const moves = active.slice(keep).some((k) => k !== 'veil');
		const trailing = moves ? out.length - out.replace(/ +$/, '').length : 0;
		const spaces = out.slice(out.length - trailing);
		out = out.slice(0, out.length - trailing);
		while (active.length > keep) out += delim(active.pop()!)[1];
		out += spaces;
	};
	nodes.forEach((n, idx) => {
		const wanted = n.marks.map((m) => m.type.name).filter((k) => delim(k)[0] !== '');
		let keep = 0;
		while (keep < Math.min(active.length, wanted.length) && active[keep] === wanted[keep]) keep++;
		closeTo(keep);
		const name = n.type.name;
		if (name === 'text') {
			const t = n.text ?? '';
			const opens = keep < wanted.length;
			// Whitespace at an opening moves outside it, but for the veil's alone.
			const moves = wanted.slice(keep).some((k) => k !== 'veil');
			const leadLen = moves ? t.length - t.replace(/^ +/, '').length : 0;
			if (opens) out += t.slice(0, leadLen);
			for (const k of wanted.slice(keep)) {
				out += delim(k)[0];
				active.push(k);
			}
			const rest = opens ? t.slice(leadLen) : t;
			if (n.marks.some((m) => m.type.name === 'code')) {
				const code = rest.replace(/[\n\r]/g, ' ');
				const ticks = '`'.repeat(longestRun(code, '`') + 1);
				const pad = code.startsWith('`') || code.endsWith('`') ? ' ' : '';
				out += `${ticks}${pad}${code}${pad}${ticks}`;
			} else {
				let escaped = escapeMd(rest, lines, out, body);
				if (n.marks.some((m) => m.type.name === 'veil')) escaped = veiled(escaped);
				// Straight after a footnote's marker, `(` would make it a link's
				// text, and `:` after one that begins a paragraph's line a definition.
				if (marker?.end === out.length && (escaped.startsWith('(') || (marker.began && lines === 'many' && escaped.startsWith(':')))) {
					escaped = `\\${escaped}`;
				}
				if (lineStart && !opens && lines === 'many') {
					const first = blockStart && out === '';
					out += escapeLineStart(escaped, first);
				} else {
					out += escaped;
				}
			}
		} else if (name === 'hardBreak') {
			// A backslash break needs a line after it.
			const trailing = endsBlock && (lastNotBreak < 0 || idx > lastNotBreak);
			if (lines === 'many' && !trailing) {
				out += '\\\n';
				lineStart = true;
				return;
			}
			out += '<br>';
		} else if (name === 'mention') {
			out = escapeBang(out);
			out += `[@${escapeMd(str(n.attrs.label), lines, '', body)}](${destination(str(n.attrs.to))})`;
		} else if (name === 'footnote') {
			// Never escaped: a label is letters, digits, `_` and `-`. Its note
			// is written after the page's last block (`footnoteLines`).
			out += `[^${exportedLabel(n)}]`;
			marker = { end: out.length, began: lineStart };
		}
		lineStart = false;
	});
	closeTo(0);
	return out;
}

/**
 * The definitions of the footnotes in `nodes` (render.rs `footnote_lines`):
 * one line per label, in the order the labels are first cited, `[^label]:
 * note`, its note the first one its footnotes carry. One carrying none
 * writes no line.
 */
export function footnoteLines(nodes: PmNode[]): string[] {
	const labels = new Map<string, string | null>();
	const visit = (n: PmNode) => {
		if (n.type.name === 'footnote') {
			const label = exportedLabel(n);
			const note = typeof n.attrs.note === 'string' ? n.attrs.note : null;
			if (!labels.has(label) || labels.get(label) === null) labels.set(label, note);
			return;
		}
		n.forEach(visit);
	};
	nodes.forEach(visit);
	return [...labels].flatMap(([label, note]) => (note === null ? [] : [`[^${label}]: ${note.replace(/[\r\n]/g, ' ')}`]));
}

/** The labels footnotes are written with while `nodes` are written (`footnotesApart`), by label and note. */
let apartLabels = new Map<string, string>();

const labelKey = (label: string, note: string) => `${label}\u0000${note}`;

/** A footnote's label as the export writes it: its own, or the one `footnotesApart` gave its note. */
function exportedLabel(n: PmNode): string {
	const label = str(n.attrs.label);
	const note = typeof n.attrs.note === 'string' ? n.attrs.note : null;
	return (note !== null && apartLabels.get(labelKey(label, note))) || label;
}

/**
 * The labels footnotes in `nodes` are written with where one label carries
 * more than one note (render.rs `footnotes_as_exported`): the first note a
 * label carries keeps it, and each other note takes a label of its own
 * (`apartLabel`), so every sentence's source is written and reads back
 * beside it. Keyed by label and note.
 */
function footnotesApart(nodes: PmNode[]): Map<string, string> {
	const first = new Map<string, string | null>();
	const used = new Set<string>();
	const order: [string, string][] = [];
	const visit = (n: PmNode) => {
		if (n.type.name === 'footnote') {
			const label = str(n.attrs.label);
			const note = typeof n.attrs.note === 'string' ? n.attrs.note : null;
			used.add(label);
			if (!first.has(label)) first.set(label, null);
			if (note !== null) {
				if (first.get(label) === null) first.set(label, note);
				order.push([label, note]);
			}
			return;
		}
		n.forEach(visit);
	};
	nodes.forEach(visit);
	const given = new Map<string, string | null>();
	for (const [label, note] of order) {
		const held = first.get(label);
		if (held === null || held === undefined || held === note) continue;
		const key = labelKey(label, note);
		if (!given.has(key)) given.set(key, apartLabel(label, used));
	}
	const out = new Map<string, string>();
	for (const [key, label] of given) if (label !== null) out.set(key, label);
	return out;
}

const U64_MAX = (1n << 64n) - 1n;

/**
 * A label for a second note under `label`, one no footnote in `used`
 * carries, which it then holds (render.rs `apart_label`): the label's prefix
 * and the number after the highest any label of that prefix ends in (`ev-2`
 * beside `ev-6` is `ev-7`); a label ending in no digit takes `-2` on, the
 * first one free. Null when no such label fits a label's length.
 */
function apartLabel(label: string, used: Set<string>): string | null {
	const digits = label.length - label.replace(/[0-9]+$/, '').length;
	let prefix: string;
	let from: bigint;
	if (digits === 0) {
		prefix = `${label}-`;
		from = 2n;
	} else {
		prefix = label.slice(0, label.length - digits);
		let highest = 0n;
		for (const l of used) {
			if (!l.startsWith(prefix)) continue;
			const n = l.slice(prefix.length);
			if (!/^[0-9]+$/.test(n)) continue;
			const v = BigInt(n);
			if (v <= U64_MAX && v > highest) highest = v;
		}
		from = highest + 1n > U64_MAX ? U64_MAX : highest + 1n;
	}
	const to = from + 1000n > U64_MAX ? U64_MAX : from + 1000n;
	for (let n = from; n < to; n++) {
		const candidate = `${prefix}${n}`;
		if (used.has(candidate)) continue;
		if (!isLabel(candidate)) return null;
		used.add(candidate);
		return candidate;
	}
	return null;
}

/** `write` with footnotes labelled as the export writes `nodes`' (`footnotesApart`). */
function withLabels<T>(nodes: PmNode[], write: () => T): T {
	const outer = apartLabels;
	apartLabels = footnotesApart(nodes);
	try {
		return write();
	} finally {
		apartLabels = outer;
	}
}

/** Blocks as the page's markdown export writes them: the `content` column's text. */
export function toMarkdown(blocks: Fragment | PmNode[]): string {
	const nodes: PmNode[] = Array.isArray(blocks) ? blocks : [];
	if (!Array.isArray(blocks)) blocks.forEach((n) => nodes.push(n));
	return withLabels(nodes, () => {
		const w = new Md();
		let alt = false;
		nodes.forEach((n, i) => {
			if (i > 0) w.text('\n\n');
			alt = continuesList(nodes, i, alt);
			w.altList = alt;
			blockMd(n, false, w);
		});
		// Last, as the day page's readers find them.
		const notes = footnoteLines(nodes);
		if (notes.length) {
			w.text('\n\n');
			w.text(notes.join('\n'));
		}
		w.text('\n');
		return w.toString();
	});
}

const isItem = (n: PmNode) => n.type.name === 'listItem' || n.type.name === 'taskItem';

/** A table cell's words, on one line: what a spreadsheet takes into one cell. */
function cellWords(cell: PmNode): string {
	return cell.textBetween(0, cell.content.size, ' ').replace(/[\t\r\n]+/g, ' ');
}

/**
 * What a copy out of a block page puts on the clipboard as text: plain words
 * as they read, and anything holding formatting, a link, a widget or more
 * than one block as markdown, which the chat composer and a markdown page
 * read back. Rich editors read the HTML beside it. Selected table cells are
 * their words, a tab between cells and a line between rows, as a
 * spreadsheet copies a range.
 */
export function clipboardText(slice: Slice): string {
	// Into the blocks the copy lies wholly inside: words copied from a list
	// item are the words, not a list of one. A list item is no block alone,
	// so a copy inside one is its blocks, however many.
	let content = slice.content;
	let start = slice.openStart;
	let end = slice.openEnd;
	while (start > 0 && end > 0 && content.childCount === 1) {
		const only = content.firstChild!;
		if (!only.isTextblock && only.childCount !== 1 && !isItem(only)) break;
		content = only.content;
		start--;
		end--;
	}
	const nodes: PmNode[] = [];
	content.forEach((n) => nodes.push(n));
	const role = nodes[0]?.type.spec.tableRole;
	if (role === 'row' || role === 'cell' || role === 'header_cell') {
		const rows = role === 'row' ? nodes.map(children) : [nodes];
		return rows.map((cells) => cells.map(cellWords).join('\t')).join('\n');
	}
	const plainRun = (run: PmNode[]) => run.every((c) => c.isText && !c.marks.length);
	if (nodes.length && nodes[0].isInline) {
		// Words from inside one block, and the sources of their footnotes.
		if (plainRun(nodes)) return nodes.map((n) => n.text).join('');
		return withLabels(nodes, () => {
			const notes = footnoteLines(nodes);
			return notes.length ? `${inlineMd(nodes, 'many')}\n\n${notes.join('\n')}` : inlineMd(nodes, 'many');
		});
	}
	if (nodes.length === 1 && nodes[0].type.name === 'paragraph' && plainRun(children(nodes[0]))) {
		return nodes[0].textContent;
	}
	return toMarkdown(nodes).replace(/\n$/, '');
}
