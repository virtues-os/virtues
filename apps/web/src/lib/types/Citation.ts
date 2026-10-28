/**
 * What a chat answer can cite: the records, queries and web pages its own
 * turn's tools returned. A link in the answer to one of them is a citation,
 * and opens the RecordCard.
 */

export interface CitationContext {
	/**
	 * Each query a tool ran this turn (`sql_query`, `show`), by call id: the
	 * id in a figure's link, `/chat/<chat>/tool/<call>`.
	 */
	queries: Map<string, QueryResult>;

	/**
	 * The records this turn's tools returned, by the route an answer links to.
	 * A link to one of these routes is a citation: the box only lets the model
	 * link a record a tool showed it.
	 */
	byRoute: Map<string, RecordSource>;

	/** The pages a web search returned, by address (`webKey`). */
	byUrl: Map<string, WebSource>;
}

/** One query's result, as the turn's stored tool output holds it. */
export interface QueryResult {
	/** The SELECT's columns, in its order. A row's `ref` is not one of them. */
	columns: string[];
	rows: Record<string, unknown>[];
	/** Rows the query returned. More than `rows.length` when the result was too large to carry. */
	count: number;
	/** The query returned rows past these: `count` of them, or an unknown number when the tool only says it cut them. */
	more: boolean;
	/** The SQL your server ran. */
	sql?: string;
}

/** One record a tool returned in this turn, as the answer's citation card reads it. */
export interface RecordSource {
	/** `/record/<ontology>/<id>`, exactly as the tool returned it. */
	route: string;
	ontology: string;
	recordId: string;
	title?: string;
	/** The passage the search matched: the text a citation rests on. */
	passage?: string;
	timestamp?: string;
	/** Order within its search, 1 first. */
	rank?: number;
	toolCallId: string;
}

/** One page a web search returned in this turn. */
export interface WebSource {
	url: string;
	title: string;
	/** The page's text as the search returned it. */
	text?: string;
	published?: string;
	toolCallId: string;
}
