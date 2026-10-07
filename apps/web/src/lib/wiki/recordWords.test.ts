import { describe, expect, it } from "vitest";
import { messageBody, messageSender, nearTurns } from "./recordWords";

const CHUNK =
	"[Speaker 1]: Let's get going. [Speaker 2]: How was Mass? [Speaker 1]: It was great, all in Latin. [Speaker 2]: Yeah. [Speaker 1]: We walked around after. [Speaker 2]: Okay.";

describe("nearTurns", () => {
	it("centers the window on the turn sharing the sentence's words", () => {
		const n = nearTurns(CHUNK, "Mass was called great, all in Latin.");
		expect(n.matched).toBe(true);
		expect(n.turns.map((t) => t.text)).toEqual(["How was Mass?", "It was great, all in Latin.", "Yeah.", "We walked around after."]);
		expect(n.turns.filter((t) => t.near).map((t) => t.text)).toEqual(["It was great, all in Latin."]);
	});

	it("shows the opening turns, unmarked, when nothing overlaps", () => {
		const n = nearTurns(CHUNK, "The flight moved to 5:52.");
		expect(n.matched).toBe(false);
		expect(n.turns[0].text).toBe("Let's get going.");
		expect(n.turns.some((t) => t.near)).toBe(false);
	});
});

describe("messages", () => {
	it("names the sender and the group", () => {
		expect(messageSender({ metadata: { is_from_me: true } })).toBe("You");
		expect(messageSender({ metadata: { is_from_me: false, group_title: "Family" }, from_name: "Nick", is_group_message: true })).toBe("Nick, in Family");
		expect(messageSender({ metadata: {} })).toBe("Someone");
	});

	it("trims a long body and drops attachment placeholders", () => {
		expect(messageBody({ body: "Look￼ here" })).toBe("Look here");
		expect(messageBody({ body: "x".repeat(500) }).length).toBe(421);
	});
});
