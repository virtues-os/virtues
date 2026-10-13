import { describe, expect, it } from "vitest";
import { plainText, readHardBreaks, tokenize, unescapeMarkdown } from "./inlineMarkdown";

describe("tokenize", () => {
	it("reads links, the editor's ref form, and leaves a stray asterisk alone", () => {
		expect(tokenize("With [Nick](/person/p) and ![@Sam](/person/q) at 2*3")).toEqual([
			{ kind: "text", text: "With " },
			{ kind: "link", text: "Nick", url: "/person/p" },
			{ kind: "text", text: " and " },
			{ kind: "link", text: "Sam", url: "/person/q" },
			{ kind: "text", text: " at 2*3" },
		]);
	});

	it("reads a ref's name without the editor's `@`, in either of its forms", () => {
		// The block page exports `![@Sam](…)` as `[@Sam](…)`: the one mention.
		for (const md of ["With [@Sam](/person/q).", "With ![@Sam](/person/q)."]) {
			expect(tokenize(md)).toEqual([
				{ kind: "text", text: "With " },
				{ kind: "link", text: "Sam", url: "/person/q" },
				{ kind: "text", text: "." },
			]);
		}
		// A link to anything else keeps its label as written.
		expect(tokenize("[@home](/people/q)")).toEqual([{ kind: "link", text: "@home", url: "/people/q" }]);
	});

	it("keeps a URL with parentheses whole", () => {
		expect(tokenize("[the page](https://en.wikipedia.org/wiki/Mass_(liturgy))")).toEqual([
			{ kind: "link", text: "the page", url: "https://en.wikipedia.org/wiki/Mass_(liturgy)" },
		]);
	});

	it("nests a link inside emphasis, and reads code literally", () => {
		expect(tokenize("**[Nick](/person/p)** said `a*b*c`")).toEqual([
			{ kind: "strong", children: [{ kind: "link", text: "Nick", url: "/person/p" }] },
			{ kind: "text", text: " said " },
			{ kind: "code", text: "a*b*c" },
		]);
	});

	it("reads _em_ and ~~strike~~ but not a snake_case word", () => {
		expect(tokenize("_quiet_ ~~gone~~ snake_case_name")).toEqual([
			{ kind: "em", children: [{ kind: "text", text: "quiet" }] },
			{ kind: "text", text: " " },
			{ kind: "del", children: [{ kind: "text", text: "gone" }] },
			{ kind: "text", text: " snake_case_name" },
		]);
	});

	it("leaves an image that isn't a ref as its text", () => {
		expect(tokenize("![a photo](/media/x.jpg)")).toEqual([{ kind: "text", text: "![a photo](/media/x.jpg)" }]);
	});

	it("reads an escape as its character, never as markup", () => {
		expect(tokenize("AT\\&T and R\\&B, user\\_name, \\~20 people, \\*not em\\* and \\[not a link\\](/person/p)")).toEqual([
			{ kind: "text", text: "AT&T and R&B, user_name, ~20 people, *not em* and [not a link](/person/p)" },
		]);
	});

	it("reads a backslash ending a line as the hard line break it writes", () => {
		expect(tokenize("You cooked dinner.\\\nThen you read.")).toEqual([{ kind: "text", text: "You cooked dinner.\nThen you read." }]);
		expect(plainText("C:\\\\\nhome")).toBe("C:\\\nhome");
		expect(readHardBreaks("a\\\nb and c\\\\\nd")).toBe("a\nb and c\\\\\nd");
	});

	it("reads the character references the export writes", () => {
		expect(tokenize("&#32;   Four spaces, a &#124; pipe, a &#10214;bracket&#10215;, &amp; &lt;b&gt; &#x41;")).toEqual([
			{ kind: "text", text: "    Four spaces, a | pipe, a ⟦bracket⟧, & <b> A" },
		]);
		expect(tokenize("&bogus; stays")).toEqual([{ kind: "text", text: "&bogus; stays" }]);
	});

	it("reads escapes inside a link's words and address, and an address in angle brackets", () => {
		expect(tokenize("[R\\&B \\[live\\]](/place/p_1) and [the page](<https://example.com/a_(b)>)")).toEqual([
			{ kind: "link", text: "R&B [live]", url: "/place/p_1" },
			{ kind: "text", text: " and " },
			{ kind: "link", text: "the page", url: "https://example.com/a_(b)" },
		]);
	});

	it("never closes emphasis on an escaped delimiter, and reads escapes inside it", () => {
		expect(tokenize("**R\\&B** and *a\\* b*")).toEqual([
			{ kind: "strong", children: [{ kind: "text", text: "R&B" }] },
			{ kind: "text", text: " and " },
			{ kind: "em", children: [{ kind: "text", text: "a* b" }] },
		]);
	});

	it("reads code literally, a backslash included", () => {
		expect(tokenize("`a\\_b` and \\`not code\\`")).toEqual([
			{ kind: "code", text: "a\\_b" },
			{ kind: "text", text: " and `not code`" },
		]);
	});
});

describe("plainText and unescapeMarkdown", () => {
	it("say the same words for a writer's markdown and its export", () => {
		expect(plainText("Dinner at [⟦Ruth's⟧](/place/p_2) with R&B, ~20 people and user_name.")).toBe(
			plainText("Dinner at [⟦Ruth's⟧](/place/p_2) with R\\&B, \\~20 people and user\\_name."),
		);
		expect(unescapeMarkdown("a\\\\b \\# &#35; \\q")).toBe("a\\b # # \\q");
	});
});
