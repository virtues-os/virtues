/**
 * The dry run's check of the day page's readers against a copy of a real
 * box: for every day page the run wrote out (`virtues pages convert
 * --dry-run --articles --texts <dir>`, one JSON file a page holding its
 * markdown, its export and its notes' anchors), the page must read the same
 * both ways and every note must land beside the same sentence.
 *
 * Skipped unless `DAY_ARTICLE_TEXTS` names that folder. The pages are
 * someone's life, so what fails is said by file and by kind of reading,
 * never by quoting a page.
 */

import fs from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";
import type { NoteAnchor } from "./dayNotes";
import { anchorsOn, readingDifferences } from "./test-utils";

const DIR = process.env.DAY_ARTICLE_TEXTS;

interface Texts {
	/** The page's markdown, as its writer and its owner left it. */
	source: string;
	/** What the page's blocks export, which the readers read once it moves. */
	export: string;
	/** The anchors of the notes written on the page. */
	anchors: NoteAnchor[];
}

describe.skipIf(!DIR)("the day page's readers on a copy of a box", () => {
	it("read every page's export as its markdown, and land every note the same", () => {
		const files = fs.readdirSync(DIR as string).filter((f) => f.endsWith(".json"));
		const differences: string[] = [];
		let anchors = 0;
		for (const file of files) {
			const texts: Texts = JSON.parse(fs.readFileSync(path.join(DIR as string, file), "utf8"));
			anchors += texts.anchors.length;
			const all = [...texts.anchors, ...anchorsOn(texts.source)];
			for (const d of readingDifferences(texts.source, texts.export, all)) differences.push(`${file}: ${d}`);
		}
		console.info(`${files.length} day pages, ${anchors} anchored notes, ${differences.length} differences`);
		expect(files.length).toBeGreaterThan(0);
		expect(differences).toEqual([]);
	});
});
