/**
 * The one markdown language definition
 *
 * Every editor surface (the page editor, its read-only twin, the chat
 * composer, and the test harness) parses the same dialect: CommonMark, GFM
 * (strikethrough, tables, task lists), and `==highlight==`. Building the
 * language in one place is what keeps that true — the highlight parser was
 * added here once, and no surface can drift into parsing `==` differently.
 */

import { markdown } from '@codemirror/lang-markdown';
import { languages } from '@codemirror/language-data';
import type { Extension } from '@codemirror/state';
import { GFM } from '@lezer/markdown';

import { highlightExtension } from './extensions/highlight-parser';

export interface VirtuesMarkdownOptions {
	/**
	 * Resolve fenced-code info strings to nested languages. Default true; the
	 * chat composer turns it off because nothing is highlighted in a message
	 * box and the language descriptions are dead weight there.
	 */
	codeLanguages?: boolean;
}

/** The markdown dialect the editor speaks, as a CodeMirror extension. */
export function virtuesMarkdown(options: VirtuesMarkdownOptions = {}): Extension {
	const { codeLanguages = true } = options;
	return markdown({
		...(codeLanguages ? { codeLanguages: languages } : {}),
		extensions: [GFM, highlightExtension],
	});
}
