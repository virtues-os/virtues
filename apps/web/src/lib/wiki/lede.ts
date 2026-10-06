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
 * The lede's first sentence, with its markdown dropped — for a row that is
 * one line and lets CSS clip whatever is left.
 *
 * Emphasis marks are dropped. A link keeps its label, wrapped in the veil's
 * ⟦ ⟧ marks: every lede link names a person, place or organization, which the
 * day page veils as a link, and a plain-text row has no link left to veil. So
 * the result can carry marks, and a caller showing it runs `veilMarks` over it.
 *
 * A sentence ends at a terminal mark followed by whitespace and a capital,
 * outside any ⟦ ⟧ mark, so "Toys \"R\" Us." and a linked "St. Mary's" do not
 * cut it early. An unlinked, unmarked "St. Mary" still does.
 */
export function ledeSentence(article: string | null | undefined): string | null {
	const paragraph = lede(article);
	if (!paragraph) return null;
	const plain = paragraph
		.replace(/\[([^\]]*)\]\([^)]*\)/g, (_, label: string) => `⟦${label.replace(/[⟦⟧]/g, '')}⟧`)
		.replace(/[*_`]/g, '')
		.replace(/\s+/g, ' ')
		.trim();
	for (const end of plain.matchAll(/[.!?]["')⟧]?(?=\s+⟦?[A-Z])/g)) {
		const head = plain.slice(0, end.index + end[0].length);
		if (head.split('⟦').length === head.split('⟧').length) return head;
	}
	return plain || null;
}
