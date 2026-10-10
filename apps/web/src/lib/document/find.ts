/**
 * Find and replace in page: matches drawn as decorations, never written. ⌘F
 * opens the bar (the host's `onOpen`), Enter and ⌘G go to the next match,
 * Shift-Enter and ⌘⇧G to the previous, Escape closes. As the CodeMirror
 * editor's search panel: case ignored unless asked, a pattern read as a
 * regular expression or a whole word when asked, and Replace (the current
 * match, then on to the next) and Replace all (every match, in one
 * transaction, so one undo step). A replacement takes the marks of the text
 * it replaces; a match holding a widget (a mention, an image) is never
 * replaced, since its text cannot say what the widget was. A match never
 * spans two blocks, as a reader would not expect it to.
 *
 * Selected words are echoed where else they occur, quietly, as the
 * CodeMirror editor's `highlightSelectionMatches` does: the same text, case
 * and all, for a selection inside one block of up to 200 characters that is
 * not only spaces, and nothing past 100 others.
 */

import { Extension } from '@tiptap/core';
import type { Node as PmNode } from '@tiptap/pm/model';
import { Plugin, PluginKey, TextSelection, type EditorState } from '@tiptap/pm/state';
import { Decoration, DecorationSet, type EditorView } from '@tiptap/pm/view';

export interface FindMatch {
	from: number;
	to: number;
}

export interface FindOptions {
	caseSensitive: boolean;
	regexp: boolean;
	wholeWord: boolean;
}

export const NO_FIND_OPTIONS: FindOptions = { caseSensitive: false, regexp: false, wholeWord: false };

export interface FindState {
	query: string;
	options: FindOptions;
	matches: FindMatch[];
	/** Index into `matches` of the match gone to, or -1. */
	current: number;
	/** The query is a regular expression that does not read. */
	invalid: boolean;
	decorations: DecorationSet;
}

type FindMeta = { query: string; options?: FindOptions } | { step: 1 | -1 } | { at: number } | { clear: true };

export const findKey = new PluginKey<FindState>('find');

const EMPTY: FindState = {
	query: '',
	options: NO_FIND_OPTIONS,
	matches: [],
	current: -1,
	invalid: false,
	decorations: DecorationSet.empty,
};

/** What an inline widget reads as in a block's text: one character, so positions line up. */
const WIDGET = '\uFFFC';

/** The pattern a query and its options make, or null when it does not read. */
export function findPattern(query: string, options: FindOptions = NO_FIND_OPTIONS): RegExp | null {
	if (!query) return null;
	let source = options.regexp ? query : query.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
	if (options.wholeWord) source = `(?<![\\p{L}\\p{N}_])(?:${source})(?![\\p{L}\\p{N}_])`;
	try {
		return new RegExp(source, `gu${options.caseSensitive ? '' : 'i'}`);
	} catch {
		return null;
	}
}

/** Every match of `query` in the page's text, in order. */
export function findMatches(doc: PmNode, query: string, options: FindOptions = NO_FIND_OPTIONS): FindMatch[] {
	const pattern = findPattern(query, options);
	if (!pattern) return [];
	const out: FindMatch[] = [];
	doc.descendants((node, pos) => {
		if (!node.isTextblock) return true;
		// One character per position: an inline widget reads as one.
		const text = node.textBetween(0, node.content.size, undefined, WIDGET);
		pattern.lastIndex = 0;
		for (let m = pattern.exec(text); m; m = pattern.exec(text)) {
			// A pattern that matches nothing would match here forever.
			if (!m[0].length) {
				pattern.lastIndex += 1;
				continue;
			}
			out.push({ from: pos + 1 + m.index, to: pos + 1 + m.index + m[0].length });
		}
		return false;
	});
	return out;
}

function decorate(doc: PmNode, matches: FindMatch[], current: number): DecorationSet {
	return DecorationSet.create(
		doc,
		matches.map((m, i) =>
			Decoration.inline(m.from, m.to, {
				class: i === current ? 'doc-find-match doc-find-current' : 'doc-find-match',
			}),
		),
	);
}

/** The match nearest after `pos`, for a fresh query: the reader stays where they were. */
function firstAfter(matches: FindMatch[], pos: number): number {
	if (!matches.length) return -1;
	const i = matches.findIndex((m) => m.from >= pos);
	return i < 0 ? 0 : i;
}

function rebuild(state: EditorState, prev: FindState, current: number): FindState {
	const matches = findMatches(state.doc, prev.query, prev.options);
	const at = matches.length ? Math.min(Math.max(current, 0), matches.length - 1) : -1;
	return { ...prev, matches, current: at, decorations: decorate(state.doc, matches, at) };
}

