/**
 * The context gesture inside the editor: right-click, or a touch or pen held
 * for 450 ms with less than 8 px of drift, since iOS fires no `contextmenu`
 * for text. The same thresholds as CodeMirror's `long-press.ts`.
 *
 * Delegated on the editor, so text the editor re-renders keeps it. `pick`
 * says what is under the gesture; when it finds nothing the gesture is left
 * alone, so plain text keeps the platform's own selection menu.
 *
 * From the keyboard, Shift-F10 or the menu key opens the same menu: for a
 * selected widget its own (the menu its node view opens on the gesture),
 * otherwise whatever is at the caret (a link, a table cell, a proposal). A
 * Mac has no key that fires `contextmenu`, so without this every one of
 * those menus is the pointer's alone. A page that is read only has no caret
 * and runs no key bindings, but its links and widgets take the focus: there,
 * and from a widget's own controls, the key opens the menu of the one focused.
 *
 * A menu opened from the keyboard starts on its first row
 * (`contextMenu.reachFirstRow`), so a screen reader says it opened and
 * Enter chooses at once.
 *
 * After a hold opens a menu, the lift's click and the platform's own
 * `contextmenu` are swallowed (`swallowLift`), or either would close it.
 */

import { Extension } from '@tiptap/core';
import { NodeSelection, Plugin } from '@tiptap/pm/state';
import type { EditorView } from '@tiptap/pm/view';
import { swallowLift } from '$lib/components/contextMenu/lift';
import { contextMenu } from '$lib/stores/contextMenu.svelte';

const HOLD_MS = 450;
const DRIFT_PX = 8;

/** Opens a menu for what is at `pos`; returns false when there is nothing to offer there. */
export type ContextHandler = (
	view: EditorView,
	at: { x: number; y: number; pos: number; target: HTMLElement },
) => boolean;

function posAt(view: EditorView, x: number, y: number): number | null {
	return view.posAtCoords({ left: x, top: y })?.pos ?? null;
}

/** A box to open a menu at: the node's or the caret's, or the editor's own when there is no layout to ask. */
function boxOf(view: EditorView, pos: number, el: Element | null): { x: number; y: number } {
	try {
		const c = view.coordsAtPos(pos);
		if (c.bottom > 0 || c.left > 0) return { x: c.left, y: c.bottom };
	} catch {
		// No layout to measure the caret in.
	}
	const box = (el ?? view.dom).getBoundingClientRect();
	return { x: box.left, y: box.bottom };
}

/** Whether a menu opened from the keyboard; when it did, it starts on its first row. */
function fromKeyboard(opened: boolean): boolean {
	if (opened) contextMenu.reachFirstRow();
	return opened;
}

/**
 * Open, from the keyboard, the menu the context gesture opens: a selected
 * widget's own, or that of what is at the caret. Returns whether one opened.
 */
export function openContextFromKeyboard(view: EditorView, handlers: ContextHandler[]): boolean {
	const sel = view.state.selection;
	if (sel instanceof NodeSelection) {
		const dom = view.nodeDOM(sel.from);
		if (!(dom instanceof HTMLElement)) return false;
		const owner = dom.matches('[data-context-gesture]') ? dom : dom.querySelector('[data-context-gesture]');
		return fromKeyboard(owner instanceof HTMLElement && openWidgetMenu(owner));
	}
	const pos = sel.head;
	const { node } = view.domAtPos(pos);
	const target = node instanceof HTMLElement ? node : node.parentElement;
	if (!target) return false;
	const at = boxOf(view, pos, target);
	return fromKeyboard(handlers.some((h) => h(view, { ...at, pos, target })));
}

/** Open a widget's own menu, as a right-click on it does; returns whether it opened. */
function openWidgetMenu(owner: HTMLElement): boolean {
	const box = owner.getBoundingClientRect();
	const evt = new MouseEvent('contextmenu', {
		bubbles: true,
		cancelable: true,
		clientX: box.left + box.width / 2,
		clientY: box.top + box.height / 2,
	});
	return !owner.dispatchEvent(evt);
}

