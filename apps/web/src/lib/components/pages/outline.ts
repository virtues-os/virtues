/**
 * The page outline: h1 to h3 headings, and how the table of contents scrolls
 * to them and spies on the scroll. Each editor supplies an `OutlineNav`
 * (`cmOutlineNav` for CodeMirror, `treeOutlineNav` for the block editor), so
 * `PageOutline` knows neither.
 */

export interface PageHeading {
	level: 1 | 2 | 3;
	text: string;
	/** The heading's position in its editor's document. */
	from: number;
	/** The heading's block id, on a block page. */
	id?: string;
}

export interface OutlineNav {
	/** The element that scrolls the page. */
	scroller: HTMLElement;
	/**
	 * Where the heading starts, in the scroller's scroll coordinates
	 * (comparable to `scroller.scrollTop`), or null when it cannot be found.
	 */
	topOf(h: PageHeading): number | null;
	scrollTo(h: PageHeading): void;
}

/** Whether two headings are the same entry of the outline. */
export function sameHeading(a: PageHeading, b: PageHeading): boolean {
	return a.id !== undefined || b.id !== undefined ? a.id === b.id : a.from === b.from;
}
