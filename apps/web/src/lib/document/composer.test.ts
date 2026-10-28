// @vitest-environment happy-dom
/**
 * The chat composer (`composer.ts`, `message.ts`) on the block editor's
 * kernel: what it sends for what is typed (held to what the CodeMirror
 * composer sent, `composer.corpus.ts`), its drafts read back, its keys,
 * refs, fences and pastes.
 */

import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import { getSchema } from '@tiptap/core';
import { createComposer, type Composer, PASTE_AS_FILE_THRESHOLD } from './composer';
import { TYPED, type Step } from './composer.corpus';
import { contractExtensions } from './schema';
import { MESSAGE, messageText, outgoingMessage, readMessage } from './message';
import { settle, useRealConverter } from './test-utils';

vi.hoisted(() => {
	(globalThis as Record<string, unknown>).$state = <T>(v: T) => v;
});

vi.mock('svelte', async (orig) => ({
	...(await orig<typeof import('svelte')>()),
	mount: (component: (target: Element, props: unknown) => void, o: { target: Element; props: unknown }) => {
		component(o.target, o.props);
		return {};
	},
	unmount: () => undefined,
}));

// The mention's widget, drawn as its label.
vi.mock('$lib/components/pages/nodes/MentionNode.svelte', () => ({
	default: (target: Element, props: { view: { attrs: Record<string, unknown> } }) => {
		const el = document.createElement('span');
		el.className = 'stand-in-mention';
		el.textContent = String(props.view.attrs.label);
		target.append(el);
	},
}));

vi.mock('svelte-sonner', () => ({ toast: Object.assign(vi.fn(), { error: vi.fn() }) }));
vi.mock('$lib/stores/contextMenu.svelte', () => ({ contextMenu: { show: vi.fn() } }));
vi.mock('$lib/stores/linkEditor.svelte', () => ({ linkEditor: { show: vi.fn() } }));

beforeAll(() => useRealConverter());

const open: Composer[] = [];
afterEach(() => {
	while (open.length) open.pop()!.destroy();
	document.body.innerHTML = '';
});

interface Harness {
	composer: Composer;
	sent: string[];
	changes: string[];
	attached: File[][];
	mentions: number;
}

function mount(o: { markdown?: string; mobile?: boolean } = {}): Harness {
	const element = document.createElement('div');
	document.body.append(element);
	const h = { sent: [], changes: [], attached: [], mentions: 0 } as unknown as Harness;
	h.composer = createComposer({
		element,
		markdown: o.markdown,
		isMobile: () => !!o.mobile,
		onSubmit: () => h.sent.push(outgoingMessage(h.composer.markdown())),
		onChange: (md) => h.changes.push(md),
		onFocusChange: () => {},
		onHeight: () => {},
		onAttach: (files) => h.attached.push(files),
		onMentionOpen: () => h.mentions++,
	});
	open.push(h.composer);
	return h;
}

/** Type `text` a key at a time, as a keyboard does: input rules and the `@` trigger run. */
function typeIn(c: Composer, text: string) {
	const { view } = c.editor;
	for (const ch of text) {
		const { from, to } = view.state.selection;
		const handled = view.someProp('handleTextInput', (f) => f(view, from, to, ch, () => view.state.tr.insertText(ch, from, to)));
		if (!handled) view.dispatch(view.state.tr.insertText(ch, from, to));
	}
}

const apple = () => /Mac|iP(hone|[oa]d)/.test(navigator.platform);

/** Press `key`, as the browser delivers it; whether the editor took it. */
function press(c: Composer, key: string, mods: { shift?: boolean; mod?: boolean } = {}): boolean {
	const event = new KeyboardEvent('keydown', {
		key,
		shiftKey: !!mods.shift,
		metaKey: !!mods.mod && apple(),
		ctrlKey: !!mods.mod && !apple(),
		bubbles: true,
		cancelable: true,
	});
	c.editor.view.dom.dispatchEvent(event);
	return event.defaultPrevented;
}

/** Paste `text` as the browser hands it over. */
function paste(c: Composer, text: string, files: File[] = []) {
	const data = {
		files,
		items: [],
		types: ['text/plain'],
		getData: (type: string) => (type === 'text/plain' ? text : ''),
	} as unknown as DataTransfer;
	const event = new Event('paste', { bubbles: true, cancelable: true }) as ClipboardEvent;
	Object.defineProperty(event, 'clipboardData', { value: data });
	c.editor.view.dom.dispatchEvent(event);
}

function run(h: Harness, steps: Step[]) {
	for (const s of steps) {
		if (typeof s === 'string') typeIn(h.composer, s);
		else if ('key' in s) press(h.composer, s.key, s);
		else if ('paste' in s) paste(h.composer, s.paste);
		else h.composer.insertRef(...s.ref);
	}
}

