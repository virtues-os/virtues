/**
 * Slash Commands Extension
 *
 * Detects `/` at line start or after whitespace by comparing cursor position
 * before and after document changes. This naturally filters out remote Yjs
 * sync (which doesn't move the local cursor to right after the insertion).
 */

import type { Extension } from '@codemirror/state';
import { type EditorView, ViewPlugin, type ViewUpdate } from '@codemirror/view';

export interface SlashCommandCallbacks {
	onOpen: (coords: { x: number; y: number }, from: number) => void;
	onClose: () => void;
	onQueryChange: (query: string) => void;
}

export interface SlashCommand {
	label: string;
	description: string;
	keywords?: string[];
	group: string;
	icon: string;
	execute: (view: EditorView, from: number) => void;
}

interface SlashState {
	active: boolean;
	from: number; // position of /
	query: string;
}

/**
 * Built-in slash commands that insert markdown syntax
 */
export function getDefaultSlashCommands(): SlashCommand[] {
	return [
		{
			label: 'Heading 1',
			description: 'Large section heading',
			keywords: ['h1', 'title'],
			group: 'Basic',
			icon: 'ri:h-1',
			execute: (view, from) => replaceSlash(view, from, '# '),
		},
		{
			label: 'Heading 2',
			description: 'Medium section heading',
			keywords: ['h2', 'subtitle'],
			group: 'Basic',
			icon: 'ri:h-2',
			execute: (view, from) => replaceSlash(view, from, '## '),
		},
		{
			label: 'Heading 3',
			description: 'Small section heading',
			keywords: ['h3'],
			group: 'Basic',
			icon: 'ri:h-3',
			execute: (view, from) => replaceSlash(view, from, '### '),
		},
		{
			label: 'Bullet List',
			description: 'Unordered list',
			keywords: ['ul', 'unordered'],
			group: 'Lists',
			icon: 'ri:list-unordered',
			execute: (view, from) => replaceSlash(view, from, '- '),
		},
		{
			label: 'Numbered List',
			description: 'Ordered list',
			keywords: ['ol', 'ordered'],
			group: 'Lists',
			icon: 'ri:list-ordered',
			execute: (view, from) => replaceSlash(view, from, '1. '),
		},
		{
			label: 'Task List',
			description: 'Checklist with toggles',
			keywords: ['todo', 'checkbox'],
			group: 'Lists',
			icon: 'ri:checkbox-line',
			execute: (view, from) => replaceSlash(view, from, '- [ ] '),
		},
		{
			label: 'Quote',
			description: 'Block quotation',
			keywords: ['blockquote'],
			group: 'Blocks',
			icon: 'ri:double-quotes-l',
			execute: (view, from) => replaceSlash(view, from, '> '),
		},
		{
			label: 'Code Block',
			description: 'Fenced code with syntax highlighting',
			keywords: ['fence', 'pre'],
			group: 'Blocks',
			icon: 'ri:code-box-line',
			execute: (view, from) => {
				const to = view.state.selection.main.head;
				view.dispatch({
					changes: { from, to, insert: '```\n\n```' },
					selection: { anchor: from + 4 },
				});
			},
		},
		{
			label: 'Horizontal Rule',
			description: 'Visual separator',
			keywords: ['hr', 'divider', 'separator'],
			group: 'Blocks',
			icon: 'ri:separator',
			execute: (view, from) => {
				const to = view.state.selection.main.head;
				view.dispatch({
					changes: { from, to, insert: '---\n' },
					selection: { anchor: from + 4 },
				});
			},
		},
		{
			label: 'Table',
			description: 'Insert a markdown table',
			keywords: ['grid'],
			group: 'Advanced',
			icon: 'ri:table-line',
			execute: (view, from) => {
				const to = view.state.selection.main.head;
				const table = '| Header | Header | Header |\n| --- | --- | --- |\n| Cell | Cell | Cell |\n| Cell | Cell | Cell |\n';
				view.dispatch({
					changes: { from, to, insert: table },
					selection: { anchor: from + 2 },
				});
			},
		},
		{
			label: 'Image',
			description: 'Upload or embed an image',
			keywords: ['photo', 'picture'],
			group: 'Media',
			icon: 'ri:image-line',
			execute: (view, from) => {
				const to = view.state.selection.main.head;
				view.dispatch({ changes: { from, to, insert: '' } });
				view.dom.dispatchEvent(
					new CustomEvent('slash-command-image', { bubbles: true, detail: { pos: from } })
				);
			},
		},
		{
			label: 'File',
			description: 'Upload any file (PDF, doc, zip, etc.)',
			keywords: ['upload', 'attachment', 'pdf', 'document'],
			group: 'Media',
			icon: 'ri:attachment-line',
			execute: (view, from) => {
				const to = view.state.selection.main.head;
				view.dispatch({ changes: { from, to, insert: '' } });
				view.dom.dispatchEvent(
					new CustomEvent('slash-command-image', { bubbles: true, detail: { pos: from } })
				);
			},
		},
	];
}

