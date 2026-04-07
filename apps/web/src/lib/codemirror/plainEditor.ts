/**
 * Plain Code Editor Factory
 *
 * Creates a CodeMirror 6 editor for plain code editing (Python, etc.)
 * without Yjs collaboration. Used for agent activation code editing.
 */

import { defaultKeymap, history, historyKeymap } from '@codemirror/commands';
import { python } from '@codemirror/lang-python';
import { bracketMatching, indentOnInput } from '@codemirror/language';
import { EditorState, type Extension } from '@codemirror/state';
import { EditorView, keymap, lineNumbers, placeholder as cmPlaceholder } from '@codemirror/view';
import { virtuesTheme } from './theme';

export interface PlainEditorOptions {
	parent: HTMLElement;
	content: string;
	readOnly?: boolean;
	placeholder?: string;
	onChange?: (content: string) => void;
}

/**
 * Create a plain code editor (no Yjs, no markdown).
 * Includes Python syntax highlighting, line numbers, bracket matching.
 */
export function createPlainEditor(options: PlainEditorOptions): EditorView {
	const {
		parent,
		content,
		readOnly = false,
		placeholder = '# Activation code (Python)...',
		onChange,
	} = options;

	const extensions: Extension[] = [
		python(),
		EditorView.lineWrapping,
		indentOnInput(),
		bracketMatching(),
		lineNumbers(),
		history(),
		keymap.of([...defaultKeymap, ...historyKeymap]),
		virtuesTheme,
		codeEditorTheme,
		cmPlaceholder(placeholder),
		EditorView.editable.of(!readOnly),
		EditorState.readOnly.of(readOnly),
	];

	if (onChange) {
		extensions.push(
			EditorView.updateListener.of((update) => {
				if (update.docChanged) {
					onChange(update.state.doc.toString());
				}
			})
		);
	}

	return new EditorView({
		parent,
		state: EditorState.create({
			doc: content,
			extensions,
		}),
	});
}

/** Theme overrides for code editing (monospace font, tighter line height) */
const codeEditorTheme = EditorView.theme({
	'&': {
		fontFamily: 'var(--font-mono, ui-monospace, monospace)',
		fontSize: '0.875rem',
		lineHeight: '1.5',
	},
	'& .cm-content': {
		fontFamily: 'var(--font-mono, ui-monospace, monospace)',
		padding: '12px 0',
	},
	'& .cm-scroller': {
		overflow: 'auto',
	},
});
