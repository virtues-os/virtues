import { describe, expect, it } from "vitest";
import registry from "$lib/ai/fixtures/tool-ids.json";
import { clip, describeTool, PRESENTED_TOOLS, TOOL_UNFINISHED, toolStatus } from "./toolPresentation";

describe("toolPresentation", () => {
	it("has an entry for every tool the box registers, and none it does not", () => {
		expect([...PRESENTED_TOOLS].sort()).toEqual(registry);
	});

	it("cuts a long query for the header and keeps it whole for the list", () => {
		const part = {
			type: "tool-web_search",
			input: { query: "Southwest flight 4469 October 2 2026 Austin departure time" },
		};
		expect(describeTool(part, true, true)).toBe(
			'Searching the web for "Southwest flight 4469 October 2 2026…"',
		);
		expect(describeTool(part, false)).toBe(
			'Searched the web for "Southwest flight 4469 October 2 2026 Austin departure time"',
		);
	});

	it("never leaves a short string clipped", () => {
		expect(clip("short")).toBe("short");
	});
});

describe("toolStatus", () => {
	const at = (state?: string, errorText?: string) => ({ type: "tool-x", state, errorText });

	it("is running only while the turn is", () => {
		for (const s of ["input-streaming", "input-available", undefined]) {
			expect(toolStatus(at(s), true)).toBe("running");
			expect(toolStatus(at(s), false)).toBe("unfinished");
		}
	});

	it("tells a call that never answered from one that failed", () => {
		expect(toolStatus(at("output-error", TOOL_UNFINISHED), false)).toBe("unfinished");
		expect(toolStatus(at("output-error", "column does not exist"), false)).toBe("failed");
		expect(toolStatus(at("output-available"), true)).toBe("done");
	});
});
