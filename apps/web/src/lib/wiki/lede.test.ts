import { describe, expect, it } from "vitest";
import { ledeSentence } from "./lede";
import { veilMarks } from "./dayArticle";

describe("ledeSentence", () => {
	it("keeps a link's label as a veil mark, once", () => {
		expect(ledeSentence("A walk with [Nick](/person/p) and [⟦David Okafor⟧](/person/q). Then home.")).toBe(
			"A walk with ⟦Nick⟧ and ⟦David Okafor⟧.",
		);
	});

	it("leaves nothing to show once veilMarks has run", () => {
		const v = veilMarks(ledeSentence("Sunday in ⟦Austin⟧ with [⟦Nick⟧](/person/p) was quiet.") ?? "");
		expect(v).toEqual({ markdown: "Sunday in Austin with Nick was quiet.", phrases: ["Austin", "Nick"] });
	});

	it("ends the sentence at a boundary a mark sits on", () => {
		expect(ledeSentence("You met [Nick](/person/p). ⟦Austin⟧ was hot.")).toBe("You met ⟦Nick⟧.");
		expect(ledeSentence("A long day. [Nick](/person/p) called.")).toBe("A long day.");
	});

	it("does not end the sentence inside a mark", () => {
		expect(ledeSentence("Mass at [St. Mary's](/place/p) ran long. Then lunch.")).toBe("Mass at ⟦St. Mary's⟧ ran long.");
		expect(ledeSentence("Mass at ⟦St. Mary's⟧, then lunch")).toBe("Mass at ⟦St. Mary's⟧, then lunch");
	});
});
