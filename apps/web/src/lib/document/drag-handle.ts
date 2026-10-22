/**
 * The grip beside a block, as Linear docs draws one. With a mouse or a pen
 * over a block, or over the gutter left of it, a grip shows to its left, on
 * its first line. Drag it and the block moves where the drop cursor shows
 * (`moveUnitTo`: out and back in, keeping its id); hold the copy modifier
 * (⌥ on a Mac) as it drops and a copy goes in, with ids of its own. Click it
 * and the block is selected, so ⌥↑ and ⌥↓, ⌘C and Backspace act on it. A
 * list item has a grip of its own: the innermost one under the pointer.
 *
 * Typing hides it until the pointer moves. A page that is read only, and a
 * screen with no hover (a phone), draw none; the keys move a block there.
 *
 * The pointer is read from the whole document, not the editor, because the
 * grip sits left of the editable element and the pointer crosses open
 * margin to reach it. Over the grip, the block it was shown for stays.
 *
 * The grip sits beside the editable element, not in it, so ProseMirror sees
 * none of its events. The drag starts here and sets `view.dragging` as
 * ProseMirror does for its own drags, so the drop cursor draws where the
 * block would go and a drop on another page moves it as any drag does
 * (`DragBetweenPages` in editor.ts); a drop on this page is taken here.
 */

import { Extension } from '@tiptap/core';
import type { Node as PmNode, Slice } from '@tiptap/pm/model';
import { NodeSelection, Plugin, PluginKey, Selection, type Transaction } from '@tiptap/pm/state';
import { dropPoint } from '@tiptap/pm/transform';
import type { EditorView } from '@tiptap/pm/view';
import { moveUnitTo, unitAround, withoutIds } from './commands';

/** What a drag from the grip carries, mapped through edits made while it lasts. */
interface Held {
	from: number;
	to: number;
	slice: Slice;
}

interface Unit {
	from: number;
	to: number;
	node: PmNode;
}

export interface DragHandleOptions {
	/** Told when a drag from the grip starts and ends, as a drag from the page's text tells `DragBetweenPages`. */
	dragStarted: ((view: EditorView) => void) | null;
	dragEnded: ((view: EditorView) => void) | null;
}

const heldKey = new PluginKey<Held | null>('dragHandle');

const GRIP_WIDTH = 18;
const GRIP_HEIGHT = 24;
/** Between the grip and the block (or a list's markers). */
const GRIP_GAP = 6;
/** How far left of the text the pointer still counts as over a block: past the grip, with room to spare. */
const GUTTER = 48;

const GRIP_SVG =
	'<svg width="10" height="16" viewBox="0 0 10 16" fill="currentColor" aria-hidden="true">' +
	'<circle cx="2.5" cy="3" r="1.4"/><circle cx="7.5" cy="3" r="1.4"/>' +
	'<circle cx="2.5" cy="8" r="1.4"/><circle cx="7.5" cy="8" r="1.4"/>' +
	'<circle cx="2.5" cy="13" r="1.4"/><circle cx="7.5" cy="13" r="1.4"/></svg>';

/** The first and last places for the caret inside `unit`, or null when it has no text. */
function textRange(view: EditorView, unit: Unit): { from: number; to: number } | null {
	const { doc } = view.state;
	const first = Selection.findFrom(doc.resolve(unit.from + 1), 1, true);
	const last = Selection.findFrom(doc.resolve(unit.to - 1), -1, true);
	if (!first || !last || first.from >= unit.to || last.to <= unit.from || first.from > last.to) return null;
	return { from: first.from, to: last.to };
}

class Grip {
	readonly dom: HTMLDivElement;
	private unit: Unit | null = null;
	private pointer: { x: number; y: number } | null = null;
	private frame = 0;

	constructor(
		private view: EditorView,
		private options: DragHandleOptions,
	) {
		this.dom = document.createElement('div');
		this.dom.className = 'doc-grip';
		this.dom.draggable = true;
		this.dom.setAttribute('aria-hidden', 'true');
		this.dom.title = 'Drag to move. Click to select';
		this.dom.innerHTML = GRIP_SVG;
		this.dom.addEventListener('dragstart', this.onDragStart);
		this.dom.addEventListener('dragend', this.onDragEnd);
		this.dom.addEventListener('click', this.onClick);
		this.host()?.append(this.dom);
		document.addEventListener('mousemove', this.onMove, { capture: true, passive: true });
		document.addEventListener('mouseout', this.onOut, { passive: true });
		document.addEventListener('scroll', this.onScroll, { capture: true, passive: true });
	}

