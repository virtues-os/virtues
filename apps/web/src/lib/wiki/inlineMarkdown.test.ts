import { describe, expect, it } from "vitest";
import { tokenize } from "./inlineMarkdown";

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
});
