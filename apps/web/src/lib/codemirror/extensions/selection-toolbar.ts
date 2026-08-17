/**
 * Selection Toolbar Extension
 *
 * A floating formatting toolbar over the current selection. The toolbar itself
 * is trivial; its MANNERS are the whole design, because a bar that appears
 * uninvited over the words you are reading is worse than no bar at all. Three
 * rules, each fixing a way the old version intruded:
 *
 *  1. NEVER DURING A DRAG. The previous version ran a flat 200ms debounce on
 *     every selection change, so the toolbar surfaced on top of the text while
 *     the user was still sweeping through it. It now stays hidden for the whole
 *     mouse interaction and appears on release, when the selection is final and
 *     the user is done looking at what is underneath.
 *
 *  2. KEYBOARD SELECTIONS WAIT. Shift+Arrow arrives one character at a time; an
 *     immediate toolbar would flash on every keystroke. The delay only applies
 *     here — a mouse release is deliberate and gets the toolbar at once.
 *
 *  3. ESCAPE MEANS ESCAPE. Dismissing leaves the selection intact (so the user
 *     keeps their place) and is REMEMBERED for that selection, so the next
 *     cursor blink or remote edit does not summon the bar straight back. Moving
 *     the selection asks the question again.
 *
 * The drag state comes from mouse-freeze.ts, which already tracks it on the
 * window (drags routinely end outside the editor) for the reveal guard.
 */

import { type EditorState, type Extension, StateEffect, StateField } from '@codemirror/state';
import { type EditorView, ViewPlugin, type ViewUpdate } from '@codemirror/view';

import { inlineMarks } from './inline-marks';
import { dragJustEnded, isMouseSelecting } from './mouse-freeze';

/** How long a keyboard selection must hold still before the toolbar appears. */
const KEYBOARD_SHOW_DELAY_MS = 200;

/** Identifies a selection, so a dismissal can be scoped to exactly that one. */
function selectionKey(state: EditorState): string {
	const { from, to } = state.selection.main;
	return `${from}:${to}`;
}

const dismissEffect = StateEffect.define<null>();

/**
 * The selection the user dismissed the toolbar for, if any. Cleared as soon as
 * the selection moves — a different selection is a different question.
 */
const dismissedSelection = StateField.define<string | null>({
	create: () => null,
	update(value, tr) {
		for (const effect of tr.effects) {
			if (effect.is(dismissEffect)) return selectionKey(tr.state);
		}
		if (value !== null && value !== selectionKey(tr.state)) return null;
		return value;
	},
});

/**
 * Dismiss the toolbar without touching the selection. Call this from the host's
 * Escape handling; hiding the component alone is not enough, because the plugin
 * would show it again on the very next update.
 */
export function dismissSelectionToolbar(view: EditorView) {
	view.dispatch({ effects: dismissEffect.of(null) });
}

export interface SelectionToolbarCallbacks {
	onShow: (coords: { x: number; y: number }, activeFormats: Set<string>) => void;
	onHide: () => void;
}

const CLS_TO_FORMAT: Record<string, string> = {
	'cm-strong': 'bold',
	'cm-emphasis': 'italic',
	'cm-inline-code': 'code',
	'cm-strikethrough': 'strikethrough',
	'cm-underline': 'underline',
	'cm-highlight': 'highlight',
};

/**
 * Which formats apply to the selection — answered by inline-marks.ts, the
 * same source the renderer uses.
 *
 * The previous version sniffed two raw characters either side of the
 * selection, so the toolbar's B/I state was only right when the selection
 * happened to hug the delimiters exactly: selecting one word inside a longer
 * bold run showed bold as OFF. A format is active when the selection sits
 * inside the construct's content or spans the whole construct.
 */
function getActiveFormats(view: EditorView): Set<string> {
	const { from, to } = view.state.selection.main;
	const formats = new Set<string>();
	if (from === to) return formats;

	const lineFrom = view.state.doc.lineAt(from).from;
	const lineTo = view.state.doc.lineAt(to).to;

	for (const mark of inlineMarks(view.state, lineFrom, lineTo)) {
		const format = CLS_TO_FORMAT[mark.cls];
		if (!format) continue;
		const insideContent = from >= mark.openTo && to <= mark.closeFrom;
		const spansConstruct = from <= mark.openFrom && to >= mark.closeTo;
		if (insideContent || spansConstruct) formats.add(format);
	}

	return formats;
}

/**
 * Create the selection toolbar extension with callbacks.
 */
export function createSelectionToolbar(callbacks: SelectionToolbarCallbacks): Extension {
	const plugin = ViewPlugin.fromClass(
		class {
			private active = false;
			private showTimer: ReturnType<typeof setTimeout> | null = null;

			constructor(private readonly view: EditorView) {}

			update(update: ViewUpdate) {
				// Rule 1. Held for the whole mouse interaction, not just the
				// moment of the press — the selection changes continuously
				// under a drag and every one of those would be a chance to
				// pop up over the text being swept.
				if (isMouseSelecting(update.state)) {
					this.hide();
					return;
				}

				// A drag ending changes no selection and sets no doc, so it has
				// to be a trigger in its own right — it is the moment a mouse
				// selection becomes final.
				const released = dragJustEnded(update);
				const dismissChanged =
					update.startState.field(dismissedSelection, false) !==
					update.state.field(dismissedSelection, false);
				if (!update.selectionSet && !update.docChanged && !released && !dismissChanged) {
					return;
				}

				const { state } = update;
				const { from, to } = state.selection.main;

				if (from === to) {
					this.hide();
					return;
				}

				// Rule 3. Asked and answered for this selection.
				if (state.field(dismissedSelection, false) === selectionKey(state)) {
					this.hide();
					return;
				}

				// Formatting a fence line is meaningless.
				if (state.doc.lineAt(from).text.trimStart().startsWith('```')) {
					this.hide();
					return;
				}

				// Rule 2. A mouse release is deliberate and gets the toolbar at
				// once; a keyboard selection is still arriving, so it waits.
				this.scheduleShow(released ? 0 : KEYBOARD_SHOW_DELAY_MS);
			}

			private scheduleShow(delay: number) {
				this.clearTimer();
				if (delay === 0) {
					this.show();
					return;
				}
				this.showTimer = setTimeout(() => {
					this.showTimer = null;
					this.show();
				}, delay);
			}

			private show() {
				const { view } = this;
				// The view can be torn down while a timer is in flight (tab
				// close, navigation), and coordsAtPos on a detached view throws.
				if (!view.dom.isConnected) return;

				const { from, to } = view.state.selection.main;
				const coordsFrom = view.coordsAtPos(from);
				const coordsTo = view.coordsAtPos(to, -1);
				if (!coordsFrom || !coordsTo) return;

				this.active = true;
				callbacks.onShow(
					{ x: (coordsFrom.left + coordsTo.right) / 2, y: Math.min(coordsFrom.top, coordsTo.top) },
					getActiveFormats(view)
				);
			}

			private clearTimer() {
				if (this.showTimer !== null) {
					clearTimeout(this.showTimer);
					this.showTimer = null;
				}
			}

			hide() {
				this.clearTimer();
				if (this.active) {
					this.active = false;
					callbacks.onHide();
				}
			}

			destroy() {
				this.hide();
			}
		}
	);

	return [dismissedSelection, plugin];
}
