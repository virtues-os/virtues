/**
 * How well `name` matches `query`: 3 when the name starts with it, 2 when
 * every word of the query starts a word of the name, 1 when the name
 * contains it, 0 for no match.
 */
export function matchRank(name: string, query: string): number {
	const n = name.toLowerCase();
	const q = query.toLowerCase();
	if (n.startsWith(q)) return 3;
	const words = n.split(/[\s\-_/·:]+/);
	if (q.split(/\s+/).every((part) => words.some((w) => w.startsWith(part)))) return 2;
	return n.includes(q) ? 1 : 0;
}
