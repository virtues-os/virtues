/**
 * Who else is on the page, and what the assistant is doing to it.
 *
 * - `localUser`: how this device appears in others' carets: its kind
 *   ("Computer", "Phone", "Tablet") and a category colour picked by its Yjs
 *   client id, so two devices of one person tell apart.
 * - The assistant's caret, telegraph and trail (`setAiCaret`,
 *   `setAiTelegraph`, `setAiTrail`, `clearAi`), the look of the CodeMirror
 *   editor's (`codemirror/extensions/ai-cursor.ts`): a caret that glides
 *   between positions, a wash over the words a rewrite is about to replace
 *   while the model thinks, and a fading wash over what it wrote. The caret
 *   is one overlay element positioned from the document; the telegraph and
 *   the trail are decorations, so all three follow edits made around them.
 * - `flashBlocks`: a brief wash over the blocks a chat edit wrote.
 */

import { Extension, type Editor } from '@tiptap/core';
import { Plugin, PluginKey, type EditorState, type Transaction } from '@tiptap/pm/state';
import { Decoration, DecorationSet, type EditorView } from '@tiptap/pm/view';
import { contract } from './schema';

/** The category colours (`--cat-*` in themes.css) a device's caret takes. */
const CARET_COLORS = ['orange', 'cyan', 'violet', 'emerald', 'rose', 'indigo', 'yellow', 'pink', 'purple'];

/** How long a flash and a trail stay before they fade out. */
const FLASH_MS = 1400;

/**
 * How a device appears in others' carets. `tint` names its category
 * colour, which each viewer draws in its own theme; `color` is that colour's
 * value here, as six-digit hex, the only form Tiptap's caret plugin passes
 * on from another device (anything else could carry CSS).
 */
export interface CaretUser {
	name: string;
	color?: string;
	tint: string;
}

/** This device as others see it: its kind, and a colour of its own. */
export function localUser(clientId: number): CaretUser {
	const ua = typeof navigator === 'undefined' ? '' : navigator.userAgent;
	const touch = typeof navigator !== 'undefined' && navigator.maxTouchPoints > 1;
	const tablet = /iPad|Tablet/i.test(ua) || (/Macintosh/.test(ua) && touch);
	const phone = !tablet && /iPhone|Android.+Mobile|Mobile/i.test(ua);
	const name = phone ? 'Phone' : tablet ? 'Tablet' : 'Computer';
	const tint = CARET_COLORS[Math.abs(clientId) % CARET_COLORS.length];
	const value =
		typeof document === 'undefined'
			? ''
			: getComputedStyle(document.documentElement).getPropertyValue(`--cat-${tint}`).trim();
	return /^#[0-9a-f]{6}$/i.test(value) ? { name, color: value, tint } : { name, tint };
}

/** The colour to draw another device in: its tint in this theme, else the hex it sent. */
function caretColor(user: Record<string, unknown>): string {
	const tint = String(user.tint ?? '');
	if (CARET_COLORS.includes(tint)) return `var(--cat-${tint})`;
	const color = String(user.color ?? '');
	return /^#[0-9a-f]{6}$/i.test(color) ? color : 'currentColor';
}

/**
 * The look of another device's caret: a bar in its colour, its name above.
 * It sits in the text, so it is hidden from assistive technology, as the
 * assistant's caret is: a screen reader read its name as words of the page.
 */
export function renderCaret(user: Record<string, unknown>): HTMLElement {
	const caret = document.createElement('span');
	caret.className = 'collaboration-carets__caret';
	caret.setAttribute('aria-hidden', 'true');
	caret.style.setProperty('--caret-color', caretColor(user));
	const label = document.createElement('span');
	label.className = 'collaboration-carets__label';
	label.textContent = String(user.name ?? '');
	caret.append(label);
	return caret;
}

export function renderSelection(user: Record<string, unknown>) {
	return {
		nodeName: 'span',
		class: 'collaboration-carets__selection',
		style: `--caret-color: ${caretColor(user)}`,
	};
}

// ── The assistant's caret and trail ───────────────────────────────────────

export type AiCaretPhase = 'active' | 'done';
export interface AiCaret {
	pos: number;
	phase: AiCaretPhase;
}

interface AiState {
	caret: AiCaret | null;
	telegraph: DecorationSet;
	trail: DecorationSet;
	flash: DecorationSet;
}

type AiMeta =
	| { caret: AiCaret | null }
	| { telegraph: { from: number; to: number } | null }
	| { trail: { from: number; to: number } }
	| { flash: string[] }
	| { unflash: true }
	| { clear: true };

const aiKey = new PluginKey<AiState>('aiPresence');

function flashDecorations(doc: EditorState['doc'], ids: Set<string>): DecorationSet {
	const decos: Decoration[] = [];
	doc.descendants((node, pos) => {
		const id = node.attrs[contract.id.attr];
		if (typeof id === 'string' && ids.has(id)) {
			decos.push(Decoration.node(pos, pos + node.nodeSize, { class: 'doc-flash' }));
			return false;
		}
		return true;
	});
	return DecorationSet.create(doc, decos);
}

