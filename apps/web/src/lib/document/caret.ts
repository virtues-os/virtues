/**
 * The drawn caret, for pages and the chat composer.
 *
 * The browser's caret is one hairline pixel with a square blink and no memory
 * of where it just was. This hides it and draws a bar of our own over the
 * editor, with three properties that make a caret feel like a place rather
 * than a marker:
 *
 *  1. It is sized to the type. The bar's height follows the FONT SIZE at the
 *     caret, not the line box, so it grows inside a heading without taking on
 *     the prose leading, which would make it tower over body copy. Its width
 *     grows with its height, so it keeps its proportions at every size.
 *
 *  2. It glides between positions on the keyboard (105ms) instead of jumping.
 *     The curve is front-loaded: it leaves fast and settles, because a
 *     symmetric curve at this duration feels like lag. Typed characters glide
 *     in 60ms, since keys land every 60 to 100ms at speed and a longer glide
 *     leaves the bar behind the letters.
 *
 *  3. It holds back. A click moves it at once, never gliding toward the spot
 *     someone just pointed at, which is the fastest way to make an editor feel
 *     slow. It stays solid while you work and only blinks after 500ms of
 *     stillness, and the blink is a soft fade (long on, quick dip), so it
 *     reads as breathing rather than flicker.
 *
 * The bar shows only for an empty text selection in a focused, editable
 * editor. A range is shown by its highlight, and a node or gap selection by
 * its own outline. While an input method is composing, the browser's caret
 * comes back, because the composition is drawn by the platform.
 *
 * The assistant's caret on a page (`presence.ts`) is the same kind of
 * overlay: a positioned bar with an eased transition.
 */

import { Extension } from '@tiptap/core';
import { Plugin, PluginKey, TextSelection } from '@tiptap/pm/state';
import type { EditorView } from '@tiptap/pm/view';
import './caret.css';

/** How long the caret stays solid after the last keystroke or move. */
const BLINK_DELAY_MS = 500;

/** Bar height as a multiple of the font size at the caret. */
const CARET_HEIGHT_RATIO = 1.25;

/** Width follows height, so the bar keeps its proportions up to a heading. */
function caretWidthFor(height: number): number {
	return height * 0.055 + 1.1;
}

/**
 * A key that types: its caret move gets the short glide. `keydown` and
 * `mousedown` fire before the selection they cause, so the input kind is set
 * before the bar moves.
 */
function typingKey(event: KeyboardEvent): boolean {
	return (
		(event.key.length === 1 && !event.metaKey && !event.ctrlKey) ||
		event.key === 'Backspace' ||
		event.key === 'Delete' ||
		event.key === 'Enter'
	);
}

class DrawnCaret {
	private bar: HTMLElement;
	private host: HTMLElement | null;
	private blinkTimer: ReturnType<typeof setTimeout> | null = null;
	private frame = 0;
	private shown = false;
	private composing = false;
	private resizes: ResizeObserver | null = null;
	private readonly schedule = () => {
		cancelAnimationFrame(this.frame);
		this.frame = requestAnimationFrame(() => this.render());
	};
	private readonly onCompositionStart = () => {
		this.composing = true;
		this.render();
	};
	private readonly onCompositionEnd = () => {
		this.composing = false;
		this.schedule();
	};

	constructor(private view: EditorView) {
		this.bar = document.createElement('div');
		this.bar.className = 'doc-caret';
		this.bar.hidden = true;
		this.bar.setAttribute('aria-hidden', 'true');
		this.host = null;
		carets.set(view, this);
		view.dom.addEventListener('focus', this.schedule);
		view.dom.addEventListener('blur', this.schedule);
		view.dom.addEventListener('compositionstart', this.onCompositionStart);
		view.dom.addEventListener('compositionend', this.onCompositionEnd);
		// Text that reflows moves the caret on screen without a transaction:
		// an image loading above it, the window or the composer resizing.
		window.addEventListener('resize', this.schedule);
		if (typeof ResizeObserver !== 'undefined') {
			this.resizes = new ResizeObserver(this.schedule);
			this.resizes.observe(view.dom);
		}
		this.schedule();
	}

	update() {
		this.schedule();
	}

	/** What moved the caret last: the CSS drops the glide for a pointer. */
	setInput(kind: 'pointer' | 'typing' | 'keyboard') {
		this.bar.dataset.input = kind;
	}

