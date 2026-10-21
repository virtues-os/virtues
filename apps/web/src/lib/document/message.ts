/**
 * A chat message in the composer: what part of the page contract it holds,
 * how its markdown is read in and written out, and what is sent.
 *
 * The composer runs on the block editor's kernel (`composer.ts`), so a
 * message is a ProseMirror document of the contract's own nodes. What the
 * server gets is markdown, written by the page's export in its message mode
 * (`messageMarkdown` in `markdown.ts`): structure as markdown, words as
 * typed, since the thread shows a message as it was written
 * (`UserMessage.svelte`). Markdown is read by the page's converter
 * (`converter.ts`), as a paste: each line a line, a raw tag that is no HTML
 * kept as text. There is no second parser or serializer here.
 */

import { DOMParser as PmDOMParser, Fragment, type Node as PmNode, type Schema } from '@tiptap/pm/model';
import { getSchema } from '@tiptap/core';
import { convertNow, type ConvertResult } from './converter';
import { messageMarkdown } from './markdown';
import { contractExtensions, type ContractPart } from './schema';

/**
 * What a message holds: paragraphs, lists, a quote, code, line breaks,
 * mentions, and the marks typed in a line. No headings, tables, media,
 * applets or to-dos: the thread shows a message as typed, so `# ` and
 * `- [ ] ` stay the text they are.
 */
export const MESSAGE: ContractPart = {
	nodes: ['doc', 'paragraph', 'text', 'hardBreak', 'blockquote', 'bulletList', 'orderedList', 'listItem', 'codeBlock', 'mention'],
	marks: ['link', 'bold', 'italic', 'strike', 'highlight', 'code'],
};

/** A message's markdown, as the composer sends it and keeps its draft. */
export function messageText(doc: PmNode): string {
	return messageMarkdown(doc.content);
}

/**
 * Text as blocks with no markdown read in it: paragraphs a blank line
 * apart, each line in one a line break. `messageText` writes them back as
 * the same text.
 */
export function textBlocks(schema: Schema, text: string): Fragment {
	const paragraphs = text
		.replace(/\r\n?/g, '\n')
		.split('\n\n')
		.map((para) => {
			const inline: PmNode[] = [];
			para.split('\n').forEach((line, i) => {
				if (i > 0) inline.push(schema.nodes.hardBreak.create());
				if (line) inline.push(schema.text(line));
			});
			return schema.nodes.paragraph.create(null, inline);
		});
	return Fragment.from(paragraphs);
}

function parseHtml(schema: Schema, html: string): PmNode {
	const div = document.createElement('div');
	div.innerHTML = html;
	return PmDOMParser.fromSchema(schema).parse(div);
}

/** How a message's markdown was read: by the converter, as its text, or as its text with no converter to run. */
export type Reading = 'converted' | 'text' | 'unavailable';

/**
 * A message's markdown as the composer's document: the converter's reading
 * of it as a paste, when that writes back as the same text, and otherwise
 * the text as it is (`textBlocks`). A draft or a prefill opens exactly as
 * it was kept: `1) a`, `# Title` or `a \* b`, which a reading would change,
 * stay their text.
 */
export function readMessage(schema: Schema, markdown: string, convert = convertPaste): { doc: PmNode; reading: Reading } {
	const text = markdown.replace(/\r\n?/g, '\n');
	const asText = (reading: Reading) => ({ doc: schema.nodes.doc.create(null, textBlocks(schema, text)), reading });
	if (!text.trim()) return asText('text');
	const result = convert(text);
	if (result.kind === 'unavailable') return asText('unavailable');
	if (result.kind !== 'converted') return asText('text');
	const doc = parseHtml(schema, result.html);
	return messageText(doc) === text ? { doc, reading: 'converted' } : asText('text');
}

const convertPaste = (markdown: string): ConvertResult => convertNow(markdown, 'paste');

let pageSchema: Schema | null = null;

/**
 * Whether the converter's HTML holds only what a message does: pasted
 * markdown with a heading, a table, an image or a to-do in it goes in as
 * its text, every character kept, rather than losing what a message
 * cannot show.
 */
export function holdsOnlyMessage(html: string): boolean {
	pageSchema ??= getSchema(contractExtensions());
	const doc = parseHtml(pageSchema, html);
	let fits = true;
	doc.descendants((node) => {
		if (!MESSAGE.nodes.includes(node.type.name) || node.marks.some((m) => !MESSAGE.marks.includes(m.type.name))) fits = false;
		return fits;
	});
	return fits;
}

/** A fence line: three or more backticks or tildes, optionally an info string. */
const FENCE_LINE = /^\s*(`{3,}|~{3,})/;

/**
 * Close a fence the writer left open, so the message is well-formed markdown
 * for every renderer that will ever see it. Walks lines the way CommonMark
 * does: a fence opens on a backtick or tilde run and closes on a run of the
 * same character at least as long. A code block always writes its fences;
 * this is for one typed as text.
 */
export function closeOpenFence(text: string): string {
	let open: string | null = null;
	for (const line of text.split('\n')) {
		const m = FENCE_LINE.exec(line);
		if (!m) continue;
		const run = m[1];
		if (open === null) open = run;
		else if (run[0] === open[0] && run.length >= open.length && line.trim() === run) open = null;
	}
	return open === null ? text : `${text}\n${open}`;
}

/** An empty list item on the message's last line, to-do brackets included. */
const TRAILING_EMPTY_MARKER = /\n\s*(?:[-*+]|\d+[.)])(?:\s+\[[ xX]\])?\s*$/;

/**
 * What is sent for a message's markdown. A mention is written `[@Label](url)`
 * so a draft reads back as one; the thread and the model have always seen
 * `[Label](url)`, so the `@` comes off. A trailing empty list item is
 * dropped: on a phone Return continues the list, so the last keystroke
 * before Send leaves one. A fence left open is closed.
 */
export function outgoingMessage(markdown: string): string {
	const text = markdown
		.replace(/\[@([^\]]+)\]\(/g, '[$1](')
		.replace(/\s+$/, '')
		.replace(TRAILING_EMPTY_MARKER, '')
		.trim();
	return closeOpenFence(text);
}
