// @vitest-environment happy-dom
/**
 * Two implementations of one contract must agree: the Rust crate
 * (`crates/virtues-document`) and the schema this directory builds from the
 * same `contract.json`. For every case in the crate's corpus
 * (`tests/corpus/cases.json`):
 *
 *   1. Canonical HTML is a fixed point: Rust renders it, ProseMirror parses
 *      and re-renders it byte for byte, to the same tree. The model reads a
 *      block, writes it back unchanged, and nothing moves.
 *   2. The Yjs update Rust writes reads back in Tiptap's binding as that tree.
 *   3. An editor bound to that update (the extensions' plugins running, the
 *      table plugin among them) changes nothing in it: after one edit after
 *      the case's blocks, Rust reads back the case's HTML and that edit. A
 *      plugin that rewrote what Rust accepted would write its rewrite into
 *      the shared document with the first keystroke.
 *   4. The Yjs fragment Tiptap's binding writes reads back in Rust as the
 *      HTML ProseMirror renders, inside the contract.
 *   5. The browser's markdown export (`markdown.ts`, what a copy out of a
 *      page writes) is the server's (`render::markdown`), byte for byte.
 *   6. That fragment is not refused by the check the server runs on every
 *      update from a browser (`check_update`), for every case, the ones Rust
 *      refuses included: pasting is the browser's path for exactly that
 *      input, and an update the server refuses costs the tab its connection.
 *
 * happy-dom does not apply one rule of HTML's parser that html5ever (the
 * server's) and browsers do: a line feed straight after `<pre>` is dropped.
 * It is applied here before happy-dom parses (`asParsed`), and Rust writes
 * the extra line feed such text needs, which a browser's serializer leaves
 * out (`asRustWrites`). The Rust side's own fixed-point test
 * (`tests/corpus.rs`) is the authority on rules of the parser like this one.
 *
 * Raw (non-canonical) input is not required to parse the same: ProseMirror's
 * parser restructures some input the server refuses or normalizes, and only
 * HTML pasted in the browser takes that path. Cases Rust refuses are recorded
 * with what ProseMirror does to them silently.
 *
 * The Rust half is the crate's `document-conformance` binary, named by
 * DOCUMENT_CONFORMANCE_BIN (CI builds it; locally:
 * `cargo build -p virtues-document --bin document-conformance`). Without it
 * the cross-language cases skip and say so; the schema check still runs.
 * DOCUMENT_CONFORMANCE_REPORT, if set, is a path to write the per-case
 * agreement report to.
 */

import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { Editor } from '@tiptap/core';
import Collaboration from '@tiptap/extension-collaboration';
import { DOMParser, DOMSerializer, type Node as PmNode } from '@tiptap/pm/model';
import { prosemirrorToYXmlFragment, yXmlFragmentToProseMirrorRootNode } from '@tiptap/y-tiptap';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import * as Y from 'yjs';
import { contract, contractExtensions, contractSchema } from './schema';
import { toMarkdown } from './markdown';

// Not `new URL(…, import.meta.url)`: under happy-dom `URL` is the DOM's, which
// fileURLToPath refuses.
const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../../../..');
const CORPUS = path.join(ROOT, 'crates/virtues-document/tests/corpus/cases.json');
const BIN_ENV = 'DOCUMENT_CONFORMANCE_BIN';
const bin = process.env[BIN_ENV] ? path.resolve(process.env[BIN_ENV]) : undefined;
const reportPath = process.env.DOCUMENT_CONFORMANCE_REPORT;

const schema = contractSchema();
const markRank = Object.keys(contract.marks);

type Json = {
	type: string;
	attrs?: Record<string, unknown>;
	content?: Json[];
	text?: string;
	marks?: Json[];
};
type Case = { name: string; html: string };
type Issue = { message: string };
type RustCase = {
	name: string;
	errors: Issue[];
	notes: Issue[];
	tree?: Json[];
	html?: string;
	markdown?: string;
	update?: string;
};
type RustRead = { name: string; html: string; problems: Issue[]; refused: Issue[] };

/** The edit made after each case's blocks in an editor bound to Rust's Yjs. */
const AFTER = '<p>after</p>';

