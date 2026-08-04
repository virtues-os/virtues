/**
 * Normalizations applied to model output before it reaches Streamdown.
 *
 * One pass, one regex: code spans/fences and already-delimited math are matched
 * first so they pass through untouched — everything else is a rewrite.
 */

const PREPROCESS =
	/(```[\s\S]*?```|`[^`\n]+`)|(\$\$[\s\S]*?\$\$|\$[^\n$]+\$)|\\\[([\s\S]+?)\\\]|\\\(([\s\S]+?)\\\)|(?<!~)~(?!~)/g;

/**
 * Legacy `entity:` links, rewritten to the one route syntax we have.
 *
 * The page tools used to teach the model `[Name](entity:page_abc)`. Nothing
 * ever parsed that scheme, and Streamdown blocks any href that isn't http(s)
 * or an absolute path — it renders the link as dead text with a `[blocked]`
 * suffix, and does so *before* our own `link` snippet runs, so the snippet
 * cannot rescue it. The tool descriptions now teach `/page/page_abc`, but
 * messages already in the database still carry the old form; rewrite on read.
 */
const LEGACY_ENTITY_LINK = /\]\(entity:([a-z]+)_([A-Za-z0-9_-]+)\)/g;

/**
 * Streamdown only recognizes `$…$` / `$$…$$` for KaTeX, but models routinely
 * emit the LaTeX-native `\(…\)` / `\[…\]` instead — markdown then eats the
 * backslash and renders the raw TeX as prose. Rewrite them to dollars.
 *
 * Also neutralizes a lone `~`, which the model uses for "approximately"
 * ("~10k stars", "~5ms"); Streamdown reads `~text~` as subscript, so an
 * unintended pair drops a whole phrase below the baseline. Escaping keeps a
 * literal tilde while leaving `~~strikethrough~~` intact — but not inside code,
 * where a backslash is literal (e.g. `rm ~/.cache`).
 */
export function preprocessMarkdown(content: string): string {
	if (!content) return '';
	content = content.replace(LEGACY_ENTITY_LINK, (_m, type, rest) => `](/${type}/${type}_${rest})`);
	return content.replace(PREPROCESS, (match, code, math, blockTex, inlineTex) => {
		if (code || math) return match;
		if (blockTex !== undefined) return `\n\n$$\n${blockTex.trim()}\n$$\n\n`;
		if (inlineTex !== undefined) return `$${inlineTex.trim()}$`;
		return '\\~';
	});
}
