/**
 * Mouse-freeze guard
 *
 * While the mouse button is down in the editor, decoration rebuilds that react
 * to the selection are held. Without this, a mousedown that moves the caret
 * reveals a construct's delimiters, the text shifts under the still-pressed
 * pointer, and the drag that follows selects something other than what the
 * user aimed at — the "pointer is off" class of bug. (atomic-editor ships the
 * same guard under the same name for the same reason.)
 *
 * The field flips true on mousedown inside the editor and false on the next
 * window mouseup — window, not editor, because drags routinely end outside the
 * element. Consumers rebuild when it flips back, so a plain click reveals on
 * release, a beat the eye reads as instant.
 */

import { StateEffect, StateField, type EditorState, type Extension } from '@codemirror/state';
import { EditorView } from '@codemirror/view';

const setMouseSelecting = StateEffect.define<boolean>();

/**
 * Whether a mouse interaction is in flight. Read with
 * `state.field(mouseSelecting, false) ?? false` so consumers stay safe when
 * the rendered surface (and this field with it) is compartment-swapped out.
 */
export const mouseSelecting = StateField.define<boolean>({
	create: () => false,
	update(value, tr) {
		for (const e of tr.effects) {
			if (e.is(setMouseSelecting)) value = e.value;
		}
		return value;
	},
});

const handlers = EditorView.domEventHandlers({
	mousedown(_event, view) {
		view.dispatch({ effects: setMouseSelecting.of(true) });
		const release = () => {
			window.removeEventListener('mouseup', release);
			// The view may have been destroyed mid-drag (tab close, navigation).
			if (view.dom.isConnected) {
				view.dispatch({ effects: setMouseSelecting.of(false) });
			}
		};
		window.addEventListener('mouseup', release);
		// Never claim the event — CodeMirror's own selection handling runs.
		return false;
	},
});

/** Is a mouse interaction in flight in this state? */
export function isMouseSelecting(state: EditorState): boolean {
	return state.field(mouseSelecting, false) ?? false;
}

/** True on the update/transaction where a mouse interaction ended. */
export function dragJustEnded(update: { state: EditorState; startState: EditorState }): boolean {
	return isMouseSelecting(update.startState) && !isMouseSelecting(update.state);
}

export const mouseFreeze: Extension = [mouseSelecting, handlers];