function replaceSlash(view: EditorView, from: number, insert: string) {
	const to = view.state.selection.main.head;
	view.dispatch({
		changes: { from, to, insert },
		selection: { anchor: from + insert.length },
	});
}

/**
 * Filter commands by query string
 */
export function filterSlashCommands(commands: SlashCommand[], query: string): SlashCommand[] {
	if (!query) return commands;
	const q = query.toLowerCase();
	return commands.filter(
		(cmd) =>
			cmd.label.toLowerCase().includes(q) ||
			cmd.keywords?.some((k) => k.includes(q)),
	);
}

/**
 * Create the slash commands extension.
 *
 * Detection strategy: compare cursor position before/after each update.
 * If head moved forward by exactly 1 and that character is `/`, the user
 * just typed it locally. Remote Yjs changes don't move the local cursor
 * to right after the insertion, so this naturally filters them out.
 */
export function createSlashCommands(callbacks: SlashCommandCallbacks): Extension {
	let state: SlashState = { active: false, from: 0, query: '' };

	function close() {
		if (state.active) {
			state = { active: false, from: 0, query: '' };
			callbacks.onClose();
		}
	}

	return ViewPlugin.fromClass(
		class {
			update(update: ViewUpdate) {
				const { head } = update.state.selection.main;

				if (!state.active) {
					if (!update.docChanged) return;

					// Detect: cursor moved forward by exactly 1 char, and that char is /
					const oldHead = update.startState.selection.main.head;
					if (head !== oldHead + 1) return;

					const typed = update.state.sliceDoc(head - 1, head);
					if (typed !== '/') return;

					// Must be at line start or after whitespace
					const slashPos = head - 1;
					if (slashPos > 0) {
						const charBefore = update.state.sliceDoc(slashPos - 1, slashPos);
						if (charBefore && !/[\s\n]/.test(charBefore)) return;
					}

					state = { active: true, from: slashPos, query: '' };
					// coordsAtPos can't be called during update — defer to after layout
					const v = update.view;
					requestAnimationFrame(() => {
						const coords = v.coordsAtPos(slashPos);
						if (coords) {
							callbacks.onOpen({ x: coords.left, y: coords.bottom }, slashPos);
						}
					});
				} else {
					if (!update.docChanged && !update.selectionSet) return;

					const { from } = state;

					if (head <= from) {
						close();
						return;
					}

					if (from >= update.state.doc.length || update.state.sliceDoc(from, from + 1) !== '/') {
						close();
						return;
					}

					const query = update.state.sliceDoc(from + 1, head);

					if (/[\s\n]/.test(query)) {
						close();
						return;
					}

					if (query !== state.query) {
						state.query = query;
						callbacks.onQueryChange(query);
					}
				}
			}

			destroy() {
				close();
			}
		},
	);
}
