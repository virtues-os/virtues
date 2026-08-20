/**
 * Render mode — the rendered surface vs. raw markdown
 *
 * The editor's document is always plain markdown; "rendered" is purely a view
 * layer built from decorations. This module owns that bundle as ONE list, so
 * the write path (createCodeMirrorEditor) and the read path
 * (createReadOnlyEditor) cannot drift apart, and so raw mode can drop the whole
 * surface in a single reconfigure.
 *
 * Raw mode removes the extensions entirely rather than asking each one to
 * no-op. That is the point: with the extensions uninstalled there is no way for
 * a stale widget or a half-applied replace to survive the switch, which is what
 * makes raw a trustworthy escape hatch when a construct mis-parses.
 *
 * `shikiHighlight` is deliberately kept in BOTH modes. It is the only member of
 * the bundle that neither hides characters nor inserts a widget — it colors
 * code that is already visible. Dropping it would make raw mode feel broken on
 * code-heavy pages without making anything more legible.
 *
 * Toggled live via a Compartment, following focus-mode.ts; the owning component
 * flips it from the pageDisplay store.
 */

import { Compartment, type Extension } from '@codemirror/state';

import { checkboxes } from './checkboxes';
import { codeBlocks } from './code-blocks';
import { livePreview } from './live-preview';
import { mouseFreeze } from './mouse-freeze';
import { mediaWidgets } from './media-widgets';
import { entityLinks } from './ref-links';
import { reviewMarks } from './review-marks';
import { shikiHighlight } from './shiki-highlight';
import { tables } from './tables';

/** Reconfigurable slot the editor host toggles. */
export const renderModeCompartment = new Compartment();

/**
 * The rendered surface. Order is load-bearing: CodeMirror resolves overlapping
 * decorations by extension precedence, so this list must keep its relative
 * order wherever it is installed.
 */
const renderedSurface: Extension = [
	// The freeze guard ships with the surface, not beside it: it only exists
	// because reveals move text, and raw mode has no reveals to hold back.
	mouseFreeze,
	// Ahead of livePreview: a review span's delimiters are its own, and the
	// higher precedence keeps the general inline pass from claiming characters
	// inside a proposal it does not know about.
	reviewMarks,
	livePreview,
	entityLinks,
	checkboxes,
	mediaWidgets,
	codeBlocks,
	shikiHighlight,
	tables,
];

/** Build the extension set for the given raw state. */
export function renderMode(raw: boolean): Extension {
	return raw ? shikiHighlight : renderedSurface;
}
