/**
 * Typed triggers: a character typed at the start of a block or after a space
 * (`/` for the insert menu, `@` for a mention) opens a menu, and the text
 * typed after it is the menu's query until a space, a move away or Escape.
 *
 * Only the person's own typing opens one. Remote transactions (another
 * device, the model) never do, and nothing opens while an input method is
 * composing: a `/` typed mid-composition may yet be part of a word. Once
 * the composition is done, a trigger it wrote at the start of a word opens
 * where it landed (`composedTrigger`), as one typed does: an Android
 * keyboard composes `@` with the word after it, and ProseMirror reads the
 * composed text without asking the plugin again.
 */

import { Extension } from '@tiptap/core';
import { isChangeOrigin } from '@tiptap/extension-collaboration';
import { Plugin, PluginKey, TextSelection, type EditorState } from '@tiptap/pm/state';
import type { EditorView } from '@tiptap/pm/view';

export interface TriggerCallbacks {
	/** The trigger character. */
	char: string;
	/** A menu opens at the trigger's screen position; `from` is the trigger's position. */
	onOpen: (coords: { x: number; y: number }, from: number) => void;
	/** The text typed after the trigger changed. */
	onQuery: (query: string) => void;
	onClose: () => void;
}

export interface TriggerState {
	active: boolean;
	/** Position of the trigger character. */
	from: number;
	query: string;
}

const CLOSED: TriggerState = { active: false, from: 0, query: '' };

/** One key per character, shared by every editor, so the state can be read by character. */
const keys = new Map<string, PluginKey<TriggerState>>();
function keyFor(char: string): PluginKey<TriggerState> {
	let key = keys.get(char);
	if (!key) {
		key = new PluginKey<TriggerState>(`trigger:${char}`);
		keys.set(char, key);
	}
	return key;
}

/** What may sit right before a trigger: the start of the block or a space. */
function opensAt(state: EditorState, pos: number): boolean {
	const $pos = state.doc.resolve(pos);
	if (!$pos.parent.isTextblock || $pos.parent.type.spec.code) return false;
	if ($pos.parentOffset === 0) return true;
	const before = $pos.parent.textBetween($pos.parentOffset - 1, $pos.parentOffset, undefined, '￼');
	return /\s/.test(before);
}

/** The trigger state after `state` changed under it, or closed when it no longer holds. */
function follow(prev: TriggerState, state: EditorState, char: string): TriggerState {
	if (!prev.active) return prev;
	const { from } = prev;
	const sel = state.selection;
	if (!sel.empty || sel.from <= from || from >= state.doc.content.size) return CLOSED;
	if (state.doc.textBetween(from, from + 1, undefined, '￼') !== char) return CLOSED;
	const $from = state.doc.resolve(from);
	const $head = sel.$head;
	if ($from.parent !== $head.parent) return CLOSED;
	const query = state.doc.textBetween(from + 1, sel.head, undefined, '￼');
	if (/\s/.test(query)) return CLOSED;
	return query === prev.query ? prev : { active: true, from, query };
}

type Meta = { open: number } | { close: true };

/** Where the caret was as a composition began: its textblock's start, and its offset there. */
interface ComposedFrom {
	start: number;
	offset: number;
}

/**
 * Where a trigger a composition wrote is, once it is done: the caret sits
 * after `char` at the start of a word, in the text written since the
 * composition began at `from`, with no space after it. Null for none.
 */
function composedTrigger(state: EditorState, char: string, from: ComposedFrom): number | null {
	const { selection } = state;
	if (!selection.empty) return null;
	const $head = selection.$head;
	if ($head.start() !== from.start || !$head.parent.isTextblock || $head.parent.type.spec.code) return null;
	const before = $head.parent.textBetween(0, $head.parentOffset, undefined, '￼');
	const i = before.lastIndexOf(char);
	if (i < from.offset || /\s/.test(before.slice(i + 1))) return null;
	if (i > 0 && !/\s/.test(before[i - 1])) return null;
	return $head.start() + i;
}

