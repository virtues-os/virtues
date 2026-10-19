/**
 * An article becomes a block page once opened, and from then on the day
 * page reads the page's export (`app_pages.content`) where it read its
 * writer's markdown. The export writes the same page another way: escapes
 * where text would read as markup, definitions in the order their markers
 * come, table rules and list markers as the export spells them. Every entry
 * of the crate's article corpus (`tests/corpus/articles.json`, fictional
 * pages in the writers' shapes, with the export the crate writes for each)
 * must read the same both ways: the same blocks, sentences, sources, veiled
 * phrases and figure fields, and a note lands beside the same sentence.
 *
 * Except a page the move leaves in markdown, whose export no reader ever
 * reads: one whose source note runs onto a second line, which the day page
 * reads as a paragraph of its own and the export folds into the note.
 */

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { anchorsOn, landings, reading } from "./test-utils";

// Not `new URL(…, import.meta.url)`: under happy-dom `URL` is the DOM's, which
// fileURLToPath refuses.
const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../../../..");
const CORPUS = path.join(ROOT, "crates/virtues-document/tests/corpus/articles.json");

interface Entry {
	name: string;
	kind: "day" | "year" | "entity" | "story" | "identity";
	markdown: string;
	export: string;
	/** The export is the markdown, byte for byte (the crate's own test holds it). */
	writer: boolean;
}

const entries: Entry[] = JSON.parse(fs.readFileSync(CORPUS, "utf8"));

const DEFINITION = /^\[\^[^\]]+\]:/;
/** A source note running onto the line under it: the move holds such a page back. */
const heldBack = (md: string) =>
	md.split("\n").some((line, i, all) => DEFINITION.test(line) && (all[i + 1] ?? "").trim() !== "" && !DEFINITION.test(all[i + 1]));

describe("an article's export reads as its writer's markdown", () => {
	it("has day pages to read", () => {
		expect(entries.filter((e) => e.kind === "day").length).toBeGreaterThan(0);
	});

	it("holds back the pages whose notes run on, and reads every other", () => {
		expect(entries.filter((e) => heldBack(e.markdown)).every((e) => !e.writer)).toBe(true);
		expect(entries.filter((e) => !heldBack(e.markdown)).length).toBeGreaterThan(entries.length / 2);
	});

	for (const e of entries.filter((e) => !heldBack(e.markdown))) {
		it(`${e.name}: the same blocks, sentences, sources, veils and figures`, () => {
			expect(reading(e.export)).toEqual(reading(e.markdown));
		});

		it(`${e.name}: a note lands beside the same sentence`, () => {
			const anchors = [...anchorsOn(e.markdown), { quote: "A sentence nobody wrote." }];
			expect(landings(e.export, anchors)).toEqual(landings(e.markdown, anchors));
		});
	}
});
