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

function childrenMd(n: PmNode, sep: string, w: Md): void {
	children(n).forEach((c, i) => {
		if (i > 0) w.text(sep);
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
		const items = children(n);
		const loose = items.some((item) => item.childCount > 1);
		const start = typeof n.attrs.start === 'number' && n.attrs.start >= 0 ? n.attrs.start : 1;
		items.forEach((item, i) => {
			if (i > 0) w.text(loose ? '\n\n' : '\n');
			const marker =
				name === 'orderedList'
					? `${start + i}. `
					: name === 'taskList'
						? `- [${item.attrs.checked === true ? 'x' : ' '}] `
						: '- ';
			w.openContainer(marker, ' '.repeat(name === 'taskList' ? 2 : marker.length));
			let firstEmpty = false;
			children(item).forEach((b, k) => {
				if (k > 0) {
					// After an empty first block, a blank line would end the item.
					const afterEmpty = k === 1 && firstEmpty;
					w.text(loose && !afterEmpty ? '\n\n' : '\n');
				}
				const before = w.writes;
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
			const fence = mark.repeat(Math.max(longestRun(text, mark) + 1, 3));
			return `${fence}${lang}\n${text}\n${fence}`;
		}
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
		default:
			return ['', ''];
	}
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
	const closeTo = (keep: number) => {
		const trailing = out.length - out.replace(/ +$/, '').length;
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
			const leadLen = t.length - t.replace(/^ +/, '').length;
			const opens = keep < wanted.length;
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
				const escaped = escapeMd(rest, lines, out, body);
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
		}
		lineStart = false;
	});
	closeTo(0);
	return out;
}

/** Blocks as the page's markdown export writes them: the `content` column's text. */
export function toMarkdown(blocks: Fragment | PmNode[]): string {
	const nodes: PmNode[] = Array.isArray(blocks) ? blocks : [];
	if (!Array.isArray(blocks)) blocks.forEach((n) => nodes.push(n));
	const w = new Md();
	nodes.forEach((n, i) => {
		if (i > 0) w.text('\n\n');
		blockMd(n, false, w);
	});
	w.text('\n');
	return w.toString();
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
 * spreadsheet copies a range and the CodeMirror editor's table copied one.
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
		// Words from inside one block.
		return plainRun(nodes) ? nodes.map((n) => n.text).join('') : inlineMd(nodes, 'many');
	}
	if (nodes.length === 1 && nodes[0].type.name === 'paragraph' && plainRun(children(nodes[0]))) {
		return nodes[0].textContent;
	}
	return toMarkdown(nodes).replace(/\n$/, '');
}