export function triggerPlugin(cb: TriggerCallbacks): Plugin<TriggerState> {
	const key = keyFor(cb.char);
	const composing = new WeakMap<EditorView, ComposedFrom>();
	return new Plugin<TriggerState>({
		key,
		state: {
			init: () => CLOSED,
			apply(tr, prev, _old, state) {
				const meta = tr.getMeta(key) as Meta | undefined;
				if (meta && 'close' in meta) return CLOSED;
				if (meta && 'open' in meta) return follow({ active: true, from: meta.open, query: '' }, state, cb.char);
				if (!prev.active) return prev;
				// Another device's edit reaches the editor as one step over the
				// whole document, which no mapped position survives. The binding
				// puts the caret back in the text it was in (a Yjs relative
				// position, as `anchor.ts` holds one), and an open trigger sits
				// the query and one character before the caret, so it is placed
				// again from there. It closes only if the edit took it away.
				const from = isChangeOrigin(tr) ? state.selection.head - prev.query.length - 1 : tr.mapping.map(prev.from);
				if (from < 0) return CLOSED;
				return follow({ ...prev, from }, state, cb.char);
			},
		},
		props: {
			// A keyboard sends the trigger alone; an input method, dictation or
			// a paste of keys can send it among other text (` /ta`), which opens
			// it too, where it lands.
			handleTextInput(view, from, to, text) {
				if (view.composing || from !== to) return false;
				const i = text.lastIndexOf(cb.char);
				if (i < 0 || /\s/.test(text.slice(i + 1))) return false;
				const parent = view.state.doc.resolve(from).parent;
				if (!parent.isTextblock || parent.type.spec.code) return false;
				const open = i > 0 ? /\s$/.test(text.slice(0, i)) : opensAt(view.state, from);
				if (!open) return false;
				const tr = view.state.tr.insertText(text, from, to).setMeta(key, { open: from + i });
				view.dispatch(tr);
				return true;
			},
			handleKeyDown(view, event) {
				if (event.key !== 'Escape' || !key.getState(view.state)?.active) return false;
				view.dispatch(view.state.tr.setMeta(key, { close: true }));
				return true;
			},
			handleDOMEvents: {
				compositionstart(view) {
					const { $from } = view.state.selection;
					if (!view.composing) composing.set(view, { start: $from.start(), offset: $from.parentOffset });
					return false;
				},
				compositionend(view) {
					const from = composing.get(view);
					composing.delete(view);
					if (!from) return false;
					// Once ProseMirror has read the composed text into the page,
					// as Tiptap's input rules wait for it.
					setTimeout(() => {
						if (view.isDestroyed || view.composing || key.getState(view.state)?.active) return;
						const open = composedTrigger(view.state, cb.char, from);
						if (open !== null) view.dispatch(view.state.tr.setMeta(key, { open }));
					});
					return false;
				},
			},
		},
		view() {
			let shown: TriggerState = CLOSED;
			return {
				update(view) {
					const now = key.getState(view.state) ?? CLOSED;
					if (now === shown) return;
					const was = shown;
					shown = now;
					if (!now.active) {
						if (was.active) cb.onClose();
						return;
					}
					if (!was.active) {
						// Coordinates are read after layout, where the trigger is by then.
						requestAnimationFrame(() => {
							const trigger = key.getState(view.state);
							if (view.isDestroyed || !trigger?.active) return;
							let at = { x: 0, y: 0 };
							try {
								const coords = view.coordsAtPos(trigger.from);
								at = { x: coords.left, y: coords.bottom };
							} catch {
								// No layout (a hidden pane): the menu opens at the corner.
							}
							cb.onOpen(at, trigger.from);
						});
					}
					if (now.query !== was.query) cb.onQuery(now.query);
				},
				destroy() {
					if (shown.active) cb.onClose();
				},
			};
		},
	});
}

/** The trigger's state in `state`, for the plugin made for `char`. */
export function triggerState(state: EditorState, char: string): TriggerState {
	return keyFor(char).getState(state) ?? CLOSED;
}

/**
 * What a pick from the menu `char` opened replaces. While the trigger is
 * open that is the trigger and the query typed after it (`@Da`), so neither
 * stays in the page beside what was picked: the menu opens a frame after
 * the trigger, and an input method or a paste can send both at once.
 * Otherwise it is `fallback`, where the menu opened as it is now (the host
 * anchors it, `anchor.ts`): the trigger alone when it is still there, or
 * nothing at its position.
 */
export function pickedRange(
	state: EditorState,
	char: string,
	fallback: { from: number; to: number },
): { from: number; to: number } {
	const typed = triggerState(state, char);
	if (typed.active) return { from: typed.from, to: typed.from + 1 + typed.query.length };
	const { from, to } = fallback;
	if (to > from && state.doc.textBetween(from, to) !== char) return { from, to: from };
	return { from, to };
}

/**
 * Close the trigger for `char`, putting `typed` back in the page after it:
 * what was written in a picker's own search box, which takes the keyboard
 * as the picker opens (the `@` picker), when it closes with nothing picked:
 * left there, Escape would throw away everything typed after the `@` but
 * the first keys. It takes the place of the query the page holds after the trigger
 * (`held` when the picker opened, which began its search); with the trigger
 * no longer open, it goes after the trigger where `fallback` says the menu
 * opened, as it is now. The caret ends after it.
 */
export function putBackQuery(
	view: EditorView,
	char: string,
	typed: string,
	held: string,
	fallback: { from: number; to: number },
): void {
	const { doc } = view.state;
	const open = triggerState(view.state, char);
	let range: { from: number; to: number } | null = null;
	if (open.active) {
		range = { from: open.from + 1, to: open.from + 1 + open.query.length };
	} else if (fallback.to > fallback.from && doc.textBetween(fallback.from, fallback.to) === char) {
		const after = doc.textBetween(fallback.to, Math.min(fallback.to + held.length, doc.content.size), undefined, '￼');
		range = { from: fallback.to, to: fallback.to + (held && after === held ? held.length : 0) };
	}
	const tr = view.state.tr.setMeta(keyFor(char), { close: true });
	if (range && typed && doc.textBetween(range.from, range.to, undefined, '￼') !== typed) {
		tr.insertText(typed, range.from, range.to);
		tr.setSelection(TextSelection.create(tr.doc, range.from + typed.length));
	}
	view.dispatch(tr);
}

/** Close the trigger for `char` without changing the text. */
export function closeTrigger(view: EditorView, char: string): void {
	if (!triggerState(view.state, char).active) return;
	view.dispatch(view.state.tr.setMeta(keyFor(char), { close: true }));
}

/** The triggers as one Tiptap extension. */
export function triggers(callbacks: TriggerCallbacks[]): Extension {
	return Extension.create({
		name: 'pageTriggers',
		addProseMirrorPlugins: () => callbacks.map(triggerPlugin),
	});
}
