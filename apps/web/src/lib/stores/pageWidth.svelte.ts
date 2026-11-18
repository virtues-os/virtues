/**
 * How wide a page's content is: Reading, Medium or Full.
 *
 * Each view declares the width that suits it with `usePageWidth` (or `Page`'s
 * `maxWidth`). You can change it from the width button in the page bar
 * (tabs/ActionToolbar.svelte). Your choice is kept per kind of page, so
 * widening one day page widens every day page and leaves the wiki alone.
 * It is saved on this device only, since the right width depends on the
 * screen.
 *
 * The widths themselves are the `--page-*` tokens in themes.css.
 *
 * TODO(2026-11-18): migrate DetailFrame.svelte (tasks, threads, notes). It
 * still caps its column itself, so the page bar offers no width there.
 */

import { getContext, setContext, untrack } from "svelte";

export type PageWidth = "reading" | "medium" | "full";
/** `sheet` is the pages editor's A4 sheet, offered only where a view asks for it. */
export type PageWidthChoice = PageWidth | "sheet";

export const PAGE_WIDTHS: { value: PageWidthChoice; label: string }[] = [
	{ value: "reading", label: "Reading" },
	{ value: "medium", label: "Medium" },
	{ value: "full", label: "Full" },
	{ value: "sheet", label: "A4 sheet" },
];

/** The CSS width of each choice, for `max-width`. */
export const PAGE_WIDTH_CSS: Record<PageWidthChoice, string> = {
	reading: "var(--page-reading)",
	medium: "var(--page-medium)",
	full: "var(--page-full)",
	sheet: "210mm",
};

const STORAGE_KEY = "virtues-page-width";
/** The pages editor's own width setting, which this replaces. */
const EDITOR_KEY = "virtues-page-display";
/** The kind `kindOf` gives an open page in the editor. */
const EDITOR_KIND = "page:one";

const CHOICES = new Set<string>(PAGE_WIDTHS.map((w) => w.value));

function load(): Record<string, PageWidthChoice> {
	try {
		const saved = localStorage.getItem(STORAGE_KEY);
		if (saved) {
			const parsed = JSON.parse(saved) as Record<string, string>;
			return Object.fromEntries(
				Object.entries(parsed).filter(([, v]) => CHOICES.has(v)),
			) as Record<string, PageWidthChoice>;
		}
		// Carry over a width picked in the editor before it moved here.
		// Its narrow and medium widths are both the reading column now.
		const editor = JSON.parse(localStorage.getItem(EDITOR_KEY) ?? "{}") as { widthMode?: string };
		if (editor.widthMode === "full") return { [EDITOR_KIND]: "full" };
		if (editor.widthMode === "page") return { [EDITOR_KIND]: "sheet" };
	} catch {
		// Storage unavailable: every page uses its own width.
	}
	return {};
}

interface Declared {
	kind: string;
	initial: PageWidth;
	sheet: boolean;
	token: symbol;
}

class PageWidthStore {
	#chosen = $state<Record<string, PageWidthChoice>>(load());
	#byTab = $state<Record<string, Declared>>({});

	/** What a tab's view declared, if it has a width at all. */
	declaredFor(tabId: string | null | undefined): Omit<Declared, "token"> | undefined {
		return tabId ? this.#byTab[tabId] : undefined;
	}

	/** Your choice for a kind of page, if you made one. */
	chosen(kind: string): PageWidthChoice | undefined {
		return this.#chosen[kind];
	}

	/** The width a kind of page shows at. */
	widthFor(kind: string | undefined, initial: PageWidth, sheet = false): PageWidthChoice {
		const c = kind ? this.#chosen[kind] : undefined;
		if (!c || (c === "sheet" && !sheet)) return initial;
		return c;
	}

	/** Choose a width for a kind of page; its own width clears the choice. */
	choose(kind: string, width: PageWidthChoice, initial: PageWidth): void {
		if (width === initial) delete this.#chosen[kind];
		else this.#chosen[kind] = width;
		try {
			localStorage.setItem(STORAGE_KEY, JSON.stringify(this.#chosen));
		} catch {
			// The choice lasts until the app reloads.
		}
	}

	declare(tabId: string, d: Omit<Declared, "token">): () => void {
		const token = Symbol();
		untrack(() => {
			this.#byTab[tabId] = { ...d, token };
		});
		return () => {
			untrack(() => {
				if (this.#byTab[tabId]?.token === token) delete this.#byTab[tabId];
			});
		};
	}
}

export const pageWidth = new PageWidthStore();

const PAGE_KIND = Symbol("page-kind");

/** TabContent names the tab and its kind of page, for `usePageWidth`. */
export function providePageKind(ctx: { tabId: () => string; kind: () => string }): void {
	setContext(PAGE_KIND, ctx);
}

/**
 * The kind of page a tab shows: its type, the system page it is, and whether
 * it is one thing or a list. `/day/day_2026-01-01` and `/day/day_2026-01-02`
 * are one kind; `/day` is another.
 */
export function kindOf(tab: { type: string; virtuesPage?: string }, isOne: boolean): string {
	return `${tab.type}${tab.virtuesPage ? `/${tab.virtuesPage}` : ""}${isOne ? ":one" : ""}`;
}

/**
 * Give a view its width. Call it while the component initialises, with the
 * width the view is designed for, or `null` for a view without a page
 * column (a map, a chat). The result is the width to draw at: your choice
 * for this kind of page, or the view's own.
 *
 * `sheet: true` offers the A4 sheet as a fourth width. `part` names the
 * section of a view that draws several under one tab (the wiki room's
 * days, people and years), so each keeps its own choice. `reading` is the
 * view's own reading column, for a room whose rows need more than the
 * reading measure (Settings, `--page-settings`).
 */
export function usePageWidth(
	initial: () => PageWidth | null,
	opts: { sheet?: boolean; part?: () => string; reading?: string } = {},
): { readonly width: PageWidthChoice; readonly css: string } {
	const ctx = getContext<{ tabId: () => string; kind: () => string } | undefined>(PAGE_KIND);
	const sheet = !!opts.sheet;
	const kind = $derived.by(() => {
		if (!ctx) return undefined;
		const part = opts.part?.();
		return part ? `${ctx.kind()}/${part}` : ctx.kind();
	});
	$effect(() => {
		const w = initial();
		if (!ctx || !kind || !w) return;
		return pageWidth.declare(ctx.tabId(), { kind, initial: w, sheet });
	});
	const width = $derived.by(() => {
		const w = initial() ?? "full";
		return pageWidth.widthFor(kind, w, sheet);
	});
	return {
		get width() {
			return width;
		},
		get css() {
			return width === "reading" && opts.reading ? opts.reading : PAGE_WIDTH_CSS[width];
		},
	};
}