describe('what is sent for what is typed', () => {
	for (const t of TYPED) {
		it(t.name, () => {
			const h = mount({ mobile: t.mobile });
			run(h, t.steps);
			const sent = h.sent.at(-1) ?? outgoingMessage(h.composer.markdown());
			expect(sent).toBe(t.sent);
		});
	}

	it('the differences from the CodeMirror composer are the listed ones', () => {
		expect(TYPED.filter((t) => t.was !== undefined).map((t) => t.name)).toEqual([
			'a bullet list, then out of it',
			'a star list',
		]);
	});
});

describe('drafts', () => {
	const schema = getSchema(contractExtensions({}, MESSAGE));

	// Each draft is what `value` held: the message's markdown, as it is kept
	// in localStorage. Read back, it is the same text, whether the converter
	// read it or it stays text.
	const drafts = [
		'',
		'Hello',
		'a\nb',
		'a\n\nb',
		'a\n\n\nb',
		'- apples\n- pears\n\ndone',
		'Shopping:\n- milk\n- eggs',
		'- fruit\n  - kiwi\n- plum',
		'1. one\n2. two',
		'```js\nlet x = 1;\n\nx++;\n```\nafter',
		'> quoted\n> more\n\nafter',
		'this is **bold** and *it* and `code` and ~~gone~~ and ==lit==',
		'rename my_var_name to x_y, 2 * 3 * 4 [1] a < b & c',
		'ask [@Nick](/person/p_nick) about [docs](https://example.com/docs)',
		'# Not a heading',
		'- [ ] buy milk',
		'* a\n* b',
		'1) a',
		'a \\* b &amp; c',
		'| a | b |\n|---|---|\n| 1 | 2 |',
		'  indented\n    more',
		'trailing\n',
		'- ',
	];

	for (const draft of drafts) {
		it(`reads back as written: ${JSON.stringify(draft)}`, () => {
			expect(messageText(readMessage(schema, draft).doc)).toBe(draft);
		});
	}

	it('a draft the converter reads is structure, not text', () => {
		const { doc, reading } = readMessage(schema, 'Shopping:\n- milk\n- eggs\n\nask [@Nick](/person/p_nick)');
		expect(reading).toBe('converted');
		expect(doc.child(1).type.name).toBe('bulletList');
		const mention = doc.child(2).child(1);
		expect(mention.type.name).toBe('mention');
		expect(mention.attrs).toMatchObject({ to: '/person/p_nick', label: 'Nick' });
	});

	it('a draft holding what a message cannot opens as its text', () => {
		expect(readMessage(schema, '# Title\n\nbody').reading).toBe('text');
	});

	it('the composer opens on a draft and reports its markdown as it changes', () => {
		const h = mount({ markdown: '- milk\n- eggs' });
		expect(h.composer.markdown()).toBe('- milk\n- eggs');
		expect(h.composer.editor.state.doc.firstChild?.type.name).toBe('bulletList');
		press(h.composer, 'Enter', { shift: true });
		typeIn(h.composer, 'bread');
		expect(h.changes.at(-1)).toBe('- milk\n- eggs\n- bread');
	});

	it('setMarkdown replaces the message, and its own echo changes nothing', () => {
		const h = mount();
		typeIn(h.composer, 'draft');
		const before = h.composer.editor.state.doc;
		h.composer.setMarkdown('draft');
		expect(h.composer.editor.state.doc).toBe(before);
		h.composer.setMarkdown('');
		expect(h.composer.markdown()).toBe('');
		expect(h.composer.editor.state.selection.from).toBe(1);
	});
});

