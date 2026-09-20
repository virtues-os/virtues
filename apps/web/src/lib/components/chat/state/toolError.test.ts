import { describe, expect, it } from "vitest";
import { toolErrorDetail, toolErrorSummary } from "./toolError";

// Verbatim from a live box's journal, 2026-09-20: the shape sql_query's
// failure explainer produces, with the wrappers the executor adds on top.
const SQL_ERROR =
	'Execution failed: Query failed: column reference "occurred_at" is ambiguous\n' +
	"data_communication_message has: id, message_id, thread_id, channel, body, occurred_at\n" +
	"wiki_refs has: id, entity_type, entity_id, source_table, source_id, role, occurred_at\n" +
	"Rewrite the query using these column names.";

describe("toolErrorSummary", () => {
	it("is the first line with our wrappers stripped", () => {
		expect(toolErrorSummary(SQL_ERROR)).toBe(
			'column reference "occurred_at" is ambiguous',
		);
	});

	it("strips the executor's outer wrapper as well", () => {
		expect(toolErrorSummary("Tool execution failed: Execution failed: nope")).toBe("nope");
	});

	it("leaves a message that is not ours alone", () => {
		expect(toolErrorSummary("permission denied for table app_pages")).toBe(
			"permission denied for table app_pages",
		);
	});

	it("is empty for nothing, not a crash", () => {
		expect(toolErrorSummary(undefined)).toBe("");
		expect(toolErrorSummary("")).toBe("");
	});

	it("stays a line", () => {
		const long = "x".repeat(400);
		expect(toolErrorSummary(long).length).toBeLessThanOrEqual(160);
		expect(toolErrorSummary(long).endsWith("…")).toBe(true);
	});
});

describe("toolErrorDetail", () => {
	it("is the column list the model was handed", () => {
		const detail = toolErrorDetail(SQL_ERROR);
		expect(detail).toContain("data_communication_message has:");
		expect(detail).toContain("Rewrite the query");
		expect(detail?.startsWith("data_communication_message")).toBe(true);
	});

	it("is null when the error is one line", () => {
		expect(toolErrorDetail("permission denied for table app_pages")).toBeNull();
		expect(toolErrorDetail("one line\n\n  ")).toBeNull();
		expect(toolErrorDetail(undefined)).toBeNull();
	});
});