/** A `<pre>` start tag, attributes and all. */
const PRE = /(<pre\b(?:[^>"']|"[^"]*"|'[^']*')*>)\n/gi;

/** HTML as a browser's parser reads it: the line feed after `<pre>` dropped. */
function asParsed(html: string): string {
	return html.replace(PRE, '$1');
}

/** A serializer's HTML as Rust writes it: that line feed written back. */
function asRustWrites(html: string): string {
	return html.replace(PRE, '$1\n\n');
}

/** One shape for both sides: null ids dropped, attrs sorted, marks in contract order. */
function norm(n: Json): Json {
	const out: Json = { type: n.type };
	if (n.attrs) {
		const attrs = { ...n.attrs };
		if (attrs[contract.id.attr] === null || attrs[contract.id.attr] === undefined) {
			delete attrs[contract.id.attr];
		}
		if (Object.keys(attrs).length) {
			out.attrs = Object.fromEntries(
				Object.entries(attrs).sort(([a], [b]) => a.localeCompare(b)),
			);
		}
	}
	if (n.text !== undefined) out.text = n.text;
	if (n.marks?.length) {
		out.marks = [...n.marks]
			.sort((a, b) => markRank.indexOf(a.type) - markRank.indexOf(b.type))
			.map((m) =>
				m.attrs && Object.keys(m.attrs).length ? { type: m.type, attrs: m.attrs } : { type: m.type },
			);
	}
	if (n.content?.length) out.content = n.content.map(norm);
	return out;
}

function blocks(doc: PmNode): string {
	return JSON.stringify(norm(doc.toJSON() as Json).content ?? []);
}

function pmParse(html: string): { doc: PmNode; domText: string } {
	const div = document.createElement('div');
	div.innerHTML = asParsed(html);
	return { doc: DOMParser.fromSchema(schema).parse(div), domText: div.textContent ?? '' };
}

function pmHtml(doc: PmNode): string {
	const div = document.createElement('div');
	div.appendChild(DOMSerializer.fromSchema(schema).serializeFragment(doc.content));
	return asRustWrites(div.innerHTML);
}

/**
 * Bind an editor to Rust's update, as a client opening the page does, make
 * one edit after the case's blocks, and return the shared document's state.
 */
function editedInAnEditor(update: string): string {
	const ydoc = new Y.Doc();
	Y.applyUpdate(ydoc, Buffer.from(update, 'hex'));
	const editor = new Editor({
		element: document.createElement('div'),
		extensions: [
			...contractExtensions(),
			Collaboration.configure({ document: ydoc, field: contract.fragment }),
		],
	});
	editor.commands.insertContentAt(editor.state.doc.content.size, AFTER);
	editor.destroy();
	return Buffer.from(Y.encodeStateAsUpdate(ydoc)).toString('hex');
}

function run(args: string[]): void {
	execFileSync(bin!, args, { stdio: ['ignore', 'ignore', 'inherit'] });
}

describe('contract schema', () => {
	it('matches the contract', () => {
		for (const [name, spec] of Object.entries(contract.nodes)) {
			const type = schema.nodes[name];
			expect(type, name).toBeDefined();
			if (spec.content !== undefined) expect(type.spec.content, `${name}.content`).toBe(spec.content);
			if (spec.group !== undefined) expect(type.spec.group, `${name}.group`).toBe(spec.group);
			const want = [...Object.keys(spec.attrs ?? {}), ...(spec.id ? [contract.id.attr] : [])].sort();
			expect(Object.keys(type.spec.attrs ?? {}).sort(), `${name}.attrs`).toEqual(want);
		}
		for (const [name, spec] of Object.entries(contract.marks)) {
			const want = Object.keys(spec.attrs ?? {}).sort();
			expect(Object.keys(schema.marks[name].spec.attrs ?? {}).sort(), `${name}.attrs`).toEqual(want);
			if (spec.excludes !== undefined) {
				expect(schema.marks[name].spec.excludes, `${name}.excludes`).toBe(spec.excludes);
			}
		}
		expect(Object.keys(schema.nodes).sort()).toEqual(Object.keys(contract.nodes).sort());
		// Mark order decides nesting in HTML, so it is compared in order: proposals
		// outermost, then link.
		expect(Object.keys(schema.marks)).toEqual(Object.keys(contract.marks));
	});
});

// Straight to stderr: the default reporter shows neither a passing file's
// console output nor skip notes, and a skip nobody sees reads as a pass.
if (!bin) {
	process.stderr.write(
		`conformance.test.ts: the Rust cases are SKIPPED because ${BIN_ENV} is unset. ` +
			'Build the binary with `cargo build -p virtues-document --bin document-conformance` ' +
			`and set ${BIN_ENV} to its path.\n`,
	);
}

const cases: Case[] = JSON.parse(fs.readFileSync(CORPUS, 'utf8'));

describe.skipIf(!bin)(
	bin ? 'conformance with the Rust crate' : `conformance with the Rust crate (skipped: ${BIN_ENV} is unset)`,
	() => {
		let tmp = '';
		let rust: RustCase[] = [];
		const fromJs: { name: string; update: string }[] = [];
		const edited: { name: string; update: string }[] = [];
		const report: Record<string, unknown>[] = [];

		beforeAll(() => {
			tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'document-conformance-'));
			run(['batch', CORPUS, path.join(tmp, 'rust.json')]);
			rust = JSON.parse(fs.readFileSync(path.join(tmp, 'rust.json'), 'utf8'));
			expect(rust.map((r) => r.name)).toEqual(cases.map((c) => c.name));
		});

		afterAll(() => {
			if (reportPath) fs.writeFileSync(reportPath, JSON.stringify(report, null, 2));
			if (tmp) fs.rmSync(tmp, { recursive: true, force: true });
		});

		for (const [i, c] of cases.entries()) {
			it(c.name, () => {
				const r = rust[i];
				const { doc, domText } = pmParse(c.html);

				// Tiptap's binding writes the raw parse, for Rust to read back.
				const out = new Y.Doc();
				prosemirrorToYXmlFragment(doc, out.getXmlFragment(contract.fragment));
				fromJs.push({
					name: c.name,
					update: Buffer.from(Y.encodeStateAsUpdate(out)).toString('hex'),
				});

				if (r.errors.length) {
					// Refused by Rust. Record what ProseMirror would have done, silently.
					const squash = (s: string) => s.replace(/\s+/g, ' ').trim();
					report.push({
						name: c.name,
						rust: 'refused',
						errors: r.errors.map((e) => e.message),
						prosemirror: pmHtml(doc),
						prosemirror_lost_text: squash(domText) !== squash(doc.textContent),
					});
					return;
				}
				const rustTree = JSON.stringify((r.tree ?? []).map(norm));
				const rustHtml = r.html ?? '';

				// Rust's Yjs, read by Tiptap's binding.
				const ydoc = new Y.Doc();
				Y.applyUpdate(ydoc, Buffer.from(r.update ?? '', 'hex'));
				const fromRust = yXmlFragmentToProseMirrorRootNode(
					ydoc.getXmlFragment(contract.fragment),
					schema,
				);

				const canon = pmParse(rustHtml).doc;
				report.push({
					name: c.name,
					rust: 'accepted',
					notes: r.notes.map((n) => n.message),
					raw_trees_agree: blocks(doc) === rustTree,
					raw_html_agree: pmHtml(doc) === rustHtml,
					canonical_fixed_point: pmHtml(canon) === rustHtml && blocks(canon) === rustTree,
					yjs_rust_to_tiptap: blocks(fromRust) === rustTree,
					...(pmHtml(doc) === rustHtml ? {} : { rust_html: rustHtml, pm_html: pmHtml(doc) }),
				});
				expect(pmHtml(canon), `${c.name}: canonical html`).toBe(rustHtml);
				expect(blocks(canon), `${c.name}: canonical tree`).toBe(rustTree);
				expect(blocks(fromRust), `${c.name}: yjs rust → tiptap`).toBe(rustTree);
				// What a copy out of the page writes is the server's own export.
				expect(toMarkdown(fromRust.content), `${c.name}: markdown export`).toBe(r.markdown);
				edited.push({ name: c.name, update: editedInAnEditor(r.update ?? '') });
			});
		}

		it('an editor bound to Rust-written Yjs changes nothing but its edit', () => {
			const accepted = rust.filter((r) => !r.errors.length);
			expect(edited.length, 'no cases reached this test').toBe(accepted.length);
			fs.writeFileSync(path.join(tmp, 'edited.json'), JSON.stringify(edited));
			run(['read-updates', path.join(tmp, 'edited.json'), path.join(tmp, 'edited-read.json')]);
			const back: RustRead[] = JSON.parse(fs.readFileSync(path.join(tmp, 'edited-read.json'), 'utf8'));
			for (const b of back) {
				const r = accepted.find((x) => x.name === b.name)!;
				const entry = report.find((e) => e.name === b.name);
				if (entry) entry.bound_editor_kept_it = b.html === `${r.html}${AFTER}`;
				expect(b.html, `${b.name}: an editor bound to it rewrote it`).toBe(`${r.html}${AFTER}`);
				expect(b.problems.map((p) => p.message), b.name).toEqual([]);
			}
		});

		it('Tiptap-written Yjs reads back in Rust, and the server would take it', () => {
			expect(fromJs.length, 'no cases reached this test').toBe(cases.length);
			fs.writeFileSync(path.join(tmp, 'from-js.json'), JSON.stringify(fromJs));
			run(['read-updates', path.join(tmp, 'from-js.json'), path.join(tmp, 'rust-read.json')]);
			const back: RustRead[] = JSON.parse(fs.readFileSync(path.join(tmp, 'rust-read.json'), 'utf8'));
			expect(back.map((b) => b.name)).toEqual(fromJs.map((f) => f.name));
			for (const [i, b] of back.entries()) {
				const entry = report.find((e) => e.name === b.name);
				const problems = b.problems.map((p) => p.message);
				const refused = b.refused.map((p) => p.message);
				if (entry) entry.yjs_tiptap_problems = problems;
				expect(refused, `${b.name}: the server would refuse what Tiptap wrote`).toEqual([]);
				if (rust[i].errors.length) continue;
				expect(problems, `${b.name}: what Tiptap wrote is outside the contract`).toEqual([]);
				const want = pmHtml(pmParse(cases[i].html).doc);
				if (entry) entry.yjs_tiptap_to_rust = b.html === want;
				expect(b.html, `${b.name}: yjs tiptap → rust`).toBe(want);
			}
		});
	},
);
