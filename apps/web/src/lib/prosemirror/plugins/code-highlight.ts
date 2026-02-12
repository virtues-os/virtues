/**
 * Code Block Syntax Highlighting Plugin
 *
 * Uses prosemirror-highlight + Shiki to add syntax coloring to code blocks
 * via ProseMirror inline decorations. Uses Shiki's css-variables theme
 * so colors come from our existing --shiki-token-* / --code-* CSS variables.
 *
 * Highlighter and languages are loaded lazily on first encounter.
 */

import { createHighlightPlugin } from 'prosemirror-highlight';
import { createParser, type Parser } from 'prosemirror-highlight/shiki';

// eslint-disable-next-line @typescript-eslint/no-explicit-any
let highlighter: any = null;
let shikiParser: Parser | null = null;
let initPromise: Promise<void> | null = null;

const lazyParser: Parser = (options) => {
	const { language } = options;

	// First call: dynamically import Shiki and create highlighter
	if (!highlighter) {
		if (!initPromise) {
			initPromise = Promise.all([
				import('shiki'),
				import('@shikijs/core'),
			]).then(([{ createHighlighter }, { createCssVariablesTheme }]) => {
				// Create a theme that outputs var(--shiki-token-*) CSS variables
				// These are mapped to --code-* in app.css, themed per-theme in themes.css
				const cssVarsTheme = createCssVariablesTheme({
					name: 'css-variables',
					variablePrefix: '--shiki-',
				});

				return createHighlighter({
					themes: [cssVarsTheme],
					langs: [],
				});
			}).then((h) => {
				highlighter = h;
				shikiParser = createParser(h);
			});
		}
		return initPromise;
	}

	// Load language on demand
	const lang = language || '';
	if (lang && !highlighter.getLoadedLanguages().includes(lang)) {
		return highlighter.loadLanguage(lang).catch(() => {
			// Unknown language — fall back to plain text
		});
	}

	// Parse with Shiki
	return shikiParser!(options);
};

export function createCodeHighlightPlugin() {
	return createHighlightPlugin({ parser: lazyParser });
}
