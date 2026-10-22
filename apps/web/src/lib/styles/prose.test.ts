/**
 * What rendered text looks like is prose.css's alone. Chat replies, pages
 * and the composer each kept their own copy of it once, and the copies
 * disagreed (agents/plan/prose-consolidation-plan.md). This test fails when
 * a stylesheet outside prose.css styles a text element (a paragraph, a
 * heading, a list, a link, code, a table...) under one of the renderers'
 * roots, so a copy cannot grow back.
 *
 * A surface still owns what is only its own: the editor's state (a selected
 * block, the empty-page placeholder), paper (`@media print`), and the few
 * rules in ALLOWED, each with its reason. A fix for how text looks goes in
 * prose.css, or in a --prose-* token the surface sets.
 */

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const SRC = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const PROSE_CSS = path.join(SRC, 'lib/styles/prose.css');

/** The class each renderer puts on its root. */
const ROOT = /\.(?:prose|markdown|doc-prose|composer-field)(?![\w-])/;

/** The elements markdown renders as text. */
const TEXT_ELEMENTS = [
	'p', 'h1', 'h2', 'h3', 'h4', 'h5', 'h6', 'a', 'strong', 'b', 'em', 'i', 's', 'u', 'del', 'mark', 'code', 'pre',
	'ul', 'ol', 'li', 'blockquote', 'hr', 'img', 'table', 'thead', 'tbody', 'tr', 'th', 'td', 'sup', 'sub', 'dl', 'dt', 'dd',
];
const ELEMENT = new RegExp(`(?:^|[\\s>+~(,])(?:${TEXT_ELEMENTS.join('|')})(?=$|[\\s>+~),.:#\\[])`);

/** Classes ProseMirror and Tiptap put on a node for the editor's state, not its look. */
const EDITOR_STATE = /\.(?:ProseMirror-selectednode|is-editor-empty)(?![\w-])/;

/** Rules a surface keeps, by file and selector as written (`:global()` unwrapped). */
const ALLOWED: { file: string; selector: string; why: string }[] = [
	{
		file: 'lib/document/document.css',
		selector: '.doc-prose a',
		why: "A link on a page is text to edit, so it shows the caret, not the hand.",
	},
	{
		file: 'lib/document/document.css',
		selector: '.doc-prose a:has(.doc-ref-link)',
		why: 'A link to something on the box is drawn as a ref by the editor.',
	},
	{
		file: 'lib/document/document.css',
		selector: ':where(.doc-prose > *, .doc-prose li, .doc-prose .doc-widget)',
		why: 'The transition a dragged block, list item or widget comes back with.',
	},
	{
		file: 'lib/components/Markdown.svelte',
		selector: '.markdown li',
		why: 'Each list item is its own layout root while a reply streams in.',
	},
	{
		file: 'lib/components/tabs/views/ChatView.svelte',
		selector: '.messages-container.room .message-wrapper[data-subject="gs:welcome"] .markdown p:first-of-type',
		why: "The welcome message's first line is set as a standfirst.",
	},
];

interface Rule {
	file: string;
	selector: string;
	print: boolean;
}

function files(dir: string): string[] {
	return fs.readdirSync(dir, { withFileTypes: true }).flatMap((e) => {
		const p = path.join(dir, e.name);
		if (e.isDirectory()) return files(p);
		return /\.(css|svelte)$/.test(e.name) ? [p] : [];
	});
}

/** The CSS a file holds: all of a stylesheet, a component's `<style>` blocks. */
function css(file: string): string {
	const text = fs.readFileSync(file, 'utf8');
	if (file.endsWith('.css')) return text;
	return [...text.matchAll(/<style[^>]*>([\s\S]*?)<\/style>/g)].map((m) => m[1]).join('\n');
}

/** `:global(x)` as `x`, for any nesting of parentheses inside it. */
function unwrapGlobal(selector: string): string {
	let out = selector;
	for (let at = out.indexOf(':global('); at !== -1; at = out.indexOf(':global(')) {
		let depth = 0;
		let end = at + ':global('.length;
		for (; end < out.length; end++) {
			if (out[end] === '(') depth++;
			else if (out[end] === ')') {
				if (depth === 0) break;
				depth--;
			}
		}
		out = out.slice(0, at) + out.slice(at + ':global('.length, end) + out.slice(end + 1);
	}
	return out;
}

