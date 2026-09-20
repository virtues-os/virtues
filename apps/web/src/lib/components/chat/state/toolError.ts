/**
 * What a failed tool call says to a person.
 *
 * The error text on a tool part was written for the MODEL. For sql_query it
 * reads `Execution failed: Query failed: column "day" does not exist`, then
 * the table's real column list, then "Rewrite the query using these column
 * names." That is the right thing to hand back to a model — on a live box
 * every failed query recovered on the very next call because of it — and the
 * wrong thing to print in a chat, where it ran to a paragraph of column names
 * in red, for a mistake the model had already fixed by the time anyone read it.
 *
 * So: the first line is the summary, everything after it is detail. The
 * wrapper prefixes are ours (`agent::executor` and `ToolError` stack two of
 * them on the way out) and say nothing a red mark does not already say.
 */

const PREFIXES = ["Tool execution failed:", "Execution failed:", "Query failed:"];
const MAX_SUMMARY = 160;

/** The first line, without our own wrappers, capped so it stays a line. */
export function toolErrorSummary(text: string | undefined | null): string {
	let line = (text ?? "").split("\n")[0].trim();
	let stripped = true;
	while (stripped) {
		stripped = false;
		for (const prefix of PREFIXES) {
			if (line.startsWith(prefix)) {
				line = line.slice(prefix.length).trim();
				stripped = true;
			}
		}
	}
	if (line.length > MAX_SUMMARY) line = line.slice(0, MAX_SUMMARY - 1) + "…";
	return line;
}

/** Everything after the first line, or null when there is nothing more. */
export function toolErrorDetail(text: string | undefined | null): string | null {
	const rest = (text ?? "").split("\n").slice(1).join("\n").trim();
	return rest.length > 0 ? rest : null;
}