	private host(): HTMLElement | null {
		return this.view.dom.parentElement;
	}

	update(view: EditorView, prev: { doc: PmNode }): void {
		this.view = view;
		if (!view.editable) this.hide();
		else if (this.unit && !view.state.doc.eq(prev.doc)) this.schedule();
	}

	destroy(): void {
		cancelAnimationFrame(this.frame);
		document.removeEventListener('mousemove', this.onMove, { capture: true });
		document.removeEventListener('mouseout', this.onOut);
		document.removeEventListener('scroll', this.onScroll, { capture: true });
		this.dom.remove();
	}

	/** Typed keys hide it until the pointer moves. */
	hide(): void {
		this.unit = null;
		this.dom.classList.remove('is-shown');
	}

	private onMove = (event: MouseEvent): void => {
		// Over the grip, it stays beside the block it was shown for.
		if (event.target instanceof Node && this.dom.contains(event.target)) return;
		this.pointer = { x: event.clientX, y: event.clientY };
		if (this.unit || this.near(this.pointer)) this.schedule();
	};

	/** The pointer left the window. */
	private onOut = (event: MouseEvent): void => {
		if (event.relatedTarget) return;
		this.pointer = null;
		this.hide();
	};

	/** Whether `pointer` is over the text or the gutter left of it. */
	private near(pointer: { x: number; y: number }): boolean {
		const content = this.view.dom.getBoundingClientRect();
		return (
			pointer.y >= content.top &&
			pointer.y <= content.bottom &&
			pointer.x >= content.left - GUTTER &&
			pointer.x <= content.right
		);
	}

	private onScroll = (): void => {
		if (this.unit) this.schedule();
	};

	private schedule(): void {
		cancelAnimationFrame(this.frame);
		this.frame = requestAnimationFrame(() => this.place());
	}

	/**
	 * Whether the pointer is on its way from the block the grip is shown for
	 * to the grip: level with the block and left of it. Read from the page
	 * there, the row is a list item's list, and the grip would jump to it.
	 */
	private heading(pointer: { x: number; y: number }): boolean {
		const { unit, view } = this;
		if (!unit || view.state.doc.nodeAt(unit.from) !== unit.node) return false;
		const block = view.nodeDOM(unit.from);
		if (!(block instanceof HTMLElement)) return false;
		const box = block.getBoundingClientRect();
		return pointer.y >= box.top && pointer.y <= box.bottom && pointer.x < box.left;
	}

	/** Put the grip beside the block under the pointer, or hide it. */
	private place(): void {
		const { view, pointer } = this;
		if (!pointer || !view.editable || view.isDestroyed || !this.near(pointer)) return this.hide();
		if (this.heading(pointer)) return;
		const content = view.dom.getBoundingClientRect();
		// Over the gutter, read the row as if the pointer were at the text's edge.
		const x = Math.min(Math.max(pointer.x, content.left + 1), content.right - 1);
		const at = view.posAtCoords({ left: x, top: pointer.y });
		if (!at) return this.hide();
		const unit = unitAround(view.state.doc, at.inside >= 0 ? at.inside : at.pos);
		const block = unit && view.nodeDOM(unit.from);
		const host = this.host();
		if (!unit || !(block instanceof HTMLElement) || !host) return this.hide();

		const hostBox = host.getBoundingClientRect();
		const box = block.getBoundingClientRect();
		const text = unit.node.isAtom ? null : textRange(view, unit);
		let top = box.top;
		if (text) {
			const line = view.coordsAtPos(text.from);
			top = (line.top + line.bottom) / 2 - GRIP_HEIGHT / 2;
		}
		// A list item's marker sits in its list's padding: the grip goes left of it.
		const list = unit.node.type.name === 'listItem' || unit.node.type.name === 'taskItem' ? block.parentElement : null;
		const left = (list ?? block).getBoundingClientRect().left - GRIP_WIDTH - GRIP_GAP;
		this.unit = unit;
		this.dom.style.transform = `translate(${Math.round(left - hostBox.left)}px, ${Math.round(top - hostBox.top)}px)`;
		this.dom.classList.add('is-shown');
	}

