/**
 * Code block highlighting: Shiki's tokens drawn as decorations over the
 * block's text, never written. A block is tokenized 150 ms after it last
 * changed, through the app's one Shiki instance (`$lib/shiki/highlighter`),
 * and the result is kept by (theme, language, text) so an unchanged block
 * is never tokenized twice in one theme. A token's colour is the theme's,
 * so a change of theme (`data-theme` on the page) tokenizes every block
 * again in the new one.
 */

import { Extension } from '@tiptap/core';
import type { Node as PmNode } from '@tiptap/pm/model';
import { Plugin, PluginKey } from '@tiptap/pm/state';
import { Decoration, DecorationSet, type EditorView } from '@tiptap/pm/view';
import type { ThemedToken } from 'shiki';
import { getThemeFromCSS, highlightCode } from '$lib/shiki/highlighter';

const DEBOUNCE_MS = 150;
/** Kept results: enough for every block on a long page, small enough to forget the rest. */
const CACHE_LIMIT = 400;

type Tokens = ThemedToken[][] | null;
export type Highlighter = (code: string, lang: string) => Promise<Tokens>;

const cache = new Map<string, Tokens>();
const cacheKey = (theme: string, lang: string, text: string) => `${theme}\u0000${lang}\u0000${text}`;

function remember(key: string, tokens: Tokens): void {
	if (cache.size >= CACHE_LIMIT) cache.delete(cache.keys().next().value as string);
	cache.set(key, tokens);
}

const codeKey = new PluginKey<DecorationSet>('codeHighlight');

/** The decorations for one block's tokens, the block's text starting at `start`. */
export function tokenDecorations(tokens: ThemedToken[][], start: number): Decoration[] {
	const out: Decoration[] = [];
	let pos = start;
	tokens.forEach((line, i) => {
		for (const token of line) {
			const end = pos + token.content.length;
			if (token.color && end > pos) out.push(Decoration.inline(pos, end, { style: `color: ${token.color}` }));
			pos = end;
		}
		if (i < tokens.length - 1) pos += 1; // the newline between lines
	});
	return out;
}

/** Every code block with a language: its position, language and text. */
function codeBlocks(doc: PmNode): { pos: number; lang: string; text: string }[] {
	const out: { pos: number; lang: string; text: string }[] = [];
	doc.descendants((node, pos) => {
		if (node.type.name !== 'codeBlock') return !node.isTextblock;
		const lang = typeof node.attrs.language === 'string' ? node.attrs.language : '';
		if (lang) out.push({ pos, lang, text: node.textContent });
		return false;
	});
	return out;
}

/** Decorations from what the cache holds for `theme`; blocks it lacks are listed. */
function fromCache(doc: PmNode, theme: string): { set: DecorationSet; missing: { lang: string; text: string }[] } {
	const decos: Decoration[] = [];
	const missing: { lang: string; text: string }[] = [];
	for (const block of codeBlocks(doc)) {
		const key = cacheKey(theme, block.lang, block.text);
		if (!cache.has(key)) {
			missing.push(block);
			continue;
		}
		const tokens = cache.get(key);
		if (tokens) decos.push(...tokenDecorations(tokens, block.pos + 1));
	}
	return { set: DecorationSet.create(doc, decos), missing };
}

/**
 * `highlight` tokenizes in the page's theme now; `theme` names it, so what is
 * kept for one theme is never drawn in another.
 */
export function codeHighlight(highlight: Highlighter = highlightCode, theme: () => string = getThemeFromCSS): Extension {
	return Extension.create({
		name: 'pageCodeHighlight',
		addProseMirrorPlugins() {
			return [
				new Plugin<DecorationSet>({
					key: codeKey,
					state: {
						init: (_config, state) => fromCache(state.doc, theme()).set,
						apply(tr, prev, _old, state) {
							if (tr.getMeta(codeKey)) return fromCache(state.doc, theme()).set;
							return tr.docChanged ? prev.map(tr.mapping, tr.doc) : prev;
						},
					},
					props: {
						decorations: (state) => codeKey.getState(state),
					},
					view(view: EditorView) {
						let timer: ReturnType<typeof setTimeout> | null = null;
						let destroyed = false;
						let drawn = theme();
						const run = async () => {
							timer = null;
							const now = theme();
							const { missing } = fromCache(view.state.doc, now);
							await Promise.all(
								missing.map(async ({ lang, text }) => {
									remember(cacheKey(now, lang, text), await highlight(text, lang));
								}),
							);
							// Redrawn even when every block was cached: a remote change
							// replaces the whole document, which maps every decoration away.
							drawn = now;
							if (!destroyed && !view.isDestroyed) view.dispatch(view.state.tr.setMeta(codeKey, true));
						};
						const schedule = () => {
							if (timer) clearTimeout(timer);
							timer = setTimeout(run, DEBOUNCE_MS);
						};
						schedule();
						// The theme is chosen on the page's root element.
						const themes =
							typeof MutationObserver === 'undefined'
								? null
								: new MutationObserver(() => {
										if (theme() !== drawn) schedule();
									});
						themes?.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme', 'class', 'style'] });
						return {
							update(v, prev) {
								if (!v.state.doc.eq(prev.doc)) schedule();
							},
							destroy() {
								destroyed = true;
								themes?.disconnect();
								if (timer) clearTimeout(timer);
							},
						};
					},
				}),
			];
		},
	});
}
