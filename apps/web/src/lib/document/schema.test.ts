// @vitest-environment happy-dom
/**
 * The schema built from the contract, on its own: the places where a stock
 * Tiptap extension would read HTML differently from the contract, and so from
 * the server. The cross-language checks are in conformance.test.ts.
 */

import { Editor } from '@tiptap/core';
import { DOMParser, DOMSerializer, type Node as PmNode } from '@tiptap/pm/model';
import { describe, expect, it } from 'vitest';
import {
	contract,
	contractExtensions,
	contractSchema,
	fits,
	normalizeUrl,
	typed,
	type AttrSpec,
} from './schema';

const schema = contractSchema();

function parse(html: string): PmNode {
	const div = document.createElement('div');
	div.innerHTML = html;
	return DOMParser.fromSchema(schema).parse(div);
}

function render(doc: PmNode): string {
	const div = document.createElement('div');
	div.appendChild(DOMSerializer.fromSchema(schema).serializeFragment(doc.content));
	return div.innerHTML;
}

function cell(html: string): PmNode {
	return parse(`<table><tr>${html}</tr></table>`).firstChild!.firstChild!.firstChild!;
}

describe('headings', () => {
	const levels = contract.nodes.heading.attrs!.level.enum as number[];

	it('lists levels 1 to 6', () => {
		expect(levels).toEqual([1, 2, 3, 4, 5, 6]);
	});

	// An attribute parser that answers anything but null for an absent
	// attribute overrides the tag's own level: h2 would parse as level 1.
	it.each(levels)('h%i parses at its own level and renders back', (level) => {
		const html = `<h${level}>Title</h${level}>`;
		const doc = parse(html);
		expect(doc.firstChild!.attrs.level).toBe(level);
		expect(render(doc)).toBe(html);
	});
});

describe('table cell alignment', () => {
	it.each(['td', 'th'])('%s reads and writes the align attribute', (tag) => {
		const node = cell(`<${tag} align="center"><p>x</p></${tag}>`);
		expect(node.attrs.align).toBe('center');
		expect(render(parse(`<table><tr><${tag} align="center"><p>x</p></${tag}></tr></table>`))).toContain(
			`<${tag} align="center">`,
		);
	});

	it('an unaligned cell has no align attribute', () => {
		const doc = parse('<table><tr><td><p>x</p></td></tr></table>');
		expect(doc.firstChild!.firstChild!.firstChild!.attrs.align).toBeNull();
		expect(render(doc)).toContain('<td>');
	});

	it('an alignment outside the contract is dropped', () => {
		expect(cell('<td align="justify"><p>x</p></td>').attrs.align).toBeNull();
	});
});

describe('marks', () => {
	it('code excludes only code, so bold code keeps both marks', () => {
		const text = parse('<p><strong><code>x</code></strong></p>').firstChild!.firstChild!;
		expect(text.marks.map((m) => m.type.name).sort()).toEqual(['bold', 'code']);
	});

	it('a link needs an href it would accept', () => {
		const marks = (html: string) => parse(html).firstChild!.firstChild!.marks.map((m) => m.type.name);
		expect(marks('<p><a href="https://example.com">x</a></p>')).toEqual(['link']);
		expect(marks('<p><a href="mailto:nick@example.com">x</a></p>')).toEqual(['link']);
		expect(marks('<p><a href="/person/p_1">x</a></p>')).toEqual(['link']);
		expect(marks('<p><a>x</a></p>')).toEqual([]);
		expect(marks('<p><a href="javascript:alert(1)">x</a></p>')).toEqual([]);
		// What a browser strips before reading the scheme hides nothing.
		expect(marks('<p><a href="java&#9;script:alert(1)">x</a></p>')).toEqual([]);
		expect(marks('<p><a href="java&#10;script:alert(1)">x</a></p>')).toEqual([]);
		expect(marks('<p><a href="&#1;javascript:alert(1)">x</a></p>')).toEqual([]);
		expect(marks('<p><a href="data:text/html,x">x</a></p>')).toEqual([]);
		expect(marks('<p><a href="zotero://select/items/1">x</a></p>')).toEqual([]);
	});
});

