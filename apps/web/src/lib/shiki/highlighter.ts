/**
 * Shared Shiki highlighter singleton (no framework / no CodeMirror deps).
 *
 * The page editor's code blocks and the chat markdown code-block renderer import
 * this so there is exactly ONE Shiki instance and one set of lazily-loaded
 * languages/themes across the app.
 */

import type { BundledLanguage, BundledTheme, Highlighter, ThemedToken } from 'shiki';

// Shiki loads on the first code block, not with the chat: a static import put
// its core and language map in the home screen's first load.
let highlighter: Promise<Highlighter> | null = null;
const loadedLangs = new Set<string>();

export function getThemeFromCSS(): BundledTheme {
	if (typeof document === 'undefined') return 'github-light';
	const theme = getComputedStyle(document.documentElement)
		.getPropertyValue('--shiki-theme')
		.trim();
	return (theme || 'github-light') as BundledTheme;
}

export function ensureHighlighter(): Promise<Highlighter> {
	// The JavaScript regex engine, not Shiki's default Oniguruma: that one is
	// a 622 KB WebAssembly chunk whose instance held 24 MB of memory from the
	// first code block on, for the life of the page.
	highlighter ??= Promise.all([import('shiki'), import('shiki/engine/javascript')])
		.then(([{ createHighlighter }, { createJavaScriptRegexEngine }]) =>
			createHighlighter({ themes: [getThemeFromCSS()], langs: [], engine: createJavaScriptRegexEngine() })
		)
		.catch((err) => {
			// Let the next code block try again instead of every caller waiting forever.
			highlighter = null;
			throw err;
		});
	return highlighter;
}

/**
 * Tokenize `code` for `lang` with the active theme. Returns null for unsupported
 * languages or any failure (caller falls back to plain text).
 */
export async function highlightCode(code: string, lang: string): Promise<ThemedToken[][] | null> {
	try {
		const h = await ensureHighlighter();
		const theme = getThemeFromCSS();

		if (!h.getLoadedThemes().includes(theme)) {
			await h.loadTheme(theme);
		}

		if (!loadedLangs.has(lang)) {
			try {
				await h.loadLanguage(lang as BundledLanguage);
				loadedLangs.add(lang);
			} catch {
				return null; // Unsupported language
			}
		}

		return h.codeToTokens(code, { lang: lang as BundledLanguage, theme }).tokens;
	} catch {
		return null;
	}
}
