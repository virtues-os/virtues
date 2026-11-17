/**
 * The grip beside a block, as Linear docs draws one. With a mouse or a pen
 * over a block, or over the gutter left of it, a grip shows to its left, on
 * its first line. Drag it and the block moves where the drop cursor shows
 * (`moveUnitTo`: out and back in, keeping its id); hold the copy modifier
 * (⌥ on a Mac) as it drops and a copy goes in, with ids of its own, and is
 * selected. Click it and the block is selected, so ⌥↑ and ⌥↓, ⌘C and
 * Backspace act on it. A list item has a grip of its own: the innermost one
 * under the pointer.
 *
 * With Flat headings on (`prose--flat-headings` around the editor), a
 * heading's grip is its level mark: the `#`s the margin already shows, in
 * the same place, darkening under the pointer. It drags as any grip does,
 * and a click opens `headingMenu` to change the level instead of selecting.
 *
 * Typing hides it until the pointer moves. A page that is read only, and a
 * screen with no hover (a phone), draw none; the keys move a block there.
 *
 * The pointer is read from the whole document, not the editor, because the
 * grip sits left of the editable element and the pointer crosses open
 * margin to reach it. Over the grip, the block it was shown for stays.
 * Mouse events stop during a drag, so after one the grip waits for the
 * pointer to move before it shows again.
 *
 * With the pointer over the grip, what a drag would move is tinted
 * (`doc-drag-aim`), and pressing the grip deepens the tint, so you know
 * what you have before it moves. While a block is dragged, a chip
 * (`doc-drag-chip`) rides below and right of the pointer: the block's kind
 * and first words, small enough to leave the drop line and the text around
 * it in sight. The grip stays,
 * pressed, beside the block's old place. The block stays where it was,
 * dimmed (`doc-drag-source`), the drop cursor draws a line where it would
 * land, and no line where a drop would do nothing (into itself). A drag
 * that is cancelled sends the chip back to the grip; one that lands fades
 * it where it landed. Near the
 * top or bottom of the page's scroller the page scrolls, faster nearer the
 * edge. On the drop the blocks that moved slide from where they were
 * (`slideMoved`), unless the person asks for reduced motion, and where it
 * landed is said to assistive technology. The editor has the focus after
 * every drag, dropped or not, so the keys act on the block that landed; a
 * drag from the grip let go of nowhere, or over the block itself, leaves
 * the block where it was, selected.
 *
 * The margin beside the text takes the drop as the text at the same height
 * would: the grip sits in it, and the pointer often stays there on its way
 * down the page. A dragover there is passed to the editor at the text's
 * edge, so the drop cursor draws the line it would draw there, and a drop
 * there goes through the same `land` as a drop on the text.
 *
 * A widget dragged by itself (an image, a figure, an applet) is ProseMirror's
 * own drag, and it goes as a drag from its grip goes: nothing under the
 * pointer, its place dimmed, the drop taken here. The selection stays as it
 * was, as ProseMirror leaves it, until the widget lands and is selected.
 *
 * The grip sits beside the editable element, not in it, so ProseMirror sees
 * none of its events. The drag starts here and sets `view.dragging` as
 * ProseMirror does for its own drags, so the drop cursor draws where the
 * block would go and a drop on another page moves it as any drag does
 * (`DragBetweenPages` in editor.ts); a drop on this page is taken here.
 */

import { Extension } from '@tiptap/core';
import type { Node as PmNode, NodeType, Slice } from '@tiptap/pm/model';
import { NodeSelection, Plugin, PluginKey, Selection, type EditorState, type Transaction } from '@tiptap/pm/state';
import { dropPoint } from '@tiptap/pm/transform';
import { Decoration, DecorationSet, type EditorView } from '@tiptap/pm/view';
import { moveUnitTo, unitAround, withoutIds } from './commands';
import { motionMs } from '../utils/motion';
import { clearDropLine, dropMotion, glowLanded } from './drop-line';
import { scrollerOf } from './outline';
import { contract } from './schema';

/** What a drag of a block carries, mapped through edits made while it lasts. */
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

/** A point on the screen, as a drag event gives one. */
interface At {
	clientX: number;
	clientY: number;
}

/** A drag of a block of this page, from its grip or of a widget itself. */
interface Drag {
	/** The element the drag began on, which is told when it ends. */
	source: EventTarget;
	/** The page's scroller: what the drag scrolls, and whose margin takes the drop. */
	scroller: HTMLElement;
	/** Over, though a drop that arrives after the end is still taken. */
	ended: boolean;
}

/** Where blocks were before a drop: the top of each with an id near the screen, and how far the page had scrolled. */
interface Before {
	tops: Map<string, number>;
	scroller: HTMLElement;
	scrollTop: number;
}