function applyAi(tr: Transaction, prev: AiState): AiState {
	let { caret, telegraph, trail, flash } = prev;
	if (tr.docChanged) {
		if (caret) caret = { ...caret, pos: tr.mapping.map(caret.pos, 1) };
		telegraph = telegraph.map(tr.mapping, tr.doc);
		trail = trail.map(tr.mapping, tr.doc);
		flash = flash.map(tr.mapping, tr.doc);
	}
	const meta = tr.getMeta(aiKey) as AiMeta | undefined;
	if (!meta) {
		const same = caret === prev.caret && telegraph === prev.telegraph && trail === prev.trail && flash === prev.flash;
		return same ? prev : { caret, telegraph, trail, flash };
	}
	if ('caret' in meta) caret = meta.caret;
	else if ('telegraph' in meta) {
		const range = meta.telegraph;
		telegraph =
			range && range.to > range.from
				? DecorationSet.create(tr.doc, [Decoration.inline(range.from, range.to, { class: 'doc-ai-telegraph' })])
				: DecorationSet.empty;
	} else if ('trail' in meta) {
		const { from, to } = meta.trail;
		if (to > from) trail = trail.add(tr.doc, [Decoration.inline(from, to, { class: 'doc-ai-trail' })]);
	} else if ('flash' in meta) flash = flashDecorations(tr.doc, new Set(meta.flash));
	else if ('unflash' in meta) flash = DecorationSet.empty;
	else if ('clear' in meta) {
		caret = null;
		telegraph = DecorationSet.empty;
		trail = DecorationSet.empty;
	}
	return { caret, telegraph, trail, flash };
}

/** The overlay caret: one element in the editor's positioned parent, moved by transform so it glides. */
class CaretOverlay {
	private dom: HTMLElement | null = null;

	constructor(private view: EditorView) {
		this.render();
	}

	render() {
		const caret = aiKey.getState(this.view.state)?.caret ?? null;
		const host = this.view.dom.parentElement;
		if (!caret || !host) {
			this.remove();
			return;
		}
		let coords: { left: number; top: number; bottom: number };
		try {
			coords = this.view.coordsAtPos(Math.min(caret.pos, this.view.state.doc.content.size));
		} catch {
			this.remove();
			return;
		}
		if (!this.dom) {
			this.dom = document.createElement('div');
			this.dom.className = 'doc-ai-caret';
			this.dom.setAttribute('aria-hidden', 'true');
			const bar = document.createElement('span');
			bar.className = 'doc-ai-caret-bar';
			const label = document.createElement('span');
			label.className = 'doc-ai-caret-label';
			label.textContent = 'Virtues';
			this.dom.append(bar, label);
			host.appendChild(this.dom);
		}
		this.dom.classList.toggle('doc-ai-caret--done', caret.phase === 'done');
		const box = host.getBoundingClientRect();
		const x = coords.left - box.left + host.scrollLeft;
		const y = coords.top - box.top + host.scrollTop;
		this.dom.style.transform = `translate(${x}px, ${y}px)`;
		this.dom.style.height = `${coords.bottom - coords.top}px`;
	}

	remove() {
		this.dom?.remove();
		this.dom = null;
	}
}

export function aiPresence(): Extension {
	return Extension.create({
		name: 'pageAiPresence',
		addProseMirrorPlugins() {
			return [
				new Plugin<AiState>({
					key: aiKey,
					state: {
						init: () => ({
							caret: null,
							telegraph: DecorationSet.empty,
							trail: DecorationSet.empty,
							flash: DecorationSet.empty,
						}),
						apply: applyAi,
					},
					props: {
						decorations(state) {
							const s = aiKey.getState(state);
							if (!s) return null;
							const more = [...s.telegraph.find(), ...s.flash.find()];
							return more.length ? s.trail.add(state.doc, more) : s.trail;
						},
					},
					view(view) {
						const overlay = new CaretOverlay(view);
						return {
							update: () => overlay.render(),
							destroy: () => overlay.remove(),
						};
					},
				}),
			];
		},
	});
}

function dispatchAi(editor: Editor, meta: AiMeta): void {
	if (editor.isDestroyed) return;
	editor.view.dispatch(editor.state.tr.setMeta(aiKey, meta).setMeta('addToHistory', false));
}

/** Show the assistant's caret at `caret.pos`, or hide it with null. */
export function setAiCaret(editor: Editor, caret: AiCaret | null): void {
	dispatchAi(editor, { caret });
}

/**
 * Wash the words a rewrite is about to replace, while the model thinks, or
 * take the wash off with null: the first words mark them as going.
 */
export function setAiTelegraph(editor: Editor, range: { from: number; to: number } | null): void {
	dispatchAi(editor, { telegraph: range });
}

/** The words the telegraph washes now, if any. */
export function aiTelegraphOf(state: EditorState): { from: number; to: number } | null {
	const found = aiKey.getState(state)?.telegraph.find() ?? [];
	return found.length ? { from: found[0].from, to: found[found.length - 1].to } : null;
}

/** Wash the range the assistant just wrote; the wash fades by itself. */
export function setAiTrail(editor: Editor, range: { from: number; to: number }): void {
	dispatchAi(editor, { trail: range });
}

/** Remove the assistant's caret and trail. */
export function clearAi(editor: Editor): void {
	dispatchAi(editor, { clear: true });
}

/** Where the assistant's caret is, if it shows one. */
export function aiCaretOf(editor: Editor): AiCaret | null {
	return aiKey.getState(editor.state)?.caret ?? null;
}

/** Briefly wash the blocks with these ids: what a chat edit just wrote. */
export function flashBlocks(editor: Editor, ids: string[]): void {
	if (!ids.length) return;
	dispatchAi(editor, { flash: ids });
	setTimeout(() => dispatchAi(editor, { unflash: true }), FLASH_MS);
}

/** The blocks washed now, by id: for tests and for code that waits on the flash. */
export function flashedBlocks(state: EditorState): string[] {
	const s = aiKey.getState(state);
	if (!s) return [];
	const ids: string[] = [];
	for (const d of s.flash.find()) {
		const node = state.doc.nodeAt(d.from);
		const id = node?.attrs[contract.id.attr];
		if (typeof id === 'string') ids.push(id);
	}
	return ids;
}
