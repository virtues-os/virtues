/**
 * When and where the selection toolbar shows over a block page, with the
 * manners of the CodeMirror editor's (`codemirror/extensions/selection-toolbar.ts`):
 *
 * - Never during a drag. The selection changes under a sweeping pointer,
 *   and a bar over the words being swept is in the way; it shows once the
 *   pointer is released, wherever that happens (a drag often ends outside
 *   the editor).
 * - A selection made with the keyboard waits until it has held still for
 *   200 ms, so Shift and the arrows are not chased by a bar moving every step.
 * - On touch it sits under the selection, where the platform's own menu is
 *   not, unless the software keyboard leaves no room there: then above.
 */

import type { Transaction } from '@tiptap/pm/state';
import type { EditorView } from '@tiptap/pm/view';

/** How long a keyboard selection holds still before the toolbar shows. */
export const KEYBOARD_SHOW_DELAY_MS = 200;
/**
 * The bar's height on touch: its 44pt buttons, its 3px padding and 1px
 * border, and its frame's 1px border (`SelectionToolbar.svelte`).
 */
export const TOUCH_TOOLBAR_HEIGHT = 44 + 2 * 3 + 2 * 1 + 2 * 1;
/** The gap the bar keeps from its anchor. */
export const TOOLBAR_OFFSET = 8;
/** Room under a selection's last line for the end handle a phone hangs there. */
export const SELECTION_HANDLE_ROOM = 24;
/**
 * How far under a selection the bar's anchor goes on touch. The bar sits
 * above its anchor, so its top clears the selection's last line and the
 * handle under it.
 */
export const TOOLBAR_CLEARANCE = SELECTION_HANDLE_ROOM + TOUCH_TOOLBAR_HEIGHT + TOOLBAR_OFFSET;

export interface ToolbarTiming {
	/** Whether the toolbar may show now. */
	ready(): boolean;
	/** Note a transaction the editor applied; a selection the keyboard moved starts the wait. */
	note(tr: Transaction): void;
	destroy(): void;
}

/**
 * Track the pointer and the keyboard for the toolbar. `show` is called when
 * the toolbar may show again: on the release that ends a drag, and when a
 * keyboard selection has held still.
 */
export function toolbarTiming(view: EditorView, show: () => void, delay = KEYBOARD_SHOW_DELAY_MS): ToolbarTiming {
	let pressed = false;
	let waiting = false;
	let timer: ReturnType<typeof setTimeout> | null = null;
	const stopWaiting = () => {
		if (timer !== null) clearTimeout(timer);
		timer = null;
		waiting = false;
	};
	const down = (e: PointerEvent) => {
		if (e.button !== 0) return;
		pressed = true;
		stopWaiting();
	};
	const up = () => {
		if (!pressed) return;
		pressed = false;
		show();
	};
	view.dom.addEventListener('pointerdown', down);
	window.addEventListener('pointerup', up, true);
	window.addEventListener('pointercancel', up, true);
	return {
		ready: () => !pressed && !waiting,
		note(tr) {
			// A pointer's selection carries `pointer`; while it is down the
			// release decides, and a click's selection is final at once.
			if (!tr.selectionSet || tr.getMeta('pointer') || pressed) return;
			stopWaiting();
			waiting = true;
			timer = setTimeout(() => {
				timer = null;
				waiting = false;
				show();
			}, delay);
		},
		destroy() {
			stopWaiting();
			view.dom.removeEventListener('pointerdown', down);
			window.removeEventListener('pointerup', up, true);
			window.removeEventListener('pointercancel', up, true);
		},
	};
}

/**
 * The bottom of the page the toolbar can use: the window's, less what the
 * software keyboard covers. WKWebView paints the keyboard over the page
 * without resizing it, and `stores/keyboard.svelte.ts` says how much it
 * covers as `--keyboard-inset`.
 */
export function visibleBottom(): number {
	if (typeof window === 'undefined') return Infinity;
	const inset = parseFloat(getComputedStyle(document.documentElement).getPropertyValue('--keyboard-inset')) || 0;
	return window.innerHeight - inset;
}

/**
 * Where the toolbar's anchor goes for a selection from `start` to `end`
 * (their caret boxes): above the selection, or on touch under it when the
 * bar fits above `bottom`. The bar sits above its anchor.
 */
export function selectionToolbarAt(
	start: { left: number; top: number },
	end: { left: number; bottom: number },
	o: { coarse: boolean; bottom: number },
): { x: number; y: number } {
	const x = (start.left + end.left) / 2;
	const under = end.bottom + TOOLBAR_CLEARANCE;
	return o.coarse && under <= o.bottom ? { x, y: under } : { x, y: start.top };
}