export interface DragHandleOptions {
	/** Told when a drag from the grip starts and ends, as a drag from the page's text tells `DragBetweenPages`. */
	dragStarted: ((view: EditorView) => void) | null;
	dragEnded: ((view: EditorView) => void) | null;
	/** Opened by a click on a heading's level mark, under it: the menu that changes the heading at `pos`. */
	headingMenu: ((view: EditorView, pos: number, at: { x: number; y: number }) => void) | null;
}

const heldKey = new PluginKey<Held | null>('dragHandle');

/** What the grip under the pointer would move, tinted; `pressed` while the grip is held down. */
interface Aim {
	from: number;
	to: number;
	pressed: boolean;
}

const aimKey = new PluginKey<Aim | null>('dragHandleAim');

/** Where the chip sits from the pointer: below and right of it, clear of the drop line it aims. */
const CHIP_OFFSET_X = 14;
const CHIP_OFFSET_Y = 12;
/** How much of a block's text the chip shows; CSS cuts it to two lines. */
const CHIP_TEXT = 120;

/** The chip's word for a block with no text of its own to show. */
const WIDGET_WORDS: Record<string, string> = {
	image: 'Image',
	audio: 'Audio',
	video: 'Video',
	file: 'File',
	table: 'Table',
	applet: 'Applet',
	figure: 'Figure',
	horizontalRule: 'Divider',
};

/** What kind of block `node` is, in the page's own marks: #s, ¶, a bullet, a number, a box. */
function kindMark(node: PmNode, el: HTMLElement): string {
	switch (node.type.name) {
		case 'heading':
			return '#'.repeat(Number(node.attrs.level) || 1);
		case 'paragraph':
			return '¶';
		case 'blockquote':
			return '\u201C';
		case 'codeBlock':
			return '{ }';
		case 'taskItem':
			return node.attrs.checked ? '\u2611' : '\u2610';
		case 'listItem': {
			const list = el.parentElement;
			if (list instanceof HTMLOListElement) return `${list.start + [...list.children].indexOf(el)}.`;
			return '\u2022';
		}
		default:
			return '';
	}
}

/**
 * The chip a drag carries: the block in brief, as its kind and its first
 * words, or a widget's name and a picture of an image. The page shows the
 * whole block, dimmed in its old place, and the line shows where it lands.
 */
function chipOf(node: PmNode, el: HTMLElement): HTMLDivElement {
	const chip = document.createElement('div');
	chip.className = 'doc-drag-chip';
	chip.setAttribute('aria-hidden', 'true');
	// Shown while the copy modifier is held: the drop puts in a copy (`is-copy`).
	const plus = document.createElement('span');
	plus.className = 'doc-drag-chip-plus';
	plus.textContent = '+';
	chip.append(plus);
	const widget = WIDGET_WORDS[node.type.name];
	const src = node.type.name === 'image' ? String(node.attrs.src ?? '') : '';
	if (src) {
		const thumb = document.createElement('img');
		thumb.className = 'doc-drag-chip-thumb';
		thumb.src = el.querySelector('img')?.currentSrc || src;
		thumb.alt = '';
		chip.append(thumb);
	} else {
		const kind = kindMark(node, el);
		if (kind) {
			const mark = document.createElement('span');
			mark.className = 'doc-drag-chip-kind';
			mark.textContent = kind;
			chip.append(mark);
		}
	}
	// A space between the blocks a quote or a list item holds, so their words do not run together.
	const words = node.textBetween(0, node.content.size, ' ', ' ').trim().replace(/\s+/g, ' ');
	const named = String(node.attrs.name ?? node.attrs.alt ?? node.attrs.caption ?? '').trim();
	const text = document.createElement('span');
	text.className = 'doc-drag-chip-text';
	text.textContent = (widget ? [widget, named || words].filter(Boolean).join(' \u00B7 ') : words || 'Empty block').slice(0, CHIP_TEXT);
	chip.append(text);
	return chip;
}

const GRIP_WIDTH = 18;
const GRIP_HEIGHT = 24;
/** Between the grip and the block (or a list's markers): room enough that the grip reads as the margin's, not the line's. */
const GRIP_GAP = 12;
/** A level mark's padding inside its grip (`.doc-grip.is-mark`): the mark's text, not its box, sits GRIP_GAP from the heading. */
const MARK_PAD = 4;
/** How far left of the text the pointer still counts as over a block: past the grip, with room to spare. */
const GUTTER = 48;

const GRIP_SVG =
	'<svg width="10" height="16" viewBox="0 0 10 16" fill="currentColor" aria-hidden="true">' +
	'<circle cx="2.5" cy="3" r="1.4"/><circle cx="7.5" cy="3" r="1.4"/>' +
	'<circle cx="2.5" cy="8" r="1.4"/><circle cx="7.5" cy="8" r="1.4"/>' +
	'<circle cx="2.5" cy="13" r="1.4"/><circle cx="7.5" cy="13" r="1.4"/></svg>';