	/**
	 * The editor's parent, which the bar is placed against. Found on first
	 * use, since the editor may be mounted after its plugins start.
	 */
	private attach(): HTMLElement | null {
		const parent = this.view.dom.parentElement;
		if (this.host === parent) return parent;
		this.host?.classList.remove('doc-caret-host');
		this.host = parent;
		if (!parent) return null;
		if (getComputedStyle(parent).position === 'static') parent.classList.add('doc-caret-host');
		parent.appendChild(this.bar);
		return parent;
	}

	destroy() {
		cancelAnimationFrame(this.frame);
		this.clearBlink();
		delete this.view.dom.dataset.caretComposing;
		this.view.dom.removeEventListener('focus', this.schedule);
		this.view.dom.removeEventListener('blur', this.schedule);
		this.view.dom.removeEventListener('compositionstart', this.onCompositionStart);
		this.view.dom.removeEventListener('compositionend', this.onCompositionEnd);
		window.removeEventListener('resize', this.schedule);
		this.resizes?.disconnect();
		this.bar.remove();
		this.host?.classList.remove('doc-caret-host');
		carets.delete(this.view);
	}

	private render() {
		const { view, bar } = this;
		const host = view.isDestroyed ? null : this.attach();
		// A data attribute, since ProseMirror rewrites the element's class
		// from its props on every update.
		if (this.composing) view.dom.dataset.caretComposing = '';
		else delete view.dom.dataset.caretComposing;
		const sel = view.state.selection;
		const visible =
			!!host &&
			!view.isDestroyed &&
			view.editable &&
			view.hasFocus() &&
			!this.composing &&
			sel instanceof TextSelection &&
			sel.empty;
		if (!visible || !host) return this.hide();

		let coords: { left: number; top: number; bottom: number };
		try {
			coords = view.coordsAtPos(sel.head);
		} catch {
			return this.hide();
		}
		const { node } = view.domAtPos(sel.head);
		const element = node.nodeType === Node.TEXT_NODE ? node.parentElement : (node as HTMLElement);
		const fontSize = element ? parseFloat(getComputedStyle(element).fontSize) : 0;
		const lineBox = coords.bottom - coords.top;
		const height = fontSize > 0 ? fontSize * CARET_HEIGHT_RATIO : lineBox;
		const width = caretWidthFor(height);

		const box = host.getBoundingClientRect();
		// Centered on the glyph box, which is where the text sits, and
		// straddling the character boundary rather than sitting to its right.
		const x = coords.left - box.left + host.scrollLeft - width / 2;
		const y = (coords.top + coords.bottom) / 2 - height / 2 - box.top + host.scrollTop;

		// Arriving from nowhere is a jump, never a glide from where it was
		// last shown.
		bar.classList.toggle('doc-caret--arriving', !this.shown);
		bar.style.width = `${width.toFixed(2)}px`;
		bar.style.height = `${height.toFixed(2)}px`;
		bar.style.transform = `translate(${x.toFixed(2)}px, ${y.toFixed(2)}px)`;
		bar.hidden = false;
		this.shown = true;
		this.restartBlink();
	}

	private hide() {
		this.clearBlink();
		this.bar.hidden = true;
		this.shown = false;
	}

	private clearBlink() {
		if (this.blinkTimer !== null) clearTimeout(this.blinkTimer);
		this.blinkTimer = null;
		this.bar.classList.remove('doc-caret--blinking');
	}

	/**
	 * Solid now, blinking once the writer stops. Removing and adding the class
	 * restarts the animation, so every move resets its phase and the bar is
	 * never caught mid-fade where it arrives.
	 */
	private restartBlink() {
		this.clearBlink();
		this.blinkTimer = setTimeout(() => {
			this.blinkTimer = null;
			if (this.shown) this.bar.classList.add('doc-caret--blinking');
		}, BLINK_DELAY_MS);
	}
}

const carets = new WeakMap<EditorView, DrawnCaret>();

const caretKey = new PluginKey('drawnCaret');

export function drawnCaret(): Extension {
	return Extension.create({
		name: 'drawnCaret',
		addProseMirrorPlugins() {
			return [
				new Plugin({
					key: caretKey,
					view: (view) => new DrawnCaret(view),
					props: {
						attributes: { class: 'doc-caret-drawn' },
						handleDOMEvents: {
							mousedown: (view) => {
								carets.get(view)?.setInput('pointer');
								return false;
							},
							touchstart: (view) => {
								carets.get(view)?.setInput('pointer');
								return false;
							},
							keydown: (view, event) => {
								carets.get(view)?.setInput(typingKey(event) ? 'typing' : 'keyboard');
								return false;
							},
						},
					},
				}),
			];
		},
	});
}
