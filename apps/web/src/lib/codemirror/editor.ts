/**
 * CodeMirror 6 Editor Factory
 *
 * Creates and configures a CodeMirror editor with Yjs collaboration,
 * markdown syntax highlighting, and the Virtues theme.
 */

import { defaultKeymap } from '@codemirror/commands';
import { markdown } from '@codemirror/lang-markdown';
import { bracketMatching, indentOnInput } from '@codemirror/language';
// GFM adds Strikethrough, Table, TaskList to the Lezer markdown parser.
import { GFM } from '@lezer/markdown';
import { languages } from '@codemirror/language-data';
import { EditorState, type Extension } from '@codemirror/state';
import { EditorView, keymap, placeholder as cmPlaceholder } from '@codemirror/view';
import { yCollab, yUndoManagerKeymap } from 'y-codemirror.next';
import type { Awareness } from 'y-protocols/awareness';
import type { Text as YText } from 'yjs';

import { markdownKeybindings } from './extensions/keybindings';
import { listRenumber } from './extensions/list-renumber';
import { renderMode, renderModeCompartment } from './extensions/render-mode';
import { virtuesTheme } from './theme';

export interface CodeMirrorEditorOptions {
	parent: HTMLElement;
	ytext: YText;
	awareness: Awareness;
	readOnly?: boolean;
	placeholder?: string;
	extensions?: Extension[];
	onDocChange?: (content: string) => void;
	/** Start in raw markdown instead of the rendered surface. */
	raw?: boolean;
}

export function createCodeMirrorEditor(options: CodeMirrorEditorOptions): EditorView {
	const {
		parent,
		ytext,
		awareness,
		readOnly = false,
		placeholder = 'Start writing, or press / for commands…',
		extensions: extraExtensions = [],
		onDocChange,
		raw = false,
	} = options;

	const baseExtensions: Extension[] = [
		// Yjs collaboration (sync + cursors + undo via Y.UndoManager)
		yCollab(ytext, awareness),

		// Markdown language support (GFM = Strikethrough + Table + TaskList)
		markdown({ codeLanguages: languages, extensions: GFM }),

		// Basic editing features
		EditorView.lineWrapping,
		indentOnInput(),
		bracketMatching(),

		// Keymaps
		keymap.of([
			...yUndoManagerKeymap,
			...defaultKeymap,
		]),

		// Theme
		virtuesTheme,

		// The rendered surface (see extensions/render-mode.ts) — swapped out
		// wholesale for raw markdown via the compartment.
		renderModeCompartment.of(renderMode(raw)),

		// Keep ordered lists sequential after edits
		listRenumber,

		// Markdown formatting keybindings
		markdownKeybindings,

		// Placeholder
		cmPlaceholder(placeholder),

		// Read-only mode
		EditorView.editable.of(!readOnly),
		EditorState.readOnly.of(readOnly),
	];

	// Doc change listener
	if (onDocChange) {
		baseExtensions.push(
			EditorView.updateListener.of((update) => {
				if (update.docChanged) {
					onDocChange(update.state.doc.toString());
				}
			})
		);
	}

	// Extra extensions (live preview, entity picker, etc.)
	baseExtensions.push(...extraExtensions);

	const view = new EditorView({
		parent,
		state: EditorState.create({
			doc: ytext.toString(),
			extensions: baseExtensions,
		}),
	});

	return view;
}

/** Options for creating a read-only CodeMirror editor (no Yjs) */
export interface ReadOnlyEditorOptions {
	parent: HTMLElement;
	content: string;
}

/** Create a read-only CodeMirror editor for rendering markdown without Yjs */
export function createReadOnlyEditor(options: ReadOnlyEditorOptions): EditorView {
	const { parent, content } = options;

	return new EditorView({
		parent,
		state: EditorState.create({
			doc: content,
			extensions: [
				markdown({ codeLanguages: languages, extensions: GFM }),
				EditorView.lineWrapping,
				virtuesTheme,
				// Reading is always rendered — raw is an authoring escape hatch.
				renderMode(false),
				EditorView.editable.of(false),
				EditorState.readOnly.of(true),
			],
		}),
	});
}