export function findPlugin(): Plugin<FindState> {
	return new Plugin<FindState>({
		key: findKey,
		state: {
			init: () => EMPTY,
			apply(tr, prev, _old, state) {
				const meta = tr.getMeta(findKey) as FindMeta | undefined;
				if (meta && 'clear' in meta) return EMPTY;
				if (meta && 'query' in meta) {
					const options = meta.options ?? prev.options;
					const matches = findMatches(state.doc, meta.query, options);
					const current = firstAfter(matches, state.selection.from);
					const invalid = !!meta.query && findPattern(meta.query, options) === null;
					return { query: meta.query, options, matches, current, invalid, decorations: decorate(state.doc, matches, current) };
				}
				if (meta && 'at' in meta) {
					if (meta.at < 0 || meta.at >= prev.matches.length) return prev;
					return { ...prev, current: meta.at, decorations: decorate(state.doc, prev.matches, meta.at) };
				}
				if (meta && 'step' in meta) {
					if (!prev.matches.length) return prev;
					const n = prev.matches.length;
					const current = (((prev.current + meta.step) % n) + n) % n;
					return { ...prev, current, decorations: decorate(state.doc, prev.matches, current) };
				}
				if (!prev.query || !tr.docChanged) return prev;
				return rebuild(state, prev, prev.current);
			},
		},
		props: {
			decorations: (state) => findKey.getState(state)?.decorations,
		},
	});
}

const echoKey = new PluginKey<DecorationSet>('selectionEcho');

/** The longest selection echoed, and the most places it is echoed at. */
const ECHO_MAX_LENGTH = 200;
const ECHO_MAX_MATCHES = 100;

/** Where else the selected text occurs; none for a selection that is not some words in one block, or too common. */
export function selectionEchoes(state: EditorState): FindMatch[] {
	const { selection } = state;
	if (!(selection instanceof TextSelection) || selection.empty || !selection.$from.sameParent(selection.$to)) return [];
	const { from, to } = selection;
	const text = state.doc.textBetween(from, to, undefined, WIDGET);
	if (to - from > ECHO_MAX_LENGTH || !text.trim() || text.includes(WIDGET)) return [];
	const pattern = findPattern(text, { caseSensitive: true, regexp: false, wholeWord: false });
	if (!pattern) return [];
	const out: FindMatch[] = [];
	let over = false;
	state.doc.descendants((node, pos) => {
		if (over) return false;
		if (!node.isTextblock) return true;
		const line = node.textBetween(0, node.content.size, undefined, WIDGET);
		pattern.lastIndex = 0;
		for (let m = pattern.exec(line); m; m = pattern.exec(line)) {
			const match = { from: pos + 1 + m.index, to: pos + 1 + m.index + m[0].length };
			if (match.to <= from || match.from >= to) out.push(match);
			if (out.length > ECHO_MAX_MATCHES) over = true;
		}
		return false;
	});
	return over ? [] : out;
}

function echoPlugin(): Plugin<DecorationSet> {
	return new Plugin<DecorationSet>({
		key: echoKey,
		state: {
			init: () => DecorationSet.empty,
			apply(tr, prev, old, state) {
				if (!tr.docChanged && old.selection.eq(state.selection)) return prev;
				const echoes = selectionEchoes(state);
				return echoes.length
					? DecorationSet.create(state.doc, echoes.map((m) => Decoration.inline(m.from, m.to, { class: 'doc-selection-match' })))
					: DecorationSet.empty;
			},
		},
		props: {
			decorations: (state) => echoKey.getState(state),
		},
	});
}

export function findState(state: EditorState): FindState {
	return findKey.getState(state) ?? EMPTY;
}

/**
 * Select and show the current match. The bar holds the focus, and the editor
 * scrolls to its selection only while it has it, so the match is scrolled to
 * here.
 */
function reveal(view: EditorView): void {
	const { matches, current } = findState(view.state);
	const m = matches[current];
	if (!m) return;
	view.dispatch(
		view.state.tr.setSelection(TextSelection.create(view.state.doc, m.from, m.to)).scrollIntoView(),
	);
	try {
		const { node } = view.domAtPos(m.from);
		const el = node.nodeType === 1 ? (node as Element) : node.parentElement;
		const box = el?.getBoundingClientRect();
		if (el && box && (box.top < 0 || box.bottom > window.innerHeight)) {
			el.scrollIntoView({ block: 'center' });
		}
	} catch {
		// No layout to scroll in.
	}
}

