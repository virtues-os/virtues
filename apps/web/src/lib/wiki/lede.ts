/**
 * THE LEDE — an article's opening paragraph, which is the short form every
 * rung above it reads.
 *
 * The first block that is neither blank nor a markdown heading. Every wiki
 * brief requires an article to open with a lede carrying no heading; the skip
 * exists because a human edit that adds a heading above it must not turn that
 * heading into the summary.
 *
 * **One rule, three languages.** The server has it twice — `wiki_editor::lede`
 * for callers holding the text and `wiki::lede_sql` for list queries that must
 * not fetch whole articles, checked against each other by the
 * `lede_and_lede_sql_agree` test. This is the third, and it lives here as one
 * function because the two hand-written copies it replaces had already
 * drifted: the chronicle's version took the text before the first heading and
 * then the first non-empty block of it, which returns `# A heading` as the
 * lede whenever an article opens with one.
 */
export function lede(article: string | null | undefined): string | null {
	if (!article) return null;
	return (
		article
			.split(/\n\s*\n/)
			.map((b) => b.trim())
			.find((b) => b !== '' && !b.startsWith('#')) ?? null
	);
}

/**
 * The lede's first sentence, as plain text — for a row that is one line and
 * lets CSS clip whatever is left.
 *
 * Markdown links keep their label, emphasis marks are dropped. A sentence ends
 * at a terminal mark followed by whitespace and a capital, so "Toys \"R\" Us."
 * and "St. Mary" do not cut it early.
 */
export function ledeSentence(article: string | null | undefined): string | null {
	const paragraph = lede(article);
	if (!paragraph) return null;
	const plain = paragraph
		.replace(/\[([^\]]*)\]\([^)]*\)/g, '$1')
		.replace(/[*_`]/g, '')
		.replace(/\s+/g, ' ')
		.trim();
	const m = plain.match(/^[\s\S]*?[.!?]["')]?(?=\s+[A-Z])/);
	return (m ? m[0] : plain) || null;
}
