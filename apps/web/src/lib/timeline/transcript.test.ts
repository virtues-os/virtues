import { describe, expect, it } from "vitest";
import { convLines, lineAt, lineTime, parseLine, rowLines, spkIdx } from "./transcript";

describe("a transcript", () => {
	it("reads a tagged window line by line and an untagged blob sentence by sentence", () => {
		expect(convLines("[Speaker 1]: Hi.\n\n[Speaker 2]: Hello there.")).toEqual(["[Speaker 1]: Hi.", "[Speaker 2]: Hello there."]);
		expect(convLines("One thing. Another? Last")).toEqual(["One thing.", "Another?", "Last"]);
		expect(convLines(null)).toEqual([]);
	});

	it("turns the recorder's speaker numbers into letters", () => {
		expect(parseLine("[Speaker 2]: yes")).toEqual({ spk: "B", txt: "yes" });
		expect(parseLine("no tag here")).toEqual({ spk: "", txt: "no tag here" });
	});

	it("shifts each later window's letters by two, so no seam claims the same person", () => {
		const lines = rowLines([{ text: "[Speaker 1]: a\n[Speaker 2]: b" }, { text: "[Speaker 1]: c" }]);
		expect(lines.map((l) => l.spk + l.txt)).toEqual(["Aa", "Bb", "Cc"]);
	});

	it("tints a letter by its place and spreads lines evenly over the conversation", () => {
		expect([spkIdx("A"), spkIdx("B"), spkIdx("E")]).toEqual([0, 1, 0]);
		expect(lineTime(0, 100, 0, 4)).toBe(13);
		expect(lineAt(0, 100, 4, 60)).toBe(2);
	});
});