	private onClick = (event: MouseEvent): void => {
		const { unit, view } = this;
		if (!unit || !view.editable) return;
		event.preventDefault();
		const { state } = view;
		view.dispatch(state.tr.setSelection(NodeSelection.create(state.doc, unit.from)));
		view.focus();
	};

	private onDragStart = (event: DragEvent): void => {
		const { unit, view } = this;
		if (!unit || !event.dataTransfer || !view.editable) {
			event.preventDefault();
			return;
		}
		const { state } = view;
		const selection = NodeSelection.create(state.doc, unit.from);
		const slice = selection.content();
		view.dispatch(state.tr.setSelection(selection).setMeta(heldKey, { from: unit.from, to: unit.to, slice }));
		const { dom, text } = view.serializeForClipboard(slice);
		event.dataTransfer.clearData();
		event.dataTransfer.setData('text/html', dom.innerHTML);
		event.dataTransfer.setData('text/plain', text);
		event.dataTransfer.effectAllowed = 'copyMove';
		const block = view.nodeDOM(unit.from);
		if (block instanceof HTMLElement) {
			event.dataTransfer.setDragImage(block, 0, Math.max(0, event.clientY - block.getBoundingClientRect().top));
		}
		// As ProseMirror's own drag sets it, `node` included: the drop cursor
		// reads it, and so does a drop on another page.
		view.dragging = { slice, move: true, node: selection } as EditorView['dragging'];
		this.options.dragStarted?.(view);
		// Faded, not hidden: Chrome ends a drag at once when its source stops
		// taking the pointer during dragstart.
		this.dom.classList.add('is-dragging');
	};

	private onDragEnd = (): void => {
		const { view } = this;
		this.dom.classList.remove('is-dragging');
		this.hide();
		this.options.dragEnded?.(view);
		// After the drop has read it, as ProseMirror clears its own.
		setTimeout(() => {
			if (view.isDestroyed) return;
			const held = heldKey.getState(view.state);
			if (held && view.dragging?.slice === held.slice) view.dragging = null;
			if (held) view.dispatch(view.state.tr.setMeta(heldKey, null));
		}, 50);
	};
}

export const DragHandle = Extension.create<DragHandleOptions>({
	name: 'dragHandle',
	// Ahead of the page's other drop handlers (a paste of what was dropped).
	priority: 200,
	addOptions() {
		return { dragStarted: null, dragEnded: null };
	},
	addProseMirrorPlugins() {
		const options = this.options;
		let grip: Grip | null = null;
		return [
			new Plugin<Held | null>({
				key: heldKey,
				state: {
					init: () => null,
					apply(tr: Transaction, held: Held | null): Held | null {
						const meta = tr.getMeta(heldKey) as Held | null | undefined;
						if (meta !== undefined) return meta;
						if (!held || !tr.docChanged) return held;
						const from = tr.mapping.mapResult(held.from, 1);
						const to = tr.mapping.mapResult(held.to, -1);
						return from.deleted || to.deleted || to.pos <= from.pos ? null : { ...held, from: from.pos, to: to.pos };
					},
				},
				view(view) {
					grip = new Grip(view, options);
					return grip;
				},
				props: {
					handleKeyDown() {
						grip?.hide();
						return false;
					},
					// The slice given here has been through `transformPasted`, so the
					// drag is known by the one `view.dragging` still holds.
					handleDrop(view, event, _slice, moved) {
						const held = heldKey.getState(view.state);
						if (!held || view.dragging?.slice !== held.slice) return false;
						const at = view.posAtCoords({ left: event.clientX, top: event.clientY });
						const node = view.state.doc.nodeAt(held.from);
						if (!at || !node) return true;
						const target = dropPoint(view.state.doc, at.pos, held.slice) ?? at.pos;
						if (moved) moveUnitTo(view, held.from, held.to, target);
						else view.dispatch(view.state.tr.replaceRangeWith(target, target, withoutIds(node)).scrollIntoView());
						return true;
					},
				},
			}),
		];
	},
});