/** A selector list's selectors, split on commas outside parentheses and brackets. */
function split(list: string): string[] {
	const parts: string[] = [];
	let depth = 0;
	let start = 0;
	for (let i = 0; i < list.length; i++) {
		const c = list[i];
		if (c === '(' || c === '[') depth++;
		else if (c === ')' || c === ']') depth--;
		else if (c === ',' && depth === 0) {
			parts.push(list.slice(start, i));
			start = i + 1;
		}
	}
	parts.push(list.slice(start));
	return parts.map((s) => s.replace(/\s+/g, ' ').trim()).filter(Boolean);
}

/** Every style rule's selector, and whether it sits inside `@media print`. */
function rules(file: string): Rule[] {
	const text = css(file).replace(/\/\*[\s\S]*?\*\//g, '');
	const out: Rule[] = [];
	const open: string[] = [];
	let from = 0;
	for (let i = 0; i < text.length; i++) {
		const c = text[i];
		if (c === '{') {
			const prelude = text.slice(from, i).trim();
			open.push(prelude);
			if (!prelude.startsWith('@')) {
				const selector = unwrapGlobal(prelude);
				// A whole-rule `:where(a, b)` is one selector as written; keep it whole.
				const selectors = selector.startsWith(':where(') && selector.endsWith(')') ? [selector] : split(selector);
				const print = open.some((p) => /^@media\b[^{]*\bprint\b/.test(p));
				for (const s of selectors) out.push({ file: path.relative(SRC, file), selector: s, print });
			}
			from = i + 1;
		} else if (c === '}') {
			open.pop();
			from = i + 1;
		} else if (c === ';') {
			from = i + 1;
		}
	}
	return out;
}

/** The part of a selector after its renderer root, if it has one. */
function underRoot(selector: string): string | null {
	const m = ROOT.exec(selector);
	return m ? selector.slice(m.index + m[0].length) : null;
}

/** Whether a selector styles a text element under a root. */
function stylesText(selector: string): boolean {
	const rest = underRoot(selector);
	if (rest === null) return false;
	// Attribute values and strings name no element.
	const bare = rest.replace(/\[[^\]]*\]/g, '[]').replace(/"[^"]*"|'[^']*'/g, '""');
	return ELEMENT.test(bare);
}

const all = files(SRC)
	.filter((f) => f !== PROSE_CSS)
	.flatMap(rules);

describe('text styles live in prose.css', () => {
	it('finds the rules it is meant to check', () => {
		// The parser reads prose.css's own element rules; were it broken, every
		// check below would pass on nothing.
		const own = rules(PROSE_CSS).filter((r) => stylesText(r.selector));
		expect(own.length).toBeGreaterThan(40);
		expect(stylesText(unwrapGlobal('.markdown :global(p)'))).toBe(true);
		expect(stylesText('.doc-prose .tableWrapper')).toBe(false);
		expect(stylesText('.prose-note')).toBe(false);
		expect(stylesText('.prose ul[data-type="taskList"] > li')).toBe(true);
	});

	it('no other stylesheet styles a text element under a renderer root', () => {
		const copies = all
			.filter((r) => stylesText(r.selector))
			.filter((r) => !r.print && !EDITOR_STATE.test(r.selector))
			.filter((r) => !ALLOWED.some((a) => a.file === r.file && a.selector === r.selector))
			.map((r) => `${r.file}: ${r.selector}`);
		expect(copies, 'Move these to prose.css, or set a --prose-* token').toEqual([]);
	});

	it('every allowed rule still exists', () => {
		const stale = ALLOWED.filter((a) => !all.some((r) => r.file === a.file && r.selector === a.selector));
		expect(stale.map((a) => `${a.file}: ${a.selector}`)).toEqual([]);
	});

	it('no component gives its own element the class `prose`', () => {
		// A scoped `.prose` in a component also takes every rule of prose.css.
		const local = all.filter((r) => /^\.prose(?![\w-])/.test(r.selector) && !r.file.startsWith('lib/styles/'));
		expect(local.map((r) => `${r.file}: ${r.selector}`)).toEqual([]);
	});
});
