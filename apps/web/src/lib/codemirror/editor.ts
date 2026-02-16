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
import { EditorView, keymap, lineNumbers, placeholder as cmPlaceholder } from '@codemirror/view';
import { yCollab, yUndoManagerKeymap } from 'y-codemirror.next';
import type { Awareness } from 'y-protocols/awareness';
import type { Text as YText } from 'yjs';

import { checkboxes } from './extensions/checkboxes';
import { codeBlocks } from './extensions/code-blocks';
import { entityLinks } from './extensions/entity-links';
import { markdownKeybindings } from './extensions/keybindings';
import { livePreview } from './extensions/live-preview';
import { mediaWidgets } from './extensions/media-widgets';
import { shikiHighlight } from './extensions/shiki-highlight';
import { tables } from './extensions/tables';
import { virtuesTheme } from './theme';

export interface CodeMirrorEditorOptions {
	parent: HTMLElement;
	ytext: YText;
	awareness: Awareness;
	readOnly?: boolean;
	placeholder?: string;
	showLineNumbers?: boolean;
	extensions?: Extension[];
	onDocChange?: (content: string) => void;
}

export function createCodeMirrorEditor(options: CodeMirrorEditorOptions): EditorView {
	const {
		parent,
		ytext,
		awareness,
		readOnly = false,
		placeholder = 'Type / for commands, @ for entities...',
		showLineNumbers = false,
		extensions: extraExtensions = [],
		onDocChange,
	} = options;

	const baseExtensions: Extension[] = [
		// Yjs collaboration (sync + cursors + undo via Y.UndoManager)
		yCollab(ytext, awareness),

		// Markdown language support (GFM = Strikethrough + Table + TaskList)
		// @ts-expect-error — @lezer/common version mismatch between hoisted and pnpm copies
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

		// Live preview decorations
		livePreview,
		entityLinks,
		checkboxes,
		mediaWidgets,
		codeBlocks,
		shikiHighlight,
		tables,

		// Markdown formatting keybindings
		markdownKeybindings,

		// Placeholder
		cmPlaceholder(placeholder),

		// Read-only mode
		EditorView.editable.of(!readOnly),
		EditorState.readOnly.of(readOnly),
	];

	// Optional line numbers gutter
	if (showLineNumbers) {
		baseExtensions.push(lineNumbers());
	}

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
				// @ts-expect-error — @lezer/common version mismatch
				markdown({ codeLanguages: languages, extensions: GFM }),
				EditorView.lineWrapping,
				virtuesTheme,
				livePreview,
				entityLinks,
				checkboxes,
				mediaWidgets,
				codeBlocks,
				shikiHighlight,
				tables,
				EditorView.editable.of(false),
				EditorState.readOnly.of(true),
			],
		}),
	});
}
