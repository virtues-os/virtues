import { describe, expect, it } from "vitest";
import { splitTurn, stopReason, toUiMessage, turnMovedPast } from "./transcript";

const text = (t: string) => ({ type: "text", text: t });
const tool = (name: string) => ({ type: `tool-${name}`, toolCallId: name });
const reasoning = (t: string) => ({ type: "reasoning", text: t });

describe("splitTurn", () => {
	it("keeps a plain reply in the body with nothing to think about", () => {
		const t = splitTurn([text("Hello")], false);
		expect(t.bodyFromIndex).toBe(0);
		expect(t.narration).toEqual([]);
		expect(t.hasThinkingContent).toBe(false);
	});

	it("moves the text before a tool call to narration, and the text after it is the reply", () => {
		const t = splitTurn([text(" Let me look. "), tool("sql_query"), text("Found it.")], false);
		expect(t.bodyFromIndex).toBe(2);
		expect(t.narration).toEqual(["Let me look."]);
		expect(t.toolParts).toHaveLength(1);
		expect(t.hasThinkingContent).toBe(true);
	});

	it("lets the last thing said stand as the reply when a finished turn ended on a tool call", () => {
		const parts = [text("First"), tool("a"), text("Checking"), tool("b")];
		const t = splitTurn(parts, false);
		expect(t.bodyFromIndex).toBe(2);
		expect(t.narration).toEqual(["First"]);
	});

	it("does not promote narration to a reply while the turn is still streaming", () => {
		const parts = [text("First"), tool("a"), text("Checking"), tool("b")];
		const t = splitTurn(parts, true);
		expect(t.bodyFromIndex).toBe(4);
		expect(t.narration).toEqual(["First", "Checking"]);
	});

	it("treats a line with no tool after it as the reply mid-stream", () => {
		const t = splitTurn([tool("a"), text("Answer so far")], true);
		expect(t.bodyFromIndex).toBe(1);
		expect(t.narration).toEqual([]);
	});

	it("ignores whitespace-only text when finding the reply", () => {
		const t = splitTurn([text("Said"), tool("a"), text("   ")], false);
		expect(t.bodyFromIndex).toBe(0);
		expect(t.narration).toEqual([]);
	});

	it("has no body index when a finished turn wrote no text at all", () => {
		const t = splitTurn([tool("a")], false);
		expect(t.bodyFromIndex).toBe(-1);
		expect(t.hasThinkingContent).toBe(true);
	});

	it("joins non-empty reasoning with newlines", () => {
		const t = splitTurn([reasoning("a"), reasoning(""), reasoning("b"), text("x")], false);
		expect(t.reasoning).toBe("a\nb");
		expect(t.hasThinkingContent).toBe(true);
	});

	it("starts an empty streaming turn at index 0 with nothing to show", () => {
		const t = splitTurn([], true);
		expect(t.bodyFromIndex).toBe(0);
		expect(t.reasoning).toBe("");
		expect(t.hasThinkingContent).toBe(false);
	});
});

describe("turnMovedPast", () => {
	it("is true when another tool call follows", () => {
		expect(turnMovedPast([tool("a"), tool("b")], 0)).toBe(true);
	});

	it("is true when reply text follows", () => {
		expect(turnMovedPast([tool("a"), text("ok")], 0)).toBe(true);
	});

	it("is false when the turn ended on the call, or only blank text followed", () => {
		expect(turnMovedPast([text("x"), tool("a")], 1)).toBe(false);
		expect(turnMovedPast([tool("a"), text("  "), reasoning("r")], 0)).toBe(false);
	});
});

describe("stopReason", () => {
	it("is null for a whole reply", () => {
		expect(stopReason(undefined)).toBeNull();
		expect(stopReason({ agentId: "general" })).toBeNull();
	});

	it("names the flag, the person's own stop first", () => {
		expect(stopReason({ cutShort: true })).toBe("length");
		expect(stopReason({ maxSteps: true })).toBe("max_steps");
		expect(stopReason({ budget: true, stopped: true })).toBe("stopped");
		expect(stopReason({ unattended: true, interrupted: true })).toBe("interrupted");
		expect(stopReason({ unavailable: true })).toBe("unavailable");
	});
});

describe("toUiMessage", () => {
	it("gives a reloaded tool part its own type, whichever spelling the box sent", () => {
		const msg = toUiMessage(
			{
				id: "m1",
				role: "assistant",
				parts: [
					{ type: "tool-invocation", toolName: "create_page", toolCallId: "c0", state: "output-available" },
					{ type: "tool-web_search", toolName: "web_search", toolCallId: "c1", state: "output-available" },
				],
			},
			new Map(),
		);
		expect(msg.parts.map((p: any) => p.type)).toEqual(["tool-create_page", "tool-web_search"]);
	});
});

describe("splitTurn intent", () => {
	const t = (s: string) => ({ type: "text", text: s });
	const call = (n: string) => ({ type: `tool-${n}`, toolCallId: n });

	it("is the line that introduced the call in flight", () => {
		const turn = splitTurn([t("Checking your messages."), call("sql_query")], true);
		expect(turn.intent).toBe("Checking your messages.");
	});

	it("goes quiet once the model makes a call without a word", () => {
		const turn = splitTurn(
			[t("Checking your messages."), call("sql_query"), call("web_search")],
			true,
		);
		expect(turn.intent).toBe("");
		expect(turn.narration).toEqual(["Checking your messages."]);
	});
});
