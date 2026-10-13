/**
 * Which editor a page opens in, and its binding: `PageContent`'s choice, here
 * so it can be tested without compiling the component.
 *
 * - A block page (`format: 'tree'`) binds its XML tree for `DocumentEditor`
 *   (`createTreeDocument`), read-only when the server is older than this
 *   app's document contract and would drop what it writes.
 * - Anything else binds its Y.Text for `CodeMirrorEditor` (`createYjsDocument`).
 *   A server from before block pages sends no `format`, and every page on it
 *   is markdown; one from before the contract sends no `contract`, and every
 *   page on it is contract 0.
 */

import { get } from 'svelte/store';
import type { PageFormat } from '$lib/api/client';
import { createTreeDocument, createYjsDocument, type TreeDocument, type YjsDocument } from '$lib/yjs';

/** What `GET /api/pages/:id` says about a page's text. */
export interface PageDescription {
	format?: PageFormat | string;
	contract?: number | null;
	box_contract?: number | null;
}

export type OpenedPage =
	| { format: 'markdown'; doc: YjsDocument }
	| {
			format: 'tree';
			doc: TreeDocument;
			/** What the page says while it is shown read-only; null when it is editable. */
			readOnlyNotice: string | null;
	  };

/** How a page holds its text, as its server describes it. */
export function formatOf(page: PageDescription): PageFormat {
	return page.format === 'tree' ? 'tree' : 'markdown';
}

/** Bind a page in the editor its format calls for. */
export function openPageDocument(pageId: string, page: PageDescription): OpenedPage {
	if (formatOf(page) === 'tree') {
		const doc = createTreeDocument(pageId, page.contract ?? null, page.box_contract ?? null);
		// Shown read-only only when it is shown at all: a page this editor
		// cannot read says why instead, and is not shown.
		const shown = doc.readOnly && doc.contractConfirmed;
		return { format: 'tree', doc, readOnlyNotice: shown ? get(doc.refused) : null };
	}
	return { format: 'markdown', doc: createYjsDocument(pageId, page.contract === undefined ? 0 : page.contract) };
}

/**
 * Whether the page is still waiting on something worth a spinner: not synced,
 * and nothing said that it never will be.
 */
export function stillLoading(opened: OpenedPage, synced: boolean, refused: string | null): boolean {
	if (synced) return false;
	return refused === null || (opened.format === 'tree' && refused === opened.readOnlyNotice);
}

/**
 * Whether the editor may be shown on the copy this device kept, when the
 * server has not synced in time: only when the binding says that copy may
 * stand in for the page, which for a block page also means it holds a block
 * (an editor bound to an empty tree writes into it).
 */
export function mayShowKeptCopy(opened: OpenedPage, refused: string | null): boolean {
	if (opened.format === 'tree') {
		return opened.doc.localCopyReadable() && (refused === null || refused === opened.readOnlyNotice);
	}
	return refused === null && opened.doc.contractConfirmed;
}
