/**
 * Page outline — h1–h3 headings extracted from the markdown, with their
 * document positions so the table-of-contents can scroll the editor to them.
 */

export interface PageHeading {
	level: 1 | 2 | 3;
	text: string;
	/** Document char offset of the heading line start (== CodeMirror position). */
	from: number;
}

const HEADING_RE = /^(#{1,3})\s+(.+?)\s*$/;

/**
 * Extract h1–h3 headings with their document positions. Fenced code blocks are
 * skipped so a `# comment` inside code isn't mistaken for a heading.
 */
export function extractHeadings(content: string): PageHeading[] {
	const out: PageHeading[] = [];
	let pos = 0;
	let inFence = false;
	for (const line of content.split('\n')) {
		const trimmed = line.trimStart();
		if (trimmed.startsWith('```') || trimmed.startsWith('~~~')) {
			inFence = !inFence;
		} else if (!inFence) {
			const m = HEADING_RE.exec(line);
			if (m) out.push({ level: m[1].length as 1 | 2 | 3, text: m[2], from: pos });
		}
		pos += line.length + 1; // +1 for the newline
	}
	return out;
}
