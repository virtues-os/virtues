/**
 * CodeMirror Theme
 *
 * Virtues editor theme for CodeMirror.
 * Uses CSS custom properties for theming consistency.
 */

import { EditorView } from '@codemirror/view';

export const virtuesTheme = EditorView.theme({
	'&': {
		fontFamily: 'var(--editor-font-family, var(--font-sans, ui-sans-serif, system-ui, -apple-system, sans-serif))',
		fontSize: 'var(--editor-font-size, 1rem)',
		lineHeight: 'var(--editor-line-height, 1.7)',
		color: 'var(--color-foreground)',
	},
	'& .cm-content': {
		fontFamily: 'var(--editor-font-family, var(--font-sans, ui-sans-serif, system-ui, -apple-system, sans-serif))',
		// The native caret is suppressed in favor of the rendered one that
		// `extensions/caret.ts` draws and animates. Nothing else may turn this
		// back on: two carets on one line is worse than either alone.
		caretColor: 'transparent',
		padding: '8px 0',
	},
	'& .cm-line': {
		// Horizontal gutter + a touch of inter-paragraph rhythm (padding, never
		// margin — margin collapses and creates dead click-zones in CM6).
		padding: '0.12rem 4px',
	},
	'&.cm-focused': {
		outline: 'none',
	},
	// `.cm-cursor` gets its shape and color from theme.css (the caret is a bar,
	// not a border); the drop cursor stays a plain rule.
	'.cm-dropCursor': {
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