/** How far from the scroller's top or bottom edge a drag scrolls the page, and its fastest step a frame. */
const EDGE = 56;
const EDGE_STEP = 18;
/** How long after a drag's end its drop is still read: some browsers send the drop after the end. */
const SETTLE_MS = 50;
/** How long after a drop a drag is ended when its element, taken out of the page by the drop, hears no end. */
const DROP_END_MS = 100;

/** The `#`s for a heading's level, when Flat headings are on around `host`; null for any other block or mode. */
function levelMark(host: HTMLElement, node: PmNode, block: HTMLElement): string | null {
	if (node.type.name !== 'heading' || !host.closest('.prose--flat-headings')) return null;
	// Where the mark leads the line (a narrow margin, a heading in a quote),
	// the grip would show it twice: the dots stand in.
	if (getComputedStyle(block, '::before').position === 'static') return null;
	return '#'.repeat(Number(node.attrs.level) || 1);
}

/** Where a drop at `at` would put what `held` carries, or null where it would do nothing. */
function dropTarget(view: EditorView, at: At, held: Held, moved: boolean): number | null {
	const pos = view.posAtCoords({ left: at.clientX, top: at.clientY });
	if (!pos) return null;
	const target = dropPoint(view.state.doc, pos.pos, held.slice) ?? pos.pos;
	return moved && target >= held.from && target <= held.to ? null : target;
}

/** Whether a drop copies, as ProseMirror reads it: the copy modifier held (⌥ on a Mac, Ctrl elsewhere). */
function copies(event: { altKey: boolean; ctrlKey: boolean }): boolean {
	return /Mac|iP(hone|[oa]d)/.test(navigator.platform) ? event.altKey : event.ctrlKey;
}

function reducedMotion(): boolean {
	return window.matchMedia?.('(prefers-reduced-motion: reduce)').matches ?? false;
}

/** Where the blocks near the screen are now, by id, and how far the page has scrolled. */
function measure(view: EditorView): Before {
	const tops = new Map<string, number>();
	const reach = window.innerHeight * 2;
	view.state.doc.descendants((node, pos) => {
		if (!node.isBlock) return false;
		const id = node.attrs[contract.id.attr];
		const el = view.nodeDOM(pos);
		if (id && el instanceof HTMLElement) {
			const top = el.getBoundingClientRect().top;
			if (Math.abs(top) < reach) tops.set(String(id), top);
		}
		return !node.isTextblock;
	});
	const scroller = scrollerOf(view.dom);
	return { tops, scroller, scrollTop: scroller.scrollTop };
}

/**
 * Slide every block that moved from where it was (`before`) to where it is
 * now. A drop that scrolled the page to show where the block landed moved
 * every block on the screen by as much; that is taken off, so only the
 * blocks that moved in the page slide. A block that moved carries its own
 * blocks, which are not slid again.
 */
function slideMoved(view: EditorView, before: Before): void {
	const scrolled = before.scroller.scrollTop - before.scrollTop;
	const motion = dropMotion();
	view.state.doc.descendants((node, pos) => {
		if (!node.isBlock) return false;
		const id = node.attrs[contract.id.attr];
		const was = id ? before.tops.get(String(id)) : undefined;
		const el = view.nodeDOM(pos);
		if (was === undefined || !(el instanceof HTMLElement)) return !node.isTextblock;
		const dy = was - scrolled - el.getBoundingClientRect().top;
		if (Math.abs(dy) < 1) return !node.isTextblock;
		// The Web Animations API writes no attribute ProseMirror would read back.
		el.animate([{ transform: `translateY(${dy}px)` }, { transform: 'none' }], motion);
		return false;
	});
}

/** Where a node of `type` went in, put in by `tr`'s last step at about `near`: the first one at or after where it went. */
function insertedAt(tr: Transaction, near: number, type: NodeType): number | null {
	const map = tr.mapping.maps[tr.mapping.maps.length - 1];
	if (!map) return null;
	let from = near;
	let to = near;
	map.forEach((_from, _to, newFrom, newTo) => {
		from = newFrom;
		to = newTo;
	});
	let found: number | null = null;
	tr.doc.nodesBetween(from, to, (node, pos) => {
		if (found === null && pos >= from && node.type === type) found = pos;
		return found === null;
	});
	return found;
}

