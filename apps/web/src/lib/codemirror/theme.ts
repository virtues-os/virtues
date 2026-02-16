/**
 * CodeMirror Theme
 *
 * Virtues editor theme for CodeMirror.
 * Uses CSS custom properties for theming consistency.
 */

import { EditorView } from '@codemirror/view';

export const virtuesTheme = EditorView.theme({
	'&': {
		fontFamily: 'var(--font-sans, ui-sans-serif, system-ui, -apple-system, sans-serif)',
		fontSize: '1rem',
		lineHeight: '1.6',
		color: 'var(--color-foreground)',
	},
	'& .cm-content': {
		fontFamily: 'var(--font-sans, ui-sans-serif, system-ui, -apple-system, sans-serif)',
		caretColor: 'var(--color-primary)',
		padding: '8px 0',
	},
	'& .cm-line': {
		padding: '0 4px',
	},
	'&.cm-focused': {
		outline: 'none',
	},
	'.cm-cursor, .cm-dropCursor': {
		borderLeftColor: 'var(--color-primary)',
	},
	'.cm-selectionBackground': {
		background: 'var(--color-highlight) !important',
	},
	'&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground': {
		background: 'var(--color-highlight) !important',
	},
	'.cm-activeLine': {
		backgroundColor: 'transparent',
	},
	'.cm-activeLineGutter': {
		backgroundColor: 'transparent',
	},
	'.cm-gutters': {
		backgroundColor: 'transparent',
		borderRight: 'none',
	},
	'.cm-lineNumbers .cm-gutterElement': {
		fontFamily: 'var(--font-mono, ui-monospace, monospace)',
		fontSize: '0.7rem',
		color: 'var(--color-foreground-subtle)',
		padding: '0 8px',
		minWidth: '2rem',
		textAlign: 'center',
	},
	'.cm-scroller': {
		overflow: 'visible',
	},
	// Yjs remote cursors
	'.cm-ySelectionInfo': {
		fontSize: '0.7rem',
		fontFamily: 'var(--font-sans)',
		padding: '1px 4px',
		borderRadius: '3px',
		opacity: '0.8',
	},
});
