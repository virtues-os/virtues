/**
 * The page outline: h1 to h3 headings, and how the headings rail
 * (`Rail.svelte`, in PageContent) and Contents in ⋯ scroll to them and spy
 * on the scroll. The editor supplies an `OutlineNav` (`treeOutlineNav`), so
 * neither knows an editor.
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