describe('keys', () => {
	it('Enter sends on a desktop and breaks the line on a phone', () => {
		const desk = mount();
		typeIn(desk.composer, 'one');
		expect(press(desk.composer, 'Enter')).toBe(true);
		expect(desk.sent).toEqual(['one']);

		const phone = mount({ mobile: true });
		typeIn(phone.composer, 'one');
		press(phone.composer, 'Enter');
		typeIn(phone.composer, 'two');
		expect(phone.sent).toEqual([]);
		expect(phone.composer.markdown()).toBe('one\ntwo');
	});

	it('Cmd+Enter sends from anywhere, a code block included', () => {
		for (const mobile of [false, true]) {
			const h = mount({ mobile });
			typeIn(h.composer, '```');
			press(h.composer, 'Enter');
			typeIn(h.composer, 'x');
			press(h.composer, 'Enter', { mod: true });
			expect(h.sent).toEqual(['```\nx\n```']);
		}
	});

	it('in a code block Enter and Shift+Enter are new lines that keep the indent', () => {
		const h = mount();
		typeIn(h.composer, '```');
		press(h.composer, 'Enter');
		expect(h.composer.editor.state.doc.firstChild?.type.name).toBe('codeBlock');
		press(h.composer, 'Tab');
		typeIn(h.composer, 'a');
		press(h.composer, 'Enter');
		typeIn(h.composer, 'b');
		press(h.composer, 'Enter', { shift: true });
		typeIn(h.composer, 'c');
		expect(h.sent).toEqual([]);
		expect(h.composer.markdown()).toBe('```\n  a\n  b\n  c\n```');
	});

	it('Shift+Tab takes an indent back in code, and outside it is left to move the focus', () => {
		const h = mount();
		typeIn(h.composer, '```');
		press(h.composer, 'Enter');
		press(h.composer, 'Tab');
		typeIn(h.composer, 'a');
		press(h.composer, 'Tab', { shift: true });
		expect(h.composer.markdown()).toBe('```\na\n```');

		const plain = mount();
		typeIn(plain.composer, 'hello');
		expect(press(plain.composer, 'Tab', { shift: true })).toBe(false);
	});

	it('Tab outside a list or code is left to move the focus', () => {
		const h = mount();
		typeIn(h.composer, 'hello');
		expect(press(h.composer, 'Tab')).toBe(false);
	});

	it('Shift+Enter on an empty last quote line ends the quote', () => {
		const h = mount();
		typeIn(h.composer, '> quoted');
		press(h.composer, 'Enter', { shift: true });
		press(h.composer, 'Enter', { shift: true });
		typeIn(h.composer, 'after');
		expect(h.composer.markdown()).toBe('> quoted\n\nafter');
	});

	it('Cmd+Z undoes typing', () => {
		const h = mount();
		typeIn(h.composer, 'hello');
		h.composer.editor.commands.undo();
		expect(h.composer.markdown()).toBe('');
	});
});

describe('refs', () => {
	it('`@` at the start of a word opens the picker; in a word it does not', async () => {
		const h = mount();
		typeIn(h.composer, 'mail me@');
		await settle();
		expect(h.mentions).toBe(0);
		typeIn(h.composer, ' @');
		await settle();
		expect(h.mentions).toBe(1);
	});

	it('a pick replaces the `@` and what was typed after it, as a mention with a space after', () => {
		const h = mount();
		typeIn(h.composer, 'ask @Ni');
		h.composer.insertRef('Nick', '/person/p_nick');
		typeIn(h.composer, 'now');
		expect(h.composer.markdown()).toBe('ask [@Nick](/person/p_nick) now');
		expect(outgoingMessage(h.composer.markdown())).toBe('ask [Nick](/person/p_nick) now');
	});

	it('an address elsewhere is a link', () => {
		const h = mount();
		typeIn(h.composer, 'see @');
		h.composer.insertRef('the notes', 'https://example.com/notes');
		expect(h.composer.markdown()).toBe('see [the notes](https://example.com/notes) ');
	});

	it('closing the `@` menu leaves what was typed after it', () => {
		const h = mount();
		typeIn(h.composer, 'hi @Dav');
		h.composer.closeRef();
		expect(h.composer.markdown()).toBe('hi @Dav');
	});
});

describe('paste', () => {
	it('files are attachments', () => {
		const h = mount();
		const file = new File(['x'], 'a.txt');
		paste(h.composer, '', [file]);
		expect(h.attached).toEqual([[file]]);
		expect(h.composer.markdown()).toBe('');
	});

	it('long text is an attachment', () => {
		const h = mount();
		paste(h.composer, 'x'.repeat(PASTE_AS_FILE_THRESHOLD + 1));
		expect(h.attached[0][0].name).toBe('Pasted Text.txt');
		expect(h.composer.markdown()).toBe('');
	});

	it('markdown is converted when a message holds all of it, and is its text otherwise', () => {
		const h = mount();
		paste(h.composer, '* one\n* two');
		expect(h.composer.editor.state.doc.firstChild?.type.name).toBe('bulletList');
		expect(h.composer.markdown()).toBe('- one\n- two');

		const t = mount();
		paste(t.composer, '## Plan\n- one');
		expect(t.composer.editor.state.doc.firstChild?.type.name).toBe('paragraph');
		expect(t.composer.markdown()).toBe('## Plan\n- one');
	});

	it('plain text keeps its lines', () => {
		const h = mount();
		paste(h.composer, 'first\nsecond\n\nthird');
		expect(h.composer.markdown()).toBe('first\nsecond\n\nthird');
	});

	it('in a code block a paste is the text as it is', () => {
		const h = mount();
		typeIn(h.composer, '```');
		press(h.composer, 'Enter');
		paste(h.composer, '- not a list\n**not bold**');
		expect(h.composer.markdown()).toBe('```\n- not a list\n**not bold**\n```');
	});
});