/** Whether a key opens a context menu: Shift-F10, or the menu key. */
function opensMenu(e: KeyboardEvent): boolean {
	return (e.key === 'F10' && e.shiftKey && !e.altKey && !e.ctrlKey && !e.metaKey) || e.key === 'ContextMenu';
}

/** The menu of the link or widget the focus is on; returns whether one opened. */
function openFocusedContext(view: EditorView, focused: EventTarget | null, handlers: ContextHandler[]): boolean {
	if (!(focused instanceof HTMLElement) || focused === view.dom || !view.dom.contains(focused)) return false;
	const owner = focused.closest('[data-context-gesture]');
	if (owner instanceof HTMLElement && view.dom.contains(owner)) return fromKeyboard(openWidgetMenu(owner));
	const pos = view.posAtDOM(focused, 0);
	const box = focused.getBoundingClientRect();
	return fromKeyboard(handlers.some((h) => h(view, { x: box.left, y: box.bottom, pos, target: focused })));
}

export function contextGesture(handlers: ContextHandler[]): Extension {
	const offer = (view: EditorView, x: number, y: number, target: EventTarget | null): boolean => {
		if (!(target instanceof HTMLElement) || !view.dom.contains(target)) return false;
		// A widget's own controls (node views) handle their own gestures. A
		// page that is read only is `contenteditable="false"` at its root,
		// which is no widget: its links still have their menu.
		const island = target.closest('[contenteditable="false"]');
		if (island && island !== view.dom) return false;
		const pos = posAt(view, x, y);
		if (pos === null) return false;
		return handlers.some((h) => h(view, { x, y, pos, target }));
	};

	return Extension.create({
		name: 'pageContextGesture',
		addKeyboardShortcuts() {
			const open = () => openContextFromKeyboard(this.editor.view, handlers);
			return { 'Shift-F10': open, ContextMenu: open };
		},
		addProseMirrorPlugins() {
			return [
				new Plugin({
					props: {
						handleDOMEvents: {
							contextmenu(view, event) {
								if (!offer(view, event.clientX, event.clientY, event.target)) return false;
								event.preventDefault();
								return true;
							},
						},
					},
					view(view) {
						let timer: ReturnType<typeof setTimeout> | null = null;
						let start = { x: 0, y: 0 };
						const cancel = () => {
							if (timer !== null) clearTimeout(timer);
							timer = null;
						};
						const down = (e: PointerEvent) => {
							if (e.pointerType === 'mouse') return;
							start = { x: e.clientX, y: e.clientY };
							const target = e.target;
							cancel();
							timer = setTimeout(() => {
								timer = null;
								if (offer(view, start.x, start.y, target)) swallowLift();
							}, HOLD_MS);
						};
						const move = (e: PointerEvent) => {
							if (timer !== null && Math.hypot(e.clientX - start.x, e.clientY - start.y) > DRIFT_PX) cancel();
						};
						// The key from a link or a widget the focus is on, rather
						// than the editor itself: on a page that is read only, and
						// from a widget's own controls, whose keys ProseMirror
						// leaves alone (a node view's `stopEvent`), so no key
						// binding sees them.
						const key = (e: KeyboardEvent) => {
							if (e.target === view.dom || !opensMenu(e)) return;
							if (!openFocusedContext(view, e.target, handlers)) return;
							e.preventDefault();
							e.stopPropagation();
						};
						view.dom.addEventListener('pointerdown', down);
						view.dom.addEventListener('pointermove', move);
						view.dom.addEventListener('pointerup', cancel);
						view.dom.addEventListener('pointercancel', cancel);
						view.dom.addEventListener('keydown', key);
						return {
							destroy() {
								cancel();
								view.dom.removeEventListener('keydown', key);
								view.dom.removeEventListener('pointerdown', down);
								view.dom.removeEventListener('pointermove', move);
								view.dom.removeEventListener('pointerup', cancel);
								view.dom.removeEventListener('pointercancel', cancel);
							},
						};
					},
				}),
			];
		},
	});
}