describe('pasted formatting', () => {
	// The shape of a Google Docs copy: everything inside a bold that is not.
	const googleDocs =
		'<b style="font-weight:normal;" id="docs-internal-guid-1"><p><span style="font-weight:400">plain text</span></p>' +
		'<p><span style="font-weight:700">bold text</span></p></b>';

	it('a Google Docs wrapper is not bold, and its bold spans are', () => {
		expect(render(parse(googleDocs))).toBe('<p>plain text</p><p><strong>bold text</strong></p>');
	});

	it('a tag its own style sets back to plain carries no mark', () => {
		expect(render(parse('<p><i style="font-style:normal">x</i></p>'))).toBe('<p>x</p>');
		expect(render(parse('<p><b style="font-weight: 400">x</b></p>'))).toBe('<p>x</p>');
		expect(render(parse('<p><b>x</b> <i>y</i></p>'))).toBe('<p><strong>x</strong> <em>y</em></p>');
	});

	it('inline styles map onto marks', () => {
		expect(render(parse('<p><span style="font-style:italic">i</span></p>'))).toBe('<p><em>i</em></p>');
		expect(render(parse('<p><span style="text-decoration:line-through">s</span></p>'))).toBe('<p><s>s</s></p>');
		expect(render(parse('<p><span style="text-decoration:underline">u</span></p>'))).toBe('<p><u>u</u></p>');
	});
});

describe('required node attributes', () => {
	const types = (html: string) => {
		const out: string[] = [];
		parse(html).descendants((n) => {
			out.push(n.type.name);
		});
		return out;
	};

	// Each of these, built with the attribute null, would be written to the
	// shared document and refused by the server.
	it('an image without a src it would accept is no image', () => {
		expect(types('<img src="/a.png" alt="x">')).toContain('image');
		expect(types('<img alt="no src">')).not.toContain('image');
		expect(types('<img data-src="https://e.example/a.png" alt="lazy">')).not.toContain('image');
		expect(types('<img src="javascript:alert(1)">')).not.toContain('image');
	});

	it('a mention without its route or label is its text', () => {
		const doc = parse('<p>See <virtues-mention to="/person/p_1">Nick</virtues-mention>.</p>');
		expect(types('<p><virtues-mention label="Nick"></virtues-mention></p>')).not.toContain('mention');
		expect(doc.textContent).toBe('See Nick.');
		expect(render(doc)).toBe('<p>See Nick.</p>');
	});

	it('an applet without a ref is no applet', () => {
		expect(types('<virtues-applet ref="sleep-week"></virtues-applet>')).toContain('applet');
		expect(types('<virtues-applet></virtues-applet>')).not.toContain('applet');
	});
});

describe('attributes', () => {
	it('an absent attribute takes the default', () => {
		expect(parse('<ol><li><p>x</p></li></ol>').firstChild!.attrs.start).toBe(1);
		expect(parse('<ol start="3"><li><p>x</p></li></ol>').firstChild!.attrs.start).toBe(3);
		const task = parse('<ul data-type="taskList"><li data-type="taskItem"><p>x</p></li></ul>');
		expect(task.firstChild!.firstChild!.attrs.checked).toBe(false);
	});

	it('block ids read from and write to data-id', () => {
		expect(render(parse('<p data-id="a1b2c3d4">x</p>'))).toBe('<p data-id="a1b2c3d4">x</p>');
		const doc = parse('<p>x</p>');
		expect(doc.firstChild!.attrs[contract.id.attr]).toBeNull();
		expect(render(doc)).toBe('<p>x</p>');
	});

	it('typed() answers what the server would accept, and null for the rest', () => {
		const int: AttrSpec = { type: 'int' };
		expect(typed(int, ' 12 ')).toBe(12);
		expect(typed(int, '')).toBeNull();
		expect(typed(int, '0x10')).toBeNull();
		expect(typed(int, '1.5')).toBeNull();
		expect(typed(int, null)).toBeNull();

		const bool: AttrSpec = { type: 'bool' };
		expect(typed(bool, '')).toBe(true);
		expect(typed(bool, 'false')).toBe(false);
		expect(typed(bool, 'yes')).toBeNull();

		const url: AttrSpec = { type: 'url' };
		expect(typed(url, ' https://example.com ')).toBe('https://example.com');
		expect(typed(url, ' JavaScript:alert(1)')).toBeNull();
		expect(typed(url, 'java\tscript:alert(1)')).toBeNull();
		expect(typed(url, '\u0001javascript:alert(1)')).toBeNull();
		expect(typed(url, 'tel:+15125550100')).toBe('tel:+15125550100');
		expect(typed(url, '?q=a:b')).toBe('?q=a:b');
		expect(normalizeUrl(' \u0001https://ex\tample.com/\n ')).toBe('https://example.com/');

		expect(typed({ type: 'string', enum: ['note', 'tip'] }, 'tip')).toBe('tip');
		expect(typed({ type: 'string', enum: ['note', 'tip'] }, 'other')).toBeNull();
	});
});

