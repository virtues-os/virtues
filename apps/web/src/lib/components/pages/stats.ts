/**
 * What the page status bar counts, from whichever editor holds the page: the
 * CodeMirror editor counts its markdown, the block editor its tree
 * (`lib/document/stats.ts`).
 */
export interface DocStats {
	wordCount: number;
	charCount: number;
	linkCount: number;
	mediaCount: number;
}
