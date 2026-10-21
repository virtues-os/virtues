/**
 * Messages typed into the chat composer, a step at a time, and what the
 * composer sends for each: the wire format the server, the thread
 * (`UserMessage.svelte`) and the model read. `sent` is what the CodeMirror
 * composer sent for the same keys, measured by driving it before it was
 * replaced on 2026-10-20; `composer.test.ts` holds the block-editor
 * composer to it. Where the two differ on purpose, `was` keeps the old
 * output and `sent` is the new. All text is invented.
 */

export type Step =
	/** Typed a key at a time. */
	| string
	| { key: 'Enter' | 'Tab' | 'Backspace'; shift?: boolean; mod?: boolean }
	| { paste: string }
	/** The ref picker picked this record at the `@` just typed. */
	| { ref: [label: string, url: string] };

export interface Typed {
	name: string;
	steps: Step[];
	/** On a phone: Enter breaks the line. */
	mobile?: boolean;
	sent: string;
	/** What the CodeMirror composer sent, where the new one sends otherwise. */
	was?: string;
}

const shiftEnter = { key: 'Enter', shift: true } as const;
const enter = { key: 'Enter' } as const;
const modEnter = { key: 'Enter', mod: true } as const;
const tab = { key: 'Tab' } as const;

export const TYPED: Typed[] = [
	{ name: 'plain words', steps: ['Hello there, how are you?'], sent: 'Hello there, how are you?' },
	{ name: 'two lines', steps: ['First line', shiftEnter, 'second line'], sent: 'First line\nsecond line' },
	{ name: 'a blank line between', steps: ['Para one', shiftEnter, shiftEnter, 'para two'], sent: 'Para one\n\npara two' },
	// The old one sent the line after the list straight under it, which
	// markdown reads as part of the last item.
	{
		name: 'a bullet list, then out of it',
		steps: ['- apples', shiftEnter, 'pears', shiftEnter, shiftEnter, 'done'],
		sent: '- apples\n- pears\n\ndone',
		was: '- apples\n- pears\ndone',
	},
	{ name: 'a numbered list', steps: ['1. one', shiftEnter, 'two', shiftEnter, 'three'], sent: '1. one\n2. two\n3. three' },
	{ name: 'a list under a line', steps: ['Shopping:', shiftEnter, '- milk', shiftEnter, 'eggs'], sent: 'Shopping:\n- milk\n- eggs' },
	{ name: 'a nested item', steps: ['- fruit', shiftEnter, tab, 'kiwi', shiftEnter, 'plum'], sent: '- fruit\n  - kiwi\n  - plum' },
	{ name: 'the phone continues a list and the empty item is dropped', mobile: true, steps: ['- a', enter, 'b', enter], sent: '- a\n- b' },
	{ name: 'lines on a phone', mobile: true, steps: ['line one', enter, 'line two'], sent: 'line one\nline two' },
	{ name: 'a fence opened with Enter, sent with Cmd+Enter', steps: ['```', enter, 'let x = 1;', enter, 'x += 2;', modEnter], sent: '```\nlet x = 1;\nx += 2;\n```' },
	{ name: 'a fence with a language and an indent', steps: ['```py', enter, 'def f():', enter, tab, 'return 1', modEnter], sent: '```py\ndef f():\n  return 1\n```' },
	{ name: 'a quote', steps: ['> quoted', shiftEnter, 'more'], sent: '> quoted\n> more' },
	{ name: 'marks typed', steps: ['this is **bold** and *it* and `code` and ~~gone~~'], sent: 'this is **bold** and *it* and `code` and ~~gone~~' },
	{ name: 'words with markdown characters', steps: ['rename my_var_name to x_y, 2 * 3 * 4 [1] a < b & c'], sent: 'rename my_var_name to x_y, 2 * 3 * 4 [1] a < b & c' },
	{ name: 'a link typed', steps: ['see [docs](https://example.com/docs) now'], sent: 'see [docs](https://example.com/docs) now' },
	{ name: 'an address typed', steps: ['open https://example.com/a_b?c=1 please'], sent: 'open https://example.com/a_b?c=1 please' },
	{ name: 'a ref in a line', steps: ['ask @', { ref: ['Nick', '/person/p_nick'] }, 'about it'], sent: 'ask [Nick](/person/p_nick) about it' },
	{ name: 'a ref alone on its line', steps: ['@', { ref: ['David Okafor', '/person/p_david'] }, shiftEnter, 'next'], sent: '[David Okafor](/person/p_david)\nnext' },
	{ name: 'a heading sign stays text', steps: ['# Not a heading', shiftEnter, 'body'], sent: '# Not a heading\nbody' },
	{ name: 'trailing spaces', steps: ['hi   '], sent: 'hi' },
	{ name: 'unicode', steps: ['café ☕ — ok'], sent: 'café ☕ — ok' },
	{ name: 'markdown pasted', steps: [{ paste: 'Steps:\n1. open\n2. close' }], sent: 'Steps:\n1. open\n2. close' },
	{ name: 'a table pasted', steps: [{ paste: '| a | b |\n|---|---|\n| 1 | 2 |' }], sent: '| a | b |\n|---|---|\n| 1 | 2 |' },
	{ name: 'an open fence pasted', steps: [{ paste: '```\ncode' }], sent: '```\ncode\n```' },
	// A list is written with `-`, whatever sign started it.
	{ name: 'a star list', steps: ['* a', shiftEnter, 'b'], sent: '- a\n- b', was: '* a\n* b' },
	{ name: 'a to-do list', steps: ['- [ ] buy milk', shiftEnter, 'call Nick'], sent: '- [ ] buy milk\n- [ ] call Nick' },
];