/** An editor on the contract's extensions, holding `html`. */
function editorWith(html: string): Editor {
	return new Editor({ element: document.createElement('div'), extensions: contractExtensions(), content: html });
}

/** Type `text` at the cursor one character at a time, as a person does, input rules running. */
function type(editor: Editor, text: string) {
	for (const ch of text) {
		const { from, to } = editor.state.selection;
		const handled = editor.view.someProp('handleTextInput', (f) => f(editor.view, from, to, ch, () => editor.state.tr));
		if (!handled) editor.view.dispatch(editor.state.tr.insertText(ch, from, to));
	}
}

function links(editor: Editor): string[] {
	const out: string[] = [];
	editor.state.doc.descendants((n) => {
		for (const m of n.marks) if (m.type.name === 'link') out.push(m.attrs.href);
	});
	return out;
}

function images(editor: Editor): string[] {
	const out: string[] = [];
	editor.state.doc.descendants((n) => {
		if (n.type.name === 'image') out.push(n.attrs.src);
	});
	return out;
}

// The server closes the socket of an editor that writes anything outside the
// contract, so every way an editor writes is held to it, not only parsing.
describe('what an editor writes stays inside the contract', () => {
	it('a link goes only to a scheme a page may hold', () => {
		for (const href of ['ftp://files.example.com', 'sms:+15125550100', 'xmpp:nick@example.com', 'javascript:alert(1)']) {
			const editor = editorWith('<p>text</p>');
			editor.commands.selectAll();
			expect(editor.commands.setLink({ href }), href).toBe(false);
			expect(links(editor), href).toEqual([]);
			editor.destroy();
		}
		for (const href of ['https://example.com', 'mailto:nick@example.com', '/person/p_1']) {
			const editor = editorWith('<p>text</p>');
			editor.commands.selectAll();
			expect(editor.commands.setLink({ href }), href).toBe(true);
			expect(links(editor), href).toEqual([href]);
			editor.destroy();
		}
	});

	it('typing a URL links it only when a page may hold it', () => {
		const editor = editorWith('<p></p>');
		type(editor, 'see ftp://files.example.com and https://example.com ');
		expect(links(editor)).toEqual(['https://example.com']);
		editor.destroy();
		const link = contractExtensions().find((e) => e.name === 'link')!;
		expect(link.options.shouldAutoLink('ftp://files.example.com')).toBe(false);
		expect(link.options.shouldAutoLink('https://example.com')).toBe(true);
	});

	it('an image takes only a source a page may hold', () => {
		for (const src of ['javascript:alert(1)', 'data:text/html,hi', 'file:///Users/x/a.png']) {
			const editor = editorWith('<p>x</p>');
			expect(editor.commands.setImage({ src }), src).toBe(false);
			expect(images(editor), src).toEqual([]);
			editor.destroy();
		}
		const editor = editorWith('<p>x</p>');
		expect(editor.commands.setImage({ src: '/api/drive/files/1/download' })).toBe(true);
		expect(images(editor)).toEqual(['/api/drive/files/1/download']);
		editor.destroy();
	});

	it('the markdown image input rule takes only a source a page may hold', () => {
		const refused = editorWith('<p></p>');
		type(refused, '![x](data:text/html,hi)');
		expect(images(refused)).toEqual([]);
		expect(refused.state.doc.textContent).toBe('![x](data:text/html,hi)');
		refused.destroy();
		const taken = editorWith('<p></p>');
		type(taken, '![x](/a.png)');
		expect(images(taken)).toEqual(['/a.png']);
		taken.destroy();
	});

	it('any other command that would write a refused value writes nothing', () => {
		const editor = editorWith('<h2>Title</h2><p>text</p><img src="/a.png">');
		const before = editor.getHTML();
		editor.commands.selectAll();
		editor.commands.setMark('link', { href: 'ftp://files.example.com' });
		editor.commands.insertContentAt(0, { type: 'heading', attrs: { level: 9 }, content: [{ type: 'text', text: 'x' }] });
		editor.commands.insertContentAt(0, { type: 'mention', attrs: { label: 'Nick' } });
		editor.commands.setNodeSelection(editor.state.doc.content.size - 1);
		editor.commands.updateAttributes('image', { src: 'javascript:alert(1)' });
		expect(editor.getHTML()).toBe(before);
		editor.destroy();
	});

	it('ordinary editing is untouched', () => {
		const editor = editorWith('<p>a</p>');
		editor.commands.setTextSelection(2);
		type(editor, ' bold');
		editor.commands.selectAll();
		editor.commands.toggleBold();
		editor.commands.setHeading({ level: 3 });
		editor.commands.insertContentAt(editor.state.doc.content.size, '<ul data-type="taskList"><li data-type="taskItem"><p>t</p></li></ul>');
		editor.commands.insertContentAt(editor.state.doc.content.size, '<p>end</p>');
		editor.commands.setTextSelection(editor.state.doc.content.size - 1);
		editor.commands.insertTable({ rows: 2, cols: 2, withHeaderRow: true });
		editor.commands.addColumnAfter();
		editor.commands.mergeOrSplit();
		const html = editor.getHTML();
		expect(html).toContain('<h3><strong>a bold</strong></h3>');
		expect(html).toContain('data-type="taskList"');
		expect(html).toContain('<table>');
		expect(html.match(/<th/g)?.length).toBe(3);
		editor.destroy();
	});

	it('fits() answers what the server checks', () => {
		const span: AttrSpec = { type: 'int', min: 1, default: 1 };
		expect(fits(span, 2)).toBe(true);
		expect(fits(span, 0)).toBe(false);
		expect(fits(span, 1.5)).toBe(false);
		expect(fits(span, null)).toBe(true);
		expect(fits({ type: 'url', required: true }, null)).toBe(false);
		expect(fits({ type: 'url', required: true }, 'tel:+15125550100')).toBe(true);
		expect(fits({ type: 'url', required: true }, 'ftp://x')).toBe(false);
		expect(fits({ type: 'string', enum: ['left', 'right'] }, 'center')).toBe(false);
		expect(typed(span, '0')).toBeNull();
		expect(typed(span, '-2')).toBeNull();
		expect(typed(span, '3')).toBe(3);
	});

	// The server reads back every integer a JavaScript number holds exactly,
	// and refuses the rest, so the two agree on each value either writes.
	it('an int is a safe integer within its range, as the server checks', () => {
		const width = contract.nodes.image.attrs!.width;
		for (const n of [2147483648, 9_000_000_000_000_000, Number.MAX_SAFE_INTEGER, -Number.MAX_SAFE_INTEGER]) {
			expect(typed(width, String(n)), String(n)).toBe(n);
			expect(fits(width, n), String(n)).toBe(true);
		}
		expect(typed(width, '9007199254740992')).toBeNull();
		expect(fits(width, 9007199254740992)).toBe(false);
		expect(fits(width, BigInt('9007199254740993'))).toBe(false);

		const colspan = contract.nodes.tableCell.attrs!.colspan;
		const rowspan = contract.nodes.tableCell.attrs!.rowspan;
		expect(typed(colspan, '1000')).toBe(1000);
		expect(typed(colspan, '1001')).toBeNull();
		expect(fits(colspan, 1001)).toBe(false);
		expect(typed(rowspan, '65534')).toBe(65534);
		expect(typed(rowspan, '65535')).toBeNull();
		expect(cell('<td colspan="1001"><p>x</p></td>').attrs.colspan).toBe(1);
	});

	it('a mention points only at a ref on the box', () => {
		const to = contract.nodes.mention.attrs!.to;
		for (const route of ['/person/p_1', '/page/pg_1?page=2', '/wiki-article/x#a']) {
			expect(typed(to, route), route).toBe(route);
			expect(fits(to, route), route).toBe(true);
		}
		for (const other of [
			'javascript:alert(1)',
			'data:text/html,x',
			'https://evil.example.com/x',
			'//evil.example.com/x',
			'/\\evil.example.com/x',
			'/\tperson/x',
			'/person/',
			'/person/?q=1',
			'person/p_1',
		]) {
			expect(typed(to, other), other).toBeNull();
			expect(fits(to, other), other).toBe(false);
		}
		expect(render(parse('<p>See <virtues-mention to="javascript:alert(1)" label="Nick"></virtues-mention></p>'))).toBe(
			'<p>See</p>',
		);
	});
});
