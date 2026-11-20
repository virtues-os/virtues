import { describe, expect, it } from "vitest";
import { speakerTurns } from "./transcript";

describe("a transcript's turns", () => {
	it("splits at every speaker tag, on its own line or inline, and drops the tags", () => {
		expect(speakerTurns("[Speaker 1]: Is the kettle on?\n[Speaker 2]: Just boiled.")).toEqual(["Is the kettle on?", "Just boiled."]);
		expect(speakerTurns("[Speaker]: Two sugars. [Speaker 2]: None for me.")).toEqual(["Two sugars.", "None for me."]);
	});

	it("reads untagged words as one turn, and nothing as none", () => {
		expect(speakerTurns("The tea went cold.")).toEqual(["The tea went cold."]);
		expect(speakerTurns(null)).toEqual([]);
	});
});
