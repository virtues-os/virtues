// @vitest-environment happy-dom
/**
 * The CodeMirror editor's outline after `PageOutline` stopped knowing
 * CodeMirror: headings still come from the markdown, and `cmOutlineNav`
 * gives the table of contents what it needs to scroll and spy.
 */
import { afterEach, describe, expect, it } from 'vitest';
import type { EditorView } from '@codemirror/view';
import { createTestView, destroyTestView } from './test-utils';
import { cmOutlineNav, extractHeadings } from './outline';
import { filterCommands } from '$lib/components/menuCommand';
import { getDefaultSlashCommands } from './extensions/slash-commands';

let view: EditorView | undefined;
afterEach(() => {
	if (view) destroyTestView(view);
	view = undefined;
});

describe('the CodeMirror outline', () => {
	const DOC = '# Title\n\nText\n\n```\n# not a heading\n```\n\n## Section\n';

	it('extracts h1 to h3, skipping code', () => {
		expect(extractHeadings(DOC).map((h) => [h.level, h.text])).toEqual([
			[1, 'Title'],
			[2, 'Section'],
		]);
	});

	it('gives the outline its scroller, each heading’s top, and a scroll that lands on it', () => {
		view = createTestView(DOC);
		const nav = cmOutlineNav(view);
		const [title, section] = extractHeadings(DOC);
		expect(nav.scroller).toBe(view.scrollDOM);
		expect(nav.topOf(title)).toBe(view.lineBlockAt(title.from).top);
		expect(() => nav.scrollTo(section)).not.toThrow();
	});
});

describe('the CodeMirror insert menu on the shared filter', () => {
	const commands = getDefaultSlashCommands();

	it('finds a command by its label or a keyword', () => {
		expect(filterCommands(commands, 'task').map((c) => c.label)).toEqual(['To-do list']);
		expect(filterCommands(commands, 'h2').map((c) => c.label)).toEqual(['Heading 2']);
		expect(filterCommands(commands, 'quote').map((c) => c.label)).toEqual(['Quote']);
	});

	it('shows every command, in order, for an empty query', () => {
		expect(filterCommands(commands, '')).toEqual(commands);
	});
});