/** Where the selected block now sits, said in words: below the block before it, or at the top. */
function landedWords(state: EditorState): string {
	const { selection } = state;
	if (!(selection instanceof NodeSelection)) return 'Block moved';
	const before = state.doc.resolve(selection.from).nodeBefore;
	if (!before) return 'Block moved to the top';
	const text = before.textContent.trim();
	if (!text) return 'Block moved';
	return `Block moved below ${text.length > 40 ? `${text.slice(0, 40)}…` : text}`;
}

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
	/** What a drag shows under the pointer: nothing, one transparent pixel. */
	private readonly blank: HTMLDivElement;
	/** Where the person is told where a block landed. */
	private readonly said: HTMLDivElement;
	private drag: Drag | null = null;
	/** The level mark the grip shows in place of its dots, or null for the dots. */
	private mark: string | null = null;
	/** The pointer's height while a block is dragged, and the frame that scrolls the page by it. */
	private dragPointer: number | null = null;
	private scrollFrame = 0;
	private endTimer: ReturnType<typeof setTimeout> | undefined;
	private dropTimer: ReturnType<typeof setTimeout> | undefined;
	/** Whether the drop line is drawn from the margin, its dragover passed to the editor. */
	private relayed = false;
	/** While a dragover from the margin is passed on; it comes back through the document's own listener. */
	private relaying = false;
	/** `view.dragging` as a dragstart in the editor began. ProseMirror's own drag sets a new one. */
	private draggingBefore: unknown = null;
	/** The chip that rides beside the pointer while a block is dragged, and the pointer's left as the drag last said it. */
	private chip: HTMLDivElement | null = null;
	private dragPointerX = 0;
	/** Whether this drag's block was put down on this page, and whether anything in the window took a drop. */
	private landed = false;
	private dropped = false;

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
		this.dom.addEventListener('dragstart', this.onGripDragStart);
		this.dom.addEventListener('click', this.onClick);
		this.dom.addEventListener('mouseenter', this.onGripEnter);
		this.dom.addEventListener('mouseleave', this.onGripLeave);
		this.dom.addEventListener('mousedown', this.onGripDown);
		this.blank = document.createElement('div');
		this.blank.style.cssText = 'position:fixed;top:-2px;left:-2px;width:1px;height:1px;opacity:0;pointer-events:none';
		this.said = document.createElement('div');
		this.said.className = 'sr-only';
		this.said.setAttribute('aria-live', 'polite');
		this.host()?.append(this.dom, this.blank, this.said);
		// Around ProseMirror's own dragstart, which is registered first.
		view.dom.addEventListener('dragstart', this.beforeDragStart, true);
		view.dom.addEventListener('dragstart', this.onWidgetDragStart);
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
		else if (this.unit && !this.drag && !view.state.doc.eq(prev.doc)) this.schedule();
	}

	destroy(): void {
		cancelAnimationFrame(this.frame);
		clearTimeout(this.endTimer);
		clearTimeout(this.dropTimer);
		this.stopScrolling();
		document.removeEventListener('drop', this.onDrop, { capture: true });
		this.drag?.source.removeEventListener('dragend', this.onDragEnd);
		this.drag = null;
		this.view.dom.removeEventListener('dragstart', this.beforeDragStart, true);
		this.view.dom.removeEventListener('dragstart', this.onWidgetDragStart);
		this.blank.remove();
		this.said.remove();
		this.chip?.remove();
		document.removeEventListener('mouseup', this.onGripUp, { capture: true });
		document.removeEventListener('mousemove', this.onMove, { capture: true });
		document.removeEventListener('mouseout', this.onOut);
		document.removeEventListener('scroll', this.onScroll, { capture: true });
		this.dom.remove();
	}

	/** Typed keys hide it until the pointer moves. */
	hide(): void {
		cancelAnimationFrame(this.frame);
		this.unit = null;
		this.dom.classList.remove('is-shown');
		// Out of whatever update called this.
		if (aimKey.getState(this.view.state)) queueMicrotask(() => this.aim(null));
	}

	/** Tint what the grip would move, or take the tint away. */
	private aim(aim: Aim | null): void {
		const { view } = this;
		if (view.isDestroyed) return;
		const now = aimKey.getState(view.state);
		if (now === aim || (now && aim && now.from === aim.from && now.to === aim.to && now.pressed === aim.pressed)) return;
		view.dispatch(view.state.tr.setMeta(aimKey, aim));
	}

	private onGripEnter = (): void => {
		if (this.unit && !this.drag && this.view.editable) this.aim({ from: this.unit.from, to: this.unit.to, pressed: false });
	};

	private onGripLeave = (): void => {
		if (!this.drag) this.aim(null);
	};

	private onGripDown = (event: MouseEvent): void => {
		if (event.button !== 0 || !this.unit || !this.view.editable) return;
		this.aim({ from: this.unit.from, to: this.unit.to, pressed: true });
		document.addEventListener('mouseup', this.onGripUp, { capture: true });
	};

	/** Let go without a drag: a click, or a press that went nowhere. */
	private onGripUp = (event: MouseEvent): void => {
		document.removeEventListener('mouseup', this.onGripUp, { capture: true });
		const over = event.target instanceof Node && this.dom.contains(event.target);
		this.aim(over && this.unit ? { from: this.unit.from, to: this.unit.to, pressed: false } : null);
	};

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

	/** A scroll under a still pointer puts the grip beside the block now under it; a drag's scrolling does not. */
	private onScroll = (): void => {
		if (this.unit && !this.drag) this.schedule();
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
		if (this.drag) return;
		if (!pointer || !view.editable || view.isDestroyed || !this.near(pointer)) return this.hide();
		// On its way to the grip, the pointer keeps the block, unless the mode
		// changed under it and the grip must show the other kind.
		if (this.heading(pointer) && this.markFor(this.unit) === this.mark) return;
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
		} else if (unit.node.type.name === 'horizontalRule') {
			// Level with the line, which runs through the middle of the rule's box.
			top = (box.top + box.bottom) / 2 - GRIP_HEIGHT / 2;
		}
		this.showMark(levelMark(host, unit.node, block));
		// A list item's marker sits in its list's padding: the grip goes left of it.
		const list = unit.node.type.name === 'listItem' || unit.node.type.name === 'taskItem' ? block.parentElement : null;
		const width = this.mark ? this.dom.offsetWidth - MARK_PAD : GRIP_WIDTH;
		const left = (list ?? block).getBoundingClientRect().left - width - GRIP_GAP;
		// A scroll under a pointer resting on the grip moves it to another block: the tint goes with it.
		const aim = aimKey.getState(view.state);
		if (aim && aim.from !== unit.from) {
			const hovered = this.dom.matches(':hover');
			queueMicrotask(() => this.aim(hovered ? { from: unit.from, to: unit.to, pressed: false } : null));
		}
		this.unit = unit;
		this.dom.style.transform = `translate(${Math.round(left - hostBox.left)}px, ${Math.round(top - hostBox.top)}px)`;
		this.dom.classList.add('is-shown');
	}

	/** The mark the grip would show for `unit`, as it is drawn now. */
	private markFor(unit: Unit | null): string | null {
		const host = this.host();
		const block = unit && this.view.nodeDOM(unit.from);
		return unit && host && block instanceof HTMLElement ? levelMark(host, unit.node, block) : null;
	}

	/** Show `mark` in place of the dots, sized to it, or the dots again for null. */
	private showMark(mark: string | null): void {
		if (mark === this.mark) return;
		this.mark = mark;
		this.dom.classList.toggle('is-mark', mark !== null);
		if (mark) this.dom.textContent = mark;
		else this.dom.innerHTML = GRIP_SVG;
		this.dom.title = mark ? 'Drag to move. Click to change the level' : 'Drag to move. Click to select';
	}

	private onClick = (event: MouseEvent): void => {
		const { unit, view } = this;
		if (!unit || !view.editable) return;
		event.preventDefault();
		if (this.mark && this.options.headingMenu) {
			const box = this.dom.getBoundingClientRect();
			this.options.headingMenu(view, unit.from, { x: box.left, y: box.bottom + 4 });
			return;
		}
		const { state } = view;
		view.dispatch(state.tr.setSelection(NodeSelection.create(state.doc, unit.from)));
		view.focus();
	};

	private onGripDragStart = (event: DragEvent): void => {
		const { unit, view } = this;
		if (this.drag) this.reset();
		if (!unit || !event.dataTransfer || !view.editable) {
			event.preventDefault();
			return;
		}
		const { state } = view;
		const selection = NodeSelection.create(state.doc, unit.from);
		const slice = selection.content();
		view.dispatch(
			state.tr.setSelection(selection).setMeta(heldKey, { from: unit.from, to: unit.to, slice }).setMeta(aimKey, null),
		);
		const { dom, text } = view.serializeForClipboard(slice);
		event.dataTransfer.clearData();
		event.dataTransfer.setData('text/html', dom.innerHTML);
		event.dataTransfer.setData('text/plain', text);
		event.dataTransfer.effectAllowed = 'copyMove';
		// The browser's picture of the drag is faded and covers the drop
		// line: it shows nothing, and the chip drawn here rides instead.
		event.dataTransfer.setDragImage(this.blank, 0, 0);
		// As ProseMirror's own drag sets it, `node` included: the drop cursor
		// reads it, and so does a drop on another page.
		view.dragging = { slice, move: true, node: selection } as EditorView['dragging'];
		this.options.dragStarted?.(view);
		// Shown pressed. Hiding it would end the drag: Chrome ends one at once
		// when its source stops taking the pointer during dragstart.
		this.dom.classList.add('is-dragging');
		document.removeEventListener('mouseup', this.onGripUp, { capture: true });
		this.begin(event, unit.from);
	};

	private beforeDragStart = (): void => {
		this.draggingBefore = this.view.dragging;
	};

	/** A block widget dragged by itself, once ProseMirror has started the drag: it goes as a drag from its grip. */
	private onWidgetDragStart = (event: DragEvent): void => {
		const { view } = this;
		const dragging = view.dragging as { slice: Slice; node?: NodeSelection } | null;
		if (!dragging || dragging === this.draggingBefore || !event.dataTransfer || !view.editable) return;
		// The dragged node, or the block selected whole that the drag began in.
		const selection = dragging.node ?? view.state.selection;
		if (!(selection instanceof NodeSelection) || !selection.node.isBlock) return;
		if (this.drag) this.reset();
		view.dispatch(view.state.tr.setMeta(heldKey, { from: selection.from, to: selection.to, slice: dragging.slice }));
		event.dataTransfer.setDragImage(this.blank, 0, 0);
		this.hide();
		this.begin(event, selection.from);
	};

	private begin(event: DragEvent, from: number): void {
		const source = event.target ?? this.dom;
		this.drag = { source, scroller: scrollerOf(this.view.dom), ended: false };
		this.landed = false;
		this.dropped = false;
		this.lift(from, event.clientX, event.clientY);
		// On the element itself: one a drop took out of the page still hears its end there.
		source.addEventListener('dragend', this.onDragEnd);
		this.dragPointer = event.clientY;
		document.addEventListener('dragover', this.onDragOver, { capture: true });
		document.addEventListener('dragleave', this.onDragLeave, { capture: true });
		document.addEventListener('drop', this.onDrop, { capture: true });
		this.scrollFrame = requestAnimationFrame(this.scrollStep);
	}

	/** Tell assistive technology where a block landed. */
	say(words: string): void {
		this.said.textContent = words;
	}

	private inText(target: EventTarget | null): boolean {
		return target instanceof Node && this.view.dom.contains(target);
	}

	/**
	 * Where a drag over the page's margin, beside the text and level with it,
	 * lands: the same height at the text's nearer edge. Null anywhere else.
	 */
	private besideText(event: { target: EventTarget | null; clientX: number; clientY: number }): At | null {
		const { drag, view } = this;
		const { target } = event;
		if (!drag || !(target instanceof Node) || this.inText(target)) return null;
		// With the window as the scroller, the editor's own host is as far as the margin goes.
		const zone = drag.scroller === document.scrollingElement ? this.host() : drag.scroller;
		if (!zone?.contains(target)) return null;
		const box = view.dom.getBoundingClientRect();
		if (event.clientY < box.top || event.clientY > box.bottom) return null;
		if (event.clientX >= box.left && event.clientX <= box.right) return null;
		return { clientX: Math.min(Math.max(event.clientX, box.left + 1), box.right - 1), clientY: event.clientY };
	}

	private onDragOver = (event: DragEvent): void => {
		if (this.relaying) return;
		this.dragPointer = event.clientY;
		this.dragPointerX = event.clientX;
		this.showCopying(copies(event));
		const at = this.besideText(event);
		if (!at) {
			// Off the margin: over the text the drop cursor reads the drag
			// itself; anywhere else the line goes.
			if (this.relayed && !this.inText(event.target)) clearDropLine(this.view);
			this.relayed = false;
			return;
		}
		event.preventDefault();
		if (event.dataTransfer) event.dataTransfer.dropEffect = copies(event) ? 'copy' : 'move';
		this.relaying = true;
		try {
			// Not bubbling: only the editor's own listeners hear it.
			this.view.dom.dispatchEvent(
				new MouseEvent('dragover', {
					clientX: at.clientX,
					clientY: at.clientY,
					altKey: event.altKey,
					ctrlKey: event.ctrlKey,
					metaKey: event.metaKey,
					shiftKey: event.shiftKey,
				}),
			);
		} finally {
			this.relaying = false;
		}
		this.relayed = true;
	};

	/**
	 * The drop cursor takes the line away when a drag leaves the text. For
	 * the margin beside it, it stays: the margin's next dragover moves it.
	 */
	private onDragLeave = (event: DragEvent): void => {
		if (!this.inText(event.target) || this.inText(event.relatedTarget)) return;
		if (this.besideText({ target: event.relatedTarget, clientX: event.clientX, clientY: event.clientY })) event.stopPropagation();
	};

	/** A drop in the margin is taken here; one on the text goes through ProseMirror to `handleDrop`. */
	private onDrop = (event: DragEvent): void => {
		this.dropped = true;
		const at = this.besideText(event);
		if (at && heldKey.getState(this.view.state)) {
			event.preventDefault();
			event.stopPropagation();
			clearDropLine(this.view);
			this.relayed = false;
			this.land(at, !copies(event));
		}
		clearTimeout(this.dropTimer);
		this.dropTimer = setTimeout(this.endDrag, DROP_END_MS);
	};

	/** Put the held block where a drop at `at` puts it, moved or copied, and give the editor the focus. */
	land(at: At, moved: boolean): void {
		const { view } = this;
		this.host()?.classList.remove('doc-drop-idle');
		const held = heldKey.getState(view.state);
		const node = held && view.state.doc.nodeAt(held.from);
		const target = held && node ? dropTarget(view, at, held, moved) : null;
		if (held && node && target !== null) {
			this.landed = true;
			const before = reducedMotion() ? null : measure(view);
			if (!moved) this.copyTo(node, target, before);
			else if (moveUnitTo(view, held.from, held.to, target)) {
				if (before) slideMoved(view, before);
				const { selection } = view.state;
				if (selection instanceof NodeSelection) glowLanded(view, selection.from, selection.to);
				this.say(landedWords(view.state));
			}
		}
		view.focus();
	}

	/** Put a copy of `node`, with ids of its own, in at `target`, selected; it fades in there. */
	private copyTo(node: PmNode, target: number, before: Before | null): void {
		const { view } = this;
		const tr = view.state.tr.replaceRangeWith(target, target, withoutIds(node));
		if (tr.doc.eq(view.state.doc)) return;
		const placed = insertedAt(tr, target, node.type);
		if (placed !== null) tr.setSelection(NodeSelection.create(tr.doc, placed));
		view.dispatch(tr.setMeta('uiEvent', 'drop').scrollIntoView());
		if (before) slideMoved(view, before);
		const { selection } = view.state;
		const copy = placed !== null && selection instanceof NodeSelection ? view.nodeDOM(selection.from) : null;
		if (copy instanceof HTMLElement) copy.animate([{ opacity: 0 }, { opacity: 1 }], dropMotion());
		if (placed !== null && selection instanceof NodeSelection) glowLanded(view, selection.from, selection.to);
		this.say('Block copied');
	}

	/** Make the chip for the block at `from`, beside the pointer at `x`, `y`. */
	private lift(from: number, x: number, y: number): void {
		this.chip?.remove();
		this.chip = null;
		const node = this.view.state.doc.nodeAt(from);
		const el = this.view.nodeDOM(from);
		if (!node || !(el instanceof HTMLElement)) return;
		// On the window, not the page: it goes wherever the pointer goes
		// without making the page wider or taller.
		const chip = chipOf(node, el);
		document.body.append(chip);
		this.chip = chip;
		this.dragPointerX = x;
		this.carry(x, y);
		// Out of the grip, so you see the block come into your hand. A widget
		// dragged by itself has no grip shown: its chip fades in where it is.
		const grip = this.dom.classList.contains('is-dragging') ? this.dom.getBoundingClientRect() : null;
		const frames: Keyframe[] =
			grip && !reducedMotion()
				? [
						{ transform: `translate(${Math.round(grip.left)}px, ${Math.round(grip.top)}px) scale(0.6)`, opacity: 0 },
						{ transform: chip.style.transform, opacity: 1 },
					]
				: [{ opacity: 0 }, { opacity: 1 }];
		chip.animate(frames, grip && !reducedMotion() ? dropMotion() : { duration: motionMs('--motion-hover-in', 120) });
	}

	/** Show that the drop will copy (the modifier held) or move: the chip's +, and the old place no longer dimmed. */
	private showCopying(on: boolean): void {
		this.chip?.classList.toggle('is-copy', on);
		this.host()?.classList.toggle('doc-drag-copying', on);
	}

	/** Keep the chip below and right of the pointer at `x`, `y`, inside the window. */
	private carry(x: number, y: number): void {
		const { chip } = this;
		if (!chip) return;
		const left = Math.min(x + CHIP_OFFSET_X, window.innerWidth - chip.offsetWidth - 8);
		const top = Math.min(y + CHIP_OFFSET_Y, window.innerHeight - chip.offsetHeight - 8);
		chip.style.transform = `translate(${Math.round(left)}px, ${Math.round(top)}px)`;
	}

	/** Take the chip away: back to the grip when the drag was called off, faded where it is otherwise. */
	private putDown(home: boolean): void {
		const { chip } = this;
		this.chip = null;
		if (!chip) return;
		const grip = this.dom.getBoundingClientRect();
		const back = home && !reducedMotion() && grip.width > 0;
		// It fades as marks do, or travels home as moved things do (design.md).
		let frames: Keyframe[] = [{ opacity: 1 }, { opacity: 0 }];
		let timing: KeyframeAnimationOptions = { duration: motionMs('--motion-hover-out', 150), easing: dropMotion().easing };
		if (back) {
			timing = dropMotion();
			frames = [
				{ transform: chip.style.transform, opacity: 1 },
				{ transform: `translate(${Math.round(grip.left)}px, ${Math.round(grip.top)}px) scale(0.6)`, opacity: 0 },
			];
		}
		const done = (): void => chip.remove();
		const run = chip.animate(frames, { ...timing, fill: 'forwards' });
		run.onfinish = done;
		run.oncancel = done;
	}

	/** One frame of scrolling while a block is dragged near the top or bottom of the page's scroller. */
	private scrollStep = (): void => {
		const y = this.dragPointer;
		const scroller = this.drag?.scroller;
		if (y !== null) this.carry(this.dragPointerX, y);
		if (y !== null && scroller && !this.view.isDestroyed) {
			const page = scroller === document.scrollingElement;
			const box = page ? { top: 0, bottom: window.innerHeight } : scroller.getBoundingClientRect();
			const into = y < box.top + EDGE ? -(box.top + EDGE - y) : y > box.bottom - EDGE ? y - (box.bottom - EDGE) : 0;
			if (into) {
				const step = Math.sign(into) * Math.ceil(EDGE_STEP * Math.min(1, Math.abs(into) / EDGE) ** 2);
				scroller.scrollBy({ top: step });
			}
		}
		this.scrollFrame = requestAnimationFrame(this.scrollStep);
	};

	private stopScrolling(): void {
		cancelAnimationFrame(this.scrollFrame);
		this.dragPointer = null;
		document.removeEventListener('dragover', this.onDragOver, { capture: true });
		document.removeEventListener('dragleave', this.onDragLeave, { capture: true });
	}

	/**
	 * A drag the browser called off (Escape, or let go where nothing takes
	 * it) sends the chip home. WebKit can say `none` after a drop that
	 * worked, so a drop anywhere in the window counts as one.
	 */
	private onDragEnd = (event: Event): void =>
		this.endDrag((event as DragEvent).dataTransfer?.dropEffect === 'none' && !this.dropped);

	/**
	 * The drag is over, dropped or not. Nothing it drew stays: the line, the
	 * idle mark, the faded grip; the dimmed block comes back once a drop that
	 * arrives late has been read (`settle`). The editor takes the focus back
	 * unless the drop put it in another editor or a field.
	 */
	private endDrag = (cancelled = false): void => {
		const { drag, view } = this;
		if (!drag || drag.ended) return;
		drag.ended = true;
		this.putDown(cancelled === true && !this.landed);
		clearTimeout(this.dropTimer);
		drag.source.removeEventListener('dragend', this.onDragEnd);
		this.stopScrolling();
		this.relayed = false;
		this.host()?.classList.remove('doc-drop-idle', 'doc-drag-copying');
		this.dom.classList.remove('is-dragging');
		// Mouse events stop during a drag: the pointer last read is from before it.
		this.pointer = null;
		this.hide();
		if (view.isDestroyed) return;
		clearDropLine(view);
		this.options.dragEnded?.(view);
		const active = document.activeElement;
		const elsewhere =
			active instanceof HTMLElement &&
			!this.host()?.contains(active) &&
			(active.isContentEditable || active.matches('input, textarea'));
		if (view.editable && !elsewhere) view.focus();
		this.endTimer = setTimeout(this.settle, SETTLE_MS);
	};

	/** After the drop has read it, as ProseMirror clears its own: the held block, and `view.dragging` if it is this drag's. */
	private settle = (): void => {
		clearTimeout(this.endTimer);
		document.removeEventListener('drop', this.onDrop, { capture: true });
		this.drag = null;
		const { view } = this;
		if (view.isDestroyed) return;
		const held = heldKey.getState(view.state);
		if (held && view.dragging?.slice === held.slice) view.dragging = null;
		if (held) view.dispatch(view.state.tr.setMeta(heldKey, null));
	};

	/** End a drag at once, as another starts. */
	private reset(): void {
		this.endDrag();
		this.settle();
	}
}

