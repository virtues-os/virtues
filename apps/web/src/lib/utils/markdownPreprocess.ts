/**
 * Normalizations applied to model output before it reaches Streamdown.
 *
 * One pass, one regex: code spans/fences and already-delimited math are matched
 * first so they pass through untouched — everything else is a rewrite.
 */

const PREPROCESS =
	/(```[\s\S]*?```|`[^`\n]+`)|(\$\$[\s\S]*?\$\$|\$[^\n$]+\$)|\\\[([\s\S]+?)\\\]|\\\(([\s\S]+?)\\\)|(?<!~)~(?!~)/g;

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
	return content.replace(PREPROCESS, (match, code, math, blockTex, inlineTex) => {
		if (code || math) return match;
		if (blockTex !== undefined) return `\n\n$$\n${blockTex.trim()}\n$$\n\n`;
		if (inlineTex !== undefined) return `$${inlineTex.trim()}$`;
		return '\\~';
	});
}
