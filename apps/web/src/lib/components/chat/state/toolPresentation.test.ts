import { describe, expect, it } from "vitest";
import registry from "$lib/ai/fixtures/tool-ids.json";
import { clip, describeTool, PRESENTED_TOOLS, siteIn, TOOL_UNFINISHED, toolStatus } from "./toolPresentation";

describe("toolPresentation", () => {
	it("has an entry for every tool the box registers, and none it does not", () => {
		expect([...PRESENTED_TOOLS].sort()).toEqual(registry);
	});

	it("cuts a long query for the header and keeps it whole for the list", () => {
		const part = {
			type: "tool-web_search",
			input: { query: "Example Air flight 123 March 3 2031 Lakeport departure time" },
		};
		expect(describeTool(part, true, true)).toBe(
			'Searching the web for "Example Air flight 123 March 3 2031…"',
		);
		expect(describeTool(part, false)).toBe(
			'Searched the web for "Example Air flight 123 March 3 2031 Lakeport departure time"',
		);
	});

	it("never leaves a short string clipped", () => {
		expect(clip("short")).toBe("short");
	});

	it("names the site a browsing task goes to, or none", () => {
		const browse = (task: string, live: boolean) => describeTool({ type: "tool-browse", input: { task } }, live);
		expect(browse("Open https://www.united.com/en/us/manageres/mytrips and read the trip.", true)).toBe(
			"Browsing united.com",
		);
		expect(browse("Go to expedia.com and find the hotel booking.", false)).toBe("Browsed expedia.com");
		expect(browse("Find the confirmation for the trip in October.", false)).toBe("Used your browser");
		expect(siteIn("Read news.ycombinator.com/news?p=2")).toBe("news.ycombinator.com");
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
		expect(
			toolStatus(
				at("output-error", "Tool failed (web_search, execution): stopped by the owner before it finished"),
				false,
			),
		).toBe("unfinished");
		expect(toolStatus(at("output-available"), true)).toBe("done");
	});
});
