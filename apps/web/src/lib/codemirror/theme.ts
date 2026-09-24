/**
 * CodeMirror Theme
 *
 * Virtues editor theme for CodeMirror.
 * Uses CSS custom properties for theming consistency.
 */

import { EditorView } from '@codemirror/view';

export const virtuesTheme = EditorView.theme({
	'&': {
		// The editor stays in the UI register (a writing surface is not a
		// printed article, and the decorations are measured against these
		// metrics), but it reads the shared tokens rather than restating them —
		// the hardcoded `1rem` here was one of the five body sizes the app had
		// drifted into. `--editor-font-*` still wins where a caller sets it.
		fontFamily: 'var(--editor-font-family, var(--md-body-family, ui-sans-serif, system-ui, -apple-system, sans-serif))',
		fontSize: 'var(--editor-font-size, var(--md-body-size, 1rem))',
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
	// drawSelection paints the selection on a layer BEHIND the content (CM sets
	// its z-index to -1 inline), so any construct with an opaque background —
	// code blocks, inline code, ==highlight==, review marks, images — hides the
	// selection completely. The layer is lifted above the content instead and
	// blended: multiply on light themes leaves dark text dark and tints every
	// background, screen does the same for light text on dark themes. The
	// token is set per scheme in themes.css. pointer-events: none keeps clicks
	// reaching the text; the caret layer is already above (z-index 150).
	//
	// The blend goes on each rect, never on the layer: WebKit (the Mac and iOS
	// apps) silently drops `mix-blend-mode` on the layer itself, so the lifted
	// selection painted opaque and erased the text under it. Chromium blends
	// either way, which is why it only showed in the apps.
	'& .cm-selectionLayer': {
		zIndex: '1 !important',
		pointerEvents: 'none',
	},
	'.cm-selectionBackground': {
		background: 'var(--color-highlight) !important',
		mixBlendMode: 'var(--selection-blend, multiply)',
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
