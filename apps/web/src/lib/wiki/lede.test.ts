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
		const v = veilMarks(ledeSentence("Sunday in ⟦Lakeport⟧ with [⟦Nick⟧](/person/p) was quiet.") ?? "");
		expect(v).toEqual({ markdown: "Sunday in Lakeport with Nick was quiet.", phrases: ["Lakeport", "Nick"] });
	});

	it("ends the sentence at a boundary a mark sits on", () => {
		expect(ledeSentence("You met [Nick](/person/p). ⟦Lakeport⟧ was hot.")).toBe("You met ⟦Nick⟧.");
		expect(ledeSentence("A long day. [Nick](/person/p) called.")).toBe("A long day.");
	});

	it("does not end the sentence inside a mark", () => {
		expect(ledeSentence("The tour of [Mt. Hesper Lighthouse](/place/p) ran long. Then lunch.")).toBe("The tour of ⟦Mt. Hesper Lighthouse⟧ ran long.");
		expect(ledeSentence("The tour of ⟦Mt. Hesper Lighthouse⟧, then lunch")).toBe("The tour of ⟦Mt. Hesper Lighthouse⟧, then lunch");
	});

	it("reads an escape as its character, as a block page's export writes it", () => {
		expect(ledeSentence("Dinner at [⟦Marlowe\\'s⟧](/place/p) with Q\\&A, \\~20 people and user\\_name. Then home.")).toBe(
			"Dinner at ⟦Marlowe's⟧ with Q&A, ~20 people and user_name.",
		);
		expect(ledeSentence("Dinner at [⟦Marlowe's⟧](/place/p) with Q&A, ~20 people and user_name. Then home.")).toBe(
			"Dinner at ⟦Marlowe's⟧ with Q&A, ~20 people and user_name.",
		);
	});
});
