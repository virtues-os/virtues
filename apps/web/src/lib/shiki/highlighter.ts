/**
 * Shared Shiki highlighter singleton (no framework / no CodeMirror deps).
 *
 * Both the CodeMirror extension and the chat markdown code-block renderer import
 * this so there is exactly ONE Shiki instance and one set of lazily-loaded
 * languages/themes across the app.
 */

import {
	type BundledLanguage,
	type BundledTheme,
	createHighlighter,
	type Highlighter,
	type ThemedToken,
} from 'shiki';

let highlighter: Highlighter | null = null;
let loading = false;
const loadedLangs = new Set<string>();

export function getThemeFromCSS(): BundledTheme {
	if (typeof document === 'undefined') return 'github-light';
	const theme = getComputedStyle(document.documentElement)
		.getPropertyValue('--shiki-theme')
		.trim();
	return (theme || 'github-light') as BundledTheme;
}

export async function ensureHighlighter(): Promise<Highlighter> {
	if (highlighter) return highlighter;
	if (loading) {
		while (!highlighter) await new Promise((r) => setTimeout(r, 50));
		return highlighter;
	}
	loading = true;
	highlighter = await createHighlighter({
		themes: [getThemeFromCSS()],
		langs: [],
	});
	loading = false;
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