export function setFindQuery(view: EditorView, query: string, options?: FindOptions): void {
	view.dispatch(view.state.tr.setMeta(findKey, { query, options }));
	reveal(view);
}

/** Whether a match holds only text, so a replacement can stand for all of it. */
function replaceable(doc: PmNode, m: FindMatch): boolean {
	let text = true;
	doc.nodesBetween(m.from, m.to, (node) => {
		if (node.isInline && !node.isText) text = false;
		return text;
	});
	return text;
}

/** What replaces `m`: `replacement`, with `$1` and `$&` read from the match when the query is a pattern. */
function replacementFor(state: EditorState, m: FindMatch, replacement: string): string {
	const s = findState(state);
	if (!s.options.regexp) return replacement;
	const pattern = findPattern(s.query, s.options);
	if (!pattern) return replacement;
	const text = state.doc.textBetween(m.from, m.to, undefined, WIDGET);
	return text.replace(new RegExp(pattern.source, pattern.flags.replace('g', '')), replacement);
}

/**
 * Replace the current match with `replacement`, keeping its marks, and go
 * to the next. Returns whether one was replaced.
 */
export function replaceCurrent(view: EditorView, replacement: string): boolean {
	const s = findState(view.state);
	const m = s.matches[s.current];
	if (!m || !view.editable) return false;
	if (!replaceable(view.state.doc, m)) {
		findNext(view);
		return false;
	}
	const text = replacementFor(view.state, m, replacement);
	const tr = view.state.tr;
	if (text) tr.insertText(text, m.from, m.to);
	else tr.delete(m.from, m.to);
	tr.setSelection(TextSelection.create(tr.doc, tr.mapping.map(m.to)));
	view.dispatch(tr);
	// The matches were found again in the edited page: go to the one after
	// the replacement.
	const after = findState(view.state).matches;
	const next = after.findIndex((n) => n.from >= tr.mapping.map(m.to));
	if (after.length) goTo(view, next < 0 ? 0 : next);
	return true;
}

/** Replace every match with `replacement`, in one transaction. Returns how many were replaced. */
export function replaceAll(view: EditorView, replacement: string): number {
	const s = findState(view.state);
	if (!view.editable || !s.matches.length) return 0;
	const tr = view.state.tr;
	let count = 0;
	// Last to first, so each match's position still holds when it is reached.
	for (const m of [...s.matches].reverse()) {
		if (!replaceable(view.state.doc, m)) continue;
		const text = replacementFor(view.state, m, replacement);
		if (text) tr.insertText(text, m.from, m.to);
		else tr.delete(m.from, m.to);
		count++;
	}
	if (count) view.dispatch(tr);
	return count;
}

/** Make match `index` the current one and show it. */
function goTo(view: EditorView, index: number): void {
	const s = findState(view.state);
	if (index < 0 || index >= s.matches.length) return;
	view.dispatch(view.state.tr.setMeta(findKey, { at: index }));
	reveal(view);
}

export function findNext(view: EditorView): void {
	view.dispatch(view.state.tr.setMeta(findKey, { step: 1 }));
	reveal(view);
}

export function findPrevious(view: EditorView): void {
	view.dispatch(view.state.tr.setMeta(findKey, { step: -1 }));
	reveal(view);
}

export function clearFind(view: EditorView): void {
	if (!findState(view.state).query) return;
	view.dispatch(view.state.tr.setMeta(findKey, { clear: true }));
}

/** Find, with its keys, and the selection's echo: `onOpen` shows the bar with the selected text as the query. */
export function find(o: { onOpen: (selected: string) => void; onClose: () => void }): Extension {
	return Extension.create({
		name: 'pageFind',
		addProseMirrorPlugins: () => [findPlugin(), echoPlugin()],
		addKeyboardShortcuts() {
			return {
				'Mod-f': ({ editor }) => {
					const { from, to } = editor.state.selection;
					o.onOpen(from === to ? '' : editor.state.doc.textBetween(from, to, ' '));
					return true;
				},
				'Mod-g': ({ editor }) => {
					if (!findState(editor.state).query) return false;
					findNext(editor.view);
					return true;
				},
				'Shift-Mod-g': ({ editor }) => {
					if (!findState(editor.state).query) return false;
					findPrevious(editor.view);
					return true;
				},
				Escape: ({ editor }) => {
					if (!findState(editor.state).query) return false;
					clearFind(editor.view);
					o.onClose();
					return true;
				},
			};
		},
	});
}
