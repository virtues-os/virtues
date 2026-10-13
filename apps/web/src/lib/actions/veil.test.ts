// @vitest-environment happy-dom
import { describe, expect, it } from "vitest";
import { parseDayArticle, veilMarks } from "$lib/wiki/dayArticle";
import { plainText } from "$lib/wiki/inlineMarkdown";
import { veiled } from "./veil";

/** The veiled spans the action makes over a paragraph drawn as the day page draws it. */
async function hiddenIn(markdown: string): Promise<string[]> {
	const block = parseDayArticle(`Lead.\n\n${markdown}`).blocks[0];
	const node = document.createElement("div");
	node.textContent = block.sentences.map((s) => plainText(veilMarks(s.markdown).markdown)).join(" ");
	document.body.appendChild(node);
	const action = veiled(node, { hiding: true, phrases: veilMarks(block.markdown).phrases });
	// It wraps once the markdown has rendered into the node.
	await new Promise<void>((done) => queueMicrotask(done));
	const spans = [...node.querySelectorAll<HTMLElement>("[data-veil]")].map((s) => s.textContent ?? "");
	action?.destroy?.();
	node.remove();
	return spans;
}

describe("veiled", () => {
	// A block page's export escapes what would read as markup, inside a veil
	// too (`⟦AT\&T⟧`). The page draws the words with their escapes read, so
	// the veil looks for each phrase that way: as written, it found none of
	// these, and with names hidden they showed.
	it("hides a phrase holding an escaped character, in the writer's markdown and in the export", async () => {
		const written = "You called ⟦AT&T⟧, then ⟦user_name_one⟧ about ⟦the [old] clinic⟧.[^ev-1]";
		const exported = "You called ⟦AT\\&T⟧, then ⟦user\\_name\\_one⟧ about ⟦the \\[old\\] clinic⟧.[^ev-1]";
		const want = ["AT&T", "user_name_one", "the [old] clinic"];
		expect(await hiddenIn(written)).toEqual(want);
		expect(await hiddenIn(exported)).toEqual(want);
	});
});
