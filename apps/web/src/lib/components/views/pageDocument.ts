/**
 * How a page opens, and its binding: `PageContent`'s choice, here so it can
 * be tested without compiling the component.
 *
 * - A block page (`format: 'tree'`) binds its XML tree for `DocumentEditor`
 *   (`createTreeDocument`), read-only when the server is older than this
 *   app's document contract and would drop what it writes. Its markdown
 *   converter starts loading beside the socket (`loadConverter`): the
 *   editor mounts once both are ready, so a paste never waits on it. Its
 *   copy on this device is kept under its lineage, the document of the page
 *   the server holds (`treeStore`); the copy this device kept of it as
 *   markdown, and any of an earlier document of it, go to the server
 *   (`keepMarkdownCopy`, `keepEarlierTreeCopies`).
 * - A page still in markdown binds nothing: no editor here edits markdown,
 *   so it is shown read only as its server's markdown of it
 *   (`MarkdownView`). The server moves a markdown page to blocks as it is
 *   opened (`GET /api/pages/:id`), so one still in markdown is one it could
 *   not move.
 */

import { get } from 'svelte/store';
import type { PageFormat } from '$lib/api/client';
import { createTreeDocument, type TreeDocument } from '$lib/yjs';
import { loadConverter } from '$lib/document/converter';
import { keepEarlierTreeCopies, keepMarkdownCopy } from '$lib/yjs/keptCopy';

/** What `GET /api/pages/:id` says about a page's text. */
export interface PageDescription {
	format?: PageFormat | string;
	contract?: number | null;
	box_contract?: number | null;
	/** `page` or `article`; absent from a server older than articles in blocks. */
	kind?: 'page' | 'article' | string;
	/** Which document of a block page the server holds; null or absent before lineages. */
	lineage?: string | null;
}

export type OpenedPage =
	| { format: 'markdown' }
	| {
			format: 'tree';
			doc: TreeDocument;
			/** What the page says while it is shown read-only; null when it is editable. */
			readOnlyNotice: string | null;
	  };

/** What a page still in markdown says above its text. */
export const MARKDOWN_READ_ONLY =
	"Your server couldn't move this page from markdown to blocks, so it opens read only. To edit it, copy its markdown into a new page.";

/** How a page holds its text, as its server describes it. */
export function formatOf(page: PageDescription): PageFormat {
	return page.format === 'tree' ? 'tree' : 'markdown';
}

/** Bind a block page for the block editor; a page in markdown binds nothing. */
export function openPageDocument(pageId: string, page: PageDescription): OpenedPage {
	if (formatOf(page) !== 'tree') return { format: 'markdown' };
	void loadConverter();
	const lineage = page.lineage ?? null;
	// Typing kept in this device's copies of the page that are not its
	// document: from before it moved to blocks, or of an earlier document.
	void keepMarkdownCopy(pageId);
	void keepEarlierTreeCopies(pageId, lineage);
	const doc = createTreeDocument(pageId, page.contract ?? null, page.box_contract ?? null, lineage);
	// Shown read-only only when it is shown at all: a page this editor
	// cannot read says why instead, and is not shown.
	const shown = doc.readOnly && doc.contractConfirmed;
	return { format: 'tree', doc, readOnlyNotice: shown ? get(doc.refused) : null };
}

/**
 * Whether the page is still waiting on something worth a spinner: a block
 * page not synced, and nothing said that it never will be.
 */
export function stillLoading(opened: OpenedPage, synced: boolean, refused: string | null): boolean {
	if (opened.format !== 'tree' || synced) return false;
	return refused === null || refused === opened.readOnlyNotice;
}

/**
 * Whether the editor may be shown on the copy this device kept, when the
 * server has not synced in time: only when the binding says that copy may
 * stand in for the page, which also means it holds a block (an editor bound
 * to an empty tree writes into it).
 */
export function mayShowKeptCopy(opened: OpenedPage, refused: string | null): boolean {
	if (opened.format !== 'tree') return false;
	return opened.doc.localCopyReadable() && (refused === null || refused === opened.readOnlyNotice);
}
