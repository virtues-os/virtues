/**
 * pageBinding — what the AI is allowed to edit, and what opens when it does.
 *
 * The permission model behind all of this lives in the edit allow list store:
 * reading is implicit, editing is explicit and per-chat. This module is the
 * chat view's side of it — bind a page, grant a gated tool its entity, and
 * open what `create_page` just made. Every one of these opens or edits a page
 * BESIDE the chat (Category A); the chat pane is never navigated in place.
 *
 * A markdown page is bound with its Yjs document, so each turn carries the
 * text as the editor holds it. A block page is bound with none: the server
 * reads its tree for the model, and the markdown binding is one the server
 * refuses on a block page.
 */

import { get } from "svelte/store";
import { getPage } from "$lib/api/client";
import { editAllowListStore, type EditableResourceType } from "$lib/stores/editAllowList.svelte";
import { windowShellStore } from "$lib/stores/window-shell.svelte";
import { createYjsDocument, type YjsDocument } from "$lib/yjs";

/** The first bound page on the edit allow list, if any. */
export function getBoundPage() {
	return editAllowListStore.items.find((i) => i.type === "page");
}

export function clearBoundPages() {
	const pages = editAllowListStore.items.filter((i) => i.type === "page");
	for (const page of pages) {
		editAllowListStore.remove("page", page.id);
	}
}

/**
 * The bound page as the agent sees it: id, title, and, for a markdown page,
 * the CURRENT Yjs content, so an AI edit is computed against what is actually
 * in the editor. No content for a block page, or for a markdown page whose
 * document has not synced yet: the server then reads the page itself, or
 * tells the model to, rather than the model reading an empty page.
 */
export function activePageContext() {
	const page = getBoundPage();
	if (!page) return null;

	const doc = page.yjsDoc;
	const content = doc && get(doc.isSynced) ? doc.ytext.toString() : undefined;

	return {
		page_id: page.id,
		page_title: page.title || undefined,
		...(content !== undefined && { content }),
	};
}

/**
 * The page's Yjs document for the chat, or none: a markdown page gets one;
 * a block page, or a page the server could not describe, does not.
 */
async function chatDocument(pageId: string): Promise<YjsDocument | undefined> {
	let page;
	try {
		page = await getPage(pageId);
	} catch (err) {
		console.warn("[pageBinding] couldn't read the page; binding it without its text:", err);
		return undefined;
	}
	if (page.format === "tree") return undefined;
	// A server from before the document contract does not say; every page on
	// it is markdown (as `PageContent` reads it).
	const contract = page.contract === undefined ? 0 : page.contract;
	return createYjsDocument(pageId, contract);
}

/** Bind a page for editing. No auto-open — the user can open it if they want. */
export async function bindPage(pageId: string, pageTitle: string) {
	clearBoundPages();
	const yjsDoc = await chatDocument(pageId);
	await editAllowListStore.addPage(pageId, pageTitle, yjsDoc);
}

/**
 * Grant a gated tool the entity it asked for. Awaited, so the backend has the
 * permission before the caller retries the turn.
 */
export async function grantEditPermission(
	entityId: string,
	entityType: string,
	title: string,
): Promise<void> {
	if (entityType === "page") {
		// Already on the list: the permission stands, and a second document
		// would be a second socket nobody closes.
		if (editAllowListStore.isAllowed("page", entityId)) return;
		const yjsDoc = await chatDocument(entityId);
		await editAllowListStore.addPage(entityId, title, yjsDoc);
	} else {
		// folder / action / wiki_entry — no Yjs doc, granted generically by (type, id)
		await editAllowListStore.add({
			type: entityType as EditableResourceType,
			id: entityId,
			title,
		});
	}
}

/** Auto-bind and open a page the model just created, in split view. */
export function openCreatedPage(pageId: string, title: string) {
	clearBoundPages();
	// Don't create Yjs doc here — PageContent will create one when the tab mounts.
	// Creating a second doc causes two WebSocket connections to the same room,
	// which races with the server's Y.Text initialization.
	editAllowListStore.addPage(pageId, title);

	// Open the page the model just created without ever CREATING a split: beside
	// the chat when the user is already in split view, otherwise a new tab in the
	// active pane. Never navigate the chat in place, and never auto-split.
	windowShellStore.openRouteInSplitOrActive(`/page/${pageId}`);
}