export const DragHandle = Extension.create<DragHandleOptions>({
	name: 'dragHandle',
	// Ahead of the page's other drop handlers (a paste of what was dropped).
	priority: 200,
	addOptions() {
		return { dragStarted: null, dragEnded: null, headingMenu: null };
	},
	addProseMirrorPlugins() {
		const options = this.options;
		let grip: Grip | null = null;
		return [
			new Plugin<Aim | null>({
				key: aimKey,
				state: {
					init: () => null,
					apply(tr: Transaction, aim: Aim | null): Aim | null {
						const meta = tr.getMeta(aimKey) as Aim | null | undefined;
						if (meta !== undefined) return meta;
						return tr.docChanged ? null : aim;
					},
				},
				props: {
					// What the grip under the pointer would move.
					decorations(state) {
						const aim = aimKey.getState(state);
						if (!aim || !state.doc.nodeAt(aim.from)) return null;
						const cls = aim.pressed ? 'doc-drag-aim is-pressed' : 'doc-drag-aim';
						return DecorationSet.create(state.doc, [Decoration.node(aim.from, aim.to, { class: cls })]);
					},
				},
			}),
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
					// The block being dragged, dimmed where it was until it lands.
					decorations(state) {
						const held = heldKey.getState(state);
						if (!held || !state.doc.nodeAt(held.from)) return null;
						return DecorationSet.create(state.doc, [Decoration.node(held.from, held.to, { class: 'doc-drag-source' })]);
					},
					handleDOMEvents: {
						// No drop line where a drop would do nothing.
						dragover(view, event) {
							const held = heldKey.getState(view.state);
							if (held && view.dragging?.slice === held.slice) {
								const idle = dropTarget(view, event, held, !copies(event)) === null;
								view.dom.parentElement?.classList.toggle('doc-drop-idle', idle);
							}
							return false;
						},
					},
					// The slice given here has been through `transformPasted`, so the
					// drag is known by the one `view.dragging` still holds.
					handleDrop(view, event, _slice, moved) {
						const held = heldKey.getState(view.state);
						if (!held || view.dragging?.slice !== held.slice || !grip) return false;
						grip.land(event, moved);
						return true;
					},
				},
			}),
		];
	},
});
