/**
 * Helpers for the block editor's tests: pages as Yjs documents, written the
 * way the server writes them when the conformance binary is at hand
 * (`DOCUMENT_CONFORMANCE_BIN`), and editors bound to them or to plain HTML.
 * Not shipped: only test files import this.
 */

import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { Editor, type AnyExtension } from '@tiptap/core';
import { DOMParser as PmDOMParser, DOMSerializer, type Node as PmNode } from '@tiptap/pm/model';
import { prosemirrorToYXmlFragment } from '@tiptap/y-tiptap';
import * as Y from 'yjs';
import { Awareness } from 'y-protocols/awareness';
import { contract, contractExtensions, contractSchema, idTypes, newId } from './schema';

const BIN = process.env.DOCUMENT_CONFORMANCE_BIN;

/**
 * `html` with a block id on every block that carries one, as every page on
 * a server has (the server gives each block one when it writes it).
 */
export function withIds(html: string): string {
	const schema = contractSchema();
	const div = document.createElement('div');
	div.innerHTML = html;
	const parsed = PmDOMParser.fromSchema(schema).parse(div);
	const give = (node: PmNode): PmNode => {
		const content = node.content.size ? node.content : null;
		const kids: PmNode[] = [];
		content?.forEach((child) => kids.push(give(child)));
		if (node.isText) return node;
		const attrs = idTypes.includes(node.type.name) && !node.attrs[contract.id.attr]
			? { ...node.attrs, [contract.id.attr]: newId() }
			: node.attrs;
		return node.type.create(attrs, kids, node.marks);
	};
	const out = document.createElement('div');
	out.appendChild(DOMSerializer.fromSchema(schema).serializeFragment(give(parsed).content));
	return out.innerHTML;
}

/**
 * A page holding `html`, every block with an id, as the Rust crate writes
 * it; null without the conformance binary.
 */
export function rustDoc(html: string): Y.Doc | null {
	if (!BIN) return null;
	html = withIds(html);
	const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'document-editor-'));
	try {
		const input = path.join(dir, 'in.json');
		const output = path.join(dir, 'out.json');
		fs.writeFileSync(input, JSON.stringify([{ name: 'fixture', html }]));
		execFileSync(path.resolve(BIN), ['batch', input, output], { stdio: ['ignore', 'ignore', 'inherit'] });
		const [out] = JSON.parse(fs.readFileSync(output, 'utf8')) as { errors: unknown[]; update?: string }[];
		if (out.errors.length || !out.update) throw new Error(`the fixture is outside the contract: ${JSON.stringify(out.errors)}`);
		const doc = new Y.Doc();
		Y.applyUpdate(doc, Buffer.from(out.update, 'hex'));
		doc.getMap('meta').set('contract', contract.version);
		return doc;
	} finally {
		fs.rmSync(dir, { recursive: true, force: true });
	}
}

/**
 * A page holding `html`, every block with an id, written by Tiptap's own
 * binding: for when the binary is not built.
 */
export function jsDoc(html: string): Y.Doc {
	const div = document.createElement('div');
	div.innerHTML = withIds(html);
	const node = PmDOMParser.fromSchema(contractSchema()).parse(div);
	const doc = new Y.Doc();
	prosemirrorToYXmlFragment(node, doc.getXmlFragment(contract.fragment));
	doc.getMap('meta').set('contract', contract.version);
	return doc;
}

/** The page as the server would hold it: Rust's when it can be, Tiptap's otherwise. */
export function pageDoc(html: string): { doc: Y.Doc; writtenBy: 'rust' | 'tiptap' } {
	const rust = rustDoc(html);
	return rust ? { doc: rust, writtenBy: 'rust' } : { doc: jsDoc(html), writtenBy: 'tiptap' };
}

/** The provider an editor's carets go through, without a socket; `emit` plays its events. */
export function fakeProvider(ydoc: Y.Doc) {
	const awareness = new Awareness(ydoc);
	const handlers = new Map<string, ((...args: unknown[]) => void)[]>();
	return {
		awareness,
		on(event: string, handler: (...args: unknown[]) => void) {
			handlers.set(event, [...(handlers.get(event) ?? []), handler]);
		},
		off(event: string, handler: (...args: unknown[]) => void) {
			handlers.set(event, (handlers.get(event) ?? []).filter((h) => h !== handler));
		},
		emit(event: string, ...args: unknown[]) {
			for (const h of handlers.get(event) ?? []) h(...args);
		},
		destroy: () => awareness.destroy(),
	};
}

/** An editor on `html` alone: the contract's schema and `extensions`, no shared document. */
export function htmlEditor(html: string, extensions: AnyExtension[] = []): Editor {
	const element = document.createElement('div');
	document.body.append(element);
	return new Editor({ element, content: html, extensions: [...contractExtensions(), ...extensions] });
}

/** The position just inside the first textblock whose text is `text`, plus `offset`. */
export function posIn(editor: Editor, text: string, offset = 0): number {
	let found = -1;
	editor.state.doc.descendants((node, pos) => {
		if (found >= 0) return false;
		if (node.isTextblock && node.textContent.includes(text)) {
			found = pos + 1 + node.textContent.indexOf(text) + offset;
			return false;
		}
		return true;
	});
	if (found < 0) throw new Error(`no block holds ${JSON.stringify(text)}`);
	return found;
}

/** Let timers, frames and microtasks run. */
export async function settle(ms = 20): Promise<void> {
	await new Promise((r) => setTimeout(r, ms));
}
