/**
 * Mouse-freeze guard
 *
 * While a pointer is down in the editor, decoration rebuilds that react to
 * the selection are held. Without this, a press that moves the caret reveals
 * a construct's delimiters, the text shifts under the still-pressed pointer,
 * and the drag that follows selects something other than what the user aimed
 * at — the "pointer is off" class of bug. (atomic-editor ships the same guard
 * under the same name for the same reason.)
 *
 * The guard is built on POINTER events, not mouse events. iOS synthesizes
 * `mousedown`/`mouseup` only after `touchend`, back to back, so a mouse-event
 * freeze engaged and released after the touch was already over and never
 * covered the touch itself. `pointerdown` fires when the finger lands.
 *
 * The field flips true on a primary, left-button `pointerdown` whose target is
 * inside `contentDOM`, listened for in the CAPTURE phase on `view.dom` so it
 * runs before CodeMirror's own press handling can move the selection and
 * trigger a rebuild. It flips back on the next window `pointerup` — window,
 * not editor, because drags routinely end outside the element — or, after a
 * short tail, on `pointercancel`, which is what a touch turning into a scroll
 * produces; the tail keeps the release rebuild from landing in the first
 * frames of that scroll. Consumers rebuild when it flips back, so a plain
 * click reveals on release, a beat the eye reads as instant.
 *
 * What is deliberately NOT frozen:
 *   - a press outside `contentDOM` (the scrollbar on `.cm-scroller`, gutters,
 *     panels) — nothing there moves the caret, and freezing for a scrollbar
 *     drag would hold decorations stale until release;
 *   - non-left buttons — CodeMirror only starts a drag selection for button 0,
 *     and a native context menu can swallow the matching `pointerup`;
 *   - non-primary pointers — the second finger of a pinch.
 * A press on a widget inside `contentDOM` (a checkbox, a copy button, a file
 * card) DOES freeze: it still moves CodeMirror's caret to the widget's edge,
 * which is exactly the shift-under-pointer this guards against, and a freeze
 * that turns out unnecessary costs one idempotent rebuild on release.
 */

import { StateEffect, StateField, type EditorState, type Extension } from '@codemirror/state';
import { EditorView, ViewPlugin } from '@codemirror/view';

/**
 * How long after `pointercancel` the freeze holds. A cancel means the touch
 * became a scroll; releasing on the spot would put the rebuild under the
 * first frames of momentum. Same value as atomic-editor's FREEZE_TAIL_MS.
 */
const CANCEL_TAIL_MS = 100;

const setMouseSelecting = StateEffect.define<boolean>();

/**
 * Whether a pointer interaction is in flight. Read with
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

const freezePlugin = ViewPlugin.fromClass(
	class {
		private down = false;
		private tail: ReturnType<typeof setTimeout> | null = null;

		constructor(private readonly view: EditorView) {
			// Capture on view.dom: the target filter below needs the whole editor
			// surface, and capture puts the freeze ahead of CodeMirror's own
			// press handlers, which listen on contentDOM in the bubble phase.
			view.dom.addEventListener('pointerdown', this.onDown, true);
			// Capture on window too: a widget that stops propagation on its own
			// pointerup (as several do on `click`) must not strand the freeze.
			window.addEventListener('pointerup', this.onUp, true);
			window.addEventListener('pointercancel', this.onCancel, true);
		}

		private readonly onDown = (event: PointerEvent) => {
			if (event.button !== 0 || !event.isPrimary) return;
			const target = event.target;
			if (!(target instanceof Node) || !this.view.contentDOM.contains(target)) return;
			this.down = true;
			this.clearTail();
			if (!isMouseSelecting(this.view.state)) {
				this.view.dispatch({ effects: setMouseSelecting.of(true) });
			}
			// Never claim the event — CodeMirror's own selection handling runs.
		};

		private readonly onUp = () => {
			if (!this.down) return;
			this.down = false;
			this.clearTail();
			this.release();
		};

		private readonly onCancel = () => {
			if (!this.down) return;
			this.down = false;
			this.clearTail();
			this.tail = setTimeout(() => {
				this.tail = null;
				this.release();
			}, CANCEL_TAIL_MS);
		};

		private release() {
			// The view may have been destroyed mid-drag (tab close, navigation).
			// `destroy` below removes the listeners and the tail, but a release
			// already queued behind the teardown still has to bail.
			if (!this.view.dom.isConnected) return;
			if (isMouseSelecting(this.view.state)) {
				this.view.dispatch({ effects: setMouseSelecting.of(false) });
			}
		}

		private clearTail() {
			if (this.tail !== null) {
				clearTimeout(this.tail);
				this.tail = null;
			}
		}

		destroy() {
			this.view.dom.removeEventListener('pointerdown', this.onDown, true);
			window.removeEventListener('pointerup', this.onUp, true);
			window.removeEventListener('pointercancel', this.onCancel, true);
			this.clearTail();
		}
	},
);

/** Is a pointer interaction in flight in this state? */
export function isMouseSelecting(state: EditorState): boolean {
	return state.field(mouseSelecting, false) ?? false;
}

/** True on the update/transaction where a pointer interaction ended. */
export function dragJustEnded(update: { state: EditorState; startState: EditorState }): boolean {
	return isMouseSelecting(update.startState) && !isMouseSelecting(update.state);
}

export const mouseFreeze: Extension = [mouseSelecting, freezePlugin];
