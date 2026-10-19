/**
 * Whether plain text is code. Code copied as plain text (a chat's Copy
 * code, a terminal, an editor without HTML on its clipboard) is pasted as
 * written: a Python, shell, YAML or Dockerfile comment starts with `# `,
 * which reads as a markdown heading, `__init__` reads as bold to markdown
 * and to the editor's paste rules alike (`paste.ts`, `schema.ts`), a diff's
 * removed and added lines read as bullets of one kind, and YAML's indents
 * are lost.
 *
 * Markdown holds code too, which is not the text being code: a fence at any
 * indent (one nested in a list item, or under item 10 or later, is indented
 * four spaces or more), code indented inside a list item, and front matter
 * at the top, are the markdown's own.
 */

/** A line a block's sign starts, which no code check reads. */
const SIGNED_LINE = /^\s*(?:[-*+>|]\s|\d+[.)]\s|#{1,6}\s)/;

/** A fence's line, at any indent. */
const FENCE = /^\s*(?:```|~~~)/;

/** Front matter's first and last line. */
const FRONT_MATTER = /^---\s*$/;

/** A list item's line: its indent, and its sign with the spaces after it. */
const LIST_ITEM = /^( *)([-*+]|\d{1,9}[.)])( +)\S/;

/**
 * Lines that read as code, whatever else the line says: a definition, an
 * import or a shebang, a shell prompt or variable, a statement ending in
 * `;`, a line opening a block with `{`, a line of closing brackets. Python
 * is read further below: its names in two underscores (`DUNDER`) and its
 * blocks, which open with a colon.
 */
const CODE_LINES = [
	/^\s*(?:async\s+)?(?:def|function|fn|func|class|struct|interface|enum|impl|pub\s+fn)\s+[\w$]+.*[(:{<]/,
	/^\s*(?:import\s+[\w.{*"']|from\s+[\w.]+\s+import\s|#include\s*[<"]|#!\/)/,
	/^\s*(?:\$|>>>|%)\s+\S/,
	/^\s*(?:export\s+)?[A-Z_][A-Z0-9_]*=\S/,
	/[=(].*;\s*$/,
	/\{\s*$/,
	/^\s*[}\])]+[;,]?\s*$/,
	// A variable opening a line, set or used (`$env:PATH = …`, `${HOME}/bin`).
	/^\s*\$(?:[A-Za-z_][\w:]*|\{\w+\})\s*[=.[({/]/,
	// A call alone on its line (`library(ggplot2)`, `main()`).
	/^\s*[A-Za-z_][\w.]*\([^()]*\)\s*;?\s*$/,
	// An ignore file's glob or folder (`*.log`, `**/dist`, `node_modules/`).
	/^\s*!?[\w./-]*(?:\*\.|\/\*|\*\/)[\w.*/-]*\s*$/,
	/^\s*!?\/?[\w.-]+(?:\/[\w.-]+)*\/\s*$/,
];

/**
 * A C preprocessor's line: `#define MAX 10`, `#endif`. A space after its
 * `#` makes it a heading's sign to markdown, so it is read before the signs.
 */
const DIRECTIVE = /^\s*#\s*(?:define|undef|ifdef|ifndef)\s+[A-Z_][A-Z0-9_]*\b|^\s*#(?:if|ifdef|ifndef|elif|else|endif|pragma|define|undef)\b/;

/**
 * Lines that read as code unless they end as a sentence does: a command
 * (`git push`), an assignment (`total = 3`). Prose opens with those words
 * too ("git keeps every version, so nothing is lost.", "result = good, so
 * ship it."), and ends with a word's full stop, which a command or an
 * assignment does not.
 */
const CODE_UNLESS_PROSE = [
	/^\s*(?:sudo|pip3?|npm|npx|pnpm|yarn|cargo|rustup|brew|apt(?:-get)?|dnf|yum|pacman|git|curl|wget|docker|kubectl|helm|terraform|tar|rsync|rm|ssh|scp|cd|ls|cp|mv|mkdir|rmdir|chmod|chown|ln|grep|sed|awk|xargs|python3?|pkill|systemctl|journalctl|launchctl|psql|ffmpeg|cmake|gcc|bash|zsh|vim|nano)\s+\S/,
	// A command whose name is also a word, followed by what only code has: a
	// flag, a path, a variable, a pipe or a redirect.
	/^\s*(?:make|find|cat|echo|source|kill|open|touch|ping|head|tail|less|node|go|java|ruby|perl|env|which|code|deno|bun|watch)\s+(?:.*\s)?(?:-{1,2}[A-Za-z]|[~.]{0,2}\/[\w.-]|\$[A-Za-z_{]|[|>]|&&)/,
	/^\s*(?:self\.|this\.)?[a-z_][\w.]*\s*(?:=|\+=|-=|:=|<-)\s*\S/,
	// A Dockerfile's instruction.
	/^\s*(?:FROM|RUN|CMD|COPY|ADD|ENV|ARG|WORKDIR|EXPOSE|ENTRYPOINT|USER|VOLUME|LABEL|HEALTHCHECK|SHELL|ONBUILD|STOPSIGNAL)\s+\S/,
	// SQL's statements, in its capitals.
	/^\s*(?:SELECT\s.*\bFROM\b|INSERT\s+INTO\s|UPDATE\s+\S+\s+SET\s|DELETE\s+FROM\s|(?:CREATE|ALTER|DROP)\s+(?:TABLE|INDEX|VIEW|UNIQUE\s+INDEX)\s)/,
	// PowerShell's commands, a verb it names and a noun (`Get-ChildItem`).
	/^\s*(?:Get|Set|New|Remove|Add|Start|Stop|Invoke|Import|Export|Write|Read|Test|Select|Where|ForEach|Out|Format|Install|Update|Enable|Disable|Restart|Copy|Move|Rename|Clear|Convert|ConvertTo|ConvertFrom|Join|Split|Measure|Sort|Group|Register|Wait|Push|Pop|Resolve|Expand|Compress)-[A-Z][A-Za-z]+\b/,
];

/** A line ending as a sentence does: a letter, then `.`, `!` or `?`. */
const SENTENCE_END = /\p{L}[.!?]\s*$/u;

/**
 * A Python name with two underscores either side, where code puts one: after
 * a `.` (`obj.__dict__`), called (`__init__(`), compared (`__name__ ==`), or
 * quoted (`"__main__"`). Markdown and the paste rules read `__init__` as
 * bold, and prose writing `__bold__` puts it after a space and before one.
 * Read with the line's inline code left out, which is the markdown's own.
 */
const DUNDER = /\.__[A-Za-z]\w*__|\b__[A-Za-z]\w*__\s*(?:\(|[=!<>]=)|(["'])__[A-Za-z]\w*__\1/;

/** Inline code on a line: the markdown's own, whatever it holds. */
const INLINE_CODE = /`[^`\n]*`/g;

/** A Python block's own line, which needs nothing under it: `else:`, `try:`, `finally:`, `except:`. */
const PYTHON_BARE = /^\s*(?:else|try|finally|except)\s*:\s*$/;

/**
 * A Python statement opening a block (`if x:`, `for a in b:`, `with f() as g:`),
 * code when the line after it is indented under it, as no list item is: a
 * sentence opening with "if" or "for" and ending in a colon is not followed
 * by one.
 */
const PYTHON_OPENER = /^\s*(?:if|elif|while|for|with|except|async\s+(?:for|with))\b.*:\s*$/;

/**
 * A YAML key opening a block (`jobs:`), code when a key is indented under
 * it (`  build:`), as no prose's line is. A list under a label
 * (`Shopping:` then `  - milk`) is markdown's.
 */
const YAML_OPENER = /^\s*[\w.-]+:\s*$/;
const YAML_KEY = /^\s*[\w.-]+:(?:\s|$)/;

/**
 * A unified diff's header: a hunk's (`@@ -1,3 +1,3 @@`), a file's
 * (`--- a/x`, `+++ b/x`), `diff --git` or `index`.
 */
const DIFF_HEADER = /^(?:@@ -\d+(?:,\d+)? \+\d+(?:,\d+)? @@|diff --git |index [0-9a-f]{7,}\.\.[0-9a-f]{7,}|(?:---|\+\+\+) (?:a\/|b\/|\/dev\/null))/;

/**
 * Whether `lines` are a diff: a header of one, or, without, lines removed
 * (`-`), added (`+`) and kept (one space before the line) together, which
 * markdown reads as bullets of one kind and a paragraph.
 */
function readsAsDiff(lines: string[]): boolean {
	if (lines.some((l) => DIFF_HEADER.test(l))) return true;
	return lines.some((l) => /^-(?!--)/.test(l)) && lines.some((l) => /^\+(?!\+\+)/.test(l)) && lines.some((l) => /^ \S/.test(l));
}

/** A blockquote's sign, and what it quotes: a terminal's `> npm start` is a prompt, not a quote. */
const QUOTED = /^\s*>\s?(.*)$/;

/** Whether `line` reads as a command or code by the rules that let prose through when it ends as a sentence. */
function commandLike(line: string): boolean {
	return CODE_UNLESS_PROSE.some((re) => re.test(line)) && !SENTENCE_END.test(line);
}

/** How far `line` is indented, a tab counted as four spaces. */
function indentOf(line: string): number {
	let n = 0;
	for (const ch of line) {
		if (ch === ' ') n += 1;
		else if (ch === '\t') n += 4 - (n % 4);
		else break;
	}
	return n;
}

/**
 * Whether a line of `text` reads as code: outside a fence, not starting
 * with a block's sign, and not indented as code inside a list item (four
 * spaces past where the item's text starts), which markdown reads as the
 * item's own code block.
 */
export function readsAsCode(text: string): boolean {
	let fenced = false;
	// Where the text of the list item the lines are in starts.
	let item: number | null = null;
	const lines = text.split(/\r?\n/);
	if (readsAsDiff(lines)) return true;
	// Front matter at the top is the markdown's own.
	let front = FRONT_MATTER.test(lines[0] ?? '') && lines.slice(1).some((l) => FRONT_MATTER.test(l));
	for (const [i, line] of lines.entries()) {
		if (front) {
			if (i > 0 && FRONT_MATTER.test(line)) front = false;
			continue;
		}
		if (FENCE.test(line)) {
			fenced = !fenced;
			continue;
		}
		if (fenced || !line.trim()) continue;
		if (DIRECTIVE.test(line)) return true;
		const quoted = QUOTED.exec(line);
		if (quoted && commandLike(quoted[1])) return true;
		const indent = indentOf(line);
		if (item !== null && indent < item) item = null;
		const sign = LIST_ITEM.exec(line);
		if (sign && indent < (item ?? 0) + 4) {
			item = indent + sign[2].length + sign[3].length;
			continue;
		}
		if (item !== null && indent >= item + 4) continue;
		if (SIGNED_LINE.test(line)) continue;
		if (CODE_LINES.some((re) => re.test(line))) return true;
		if (commandLike(line)) return true;
		if (DUNDER.test(line.replace(INLINE_CODE, '')) || PYTHON_BARE.test(line)) return true;
		const next = lines.slice(i + 1).find((l) => l.trim());
		if (next !== undefined && indentOf(next) > indent) {
			if (PYTHON_OPENER.test(line) && !SIGNED_LINE.test(next)) return true;
			if (YAML_OPENER.test(line) && YAML_KEY.test(next) && !SIGNED_LINE.test(next)) return true;
		}
	}
	return false;
}
