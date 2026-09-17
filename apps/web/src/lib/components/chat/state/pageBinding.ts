/**
 * pageBinding — what the AI is allowed to edit, and what opens when it does.
 *
 * The permission model behind all of this lives in the edit allow list store:
 * reading is implicit, editing is explicit and per-chat. This module is the
 * chat view's side of it — bind a page, grant a gated tool its entity, and
 * open what `create_page` just made. Every one of these opens or edits a page
 * BESIDE the chat (Category A); the chat pane is never navigated in place.
 */

import { editAllowListStore, type EditableResourceType } from "$lib/stores/editAllowList.svelte";
import { windowShellStore } from "$lib/stores/window-shell.svelte";
import { createYjsDocument } from "$lib/yjs";

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
 * The bound page as the agent sees it: id, title, and the CURRENT Yjs content,
 * so an AI edit is computed against what is actually in the editor.
 */
export function activePageContext() {
	const page = getBoundPage();
	if (!page) return null;

	const content = page.yjsDoc?.ytext.toString() || "";

	return {
		page_id: page.id,
		page_title: page.title || undefined,
		content: content,
	};
}

/** Bind a page for editing. No auto-open — the user can open it if they want. */
export function bindPage(pageId: string, pageTitle: string) {
	clearBoundPages();
	const yjsDoc = createYjsDocument(pageId);
	editAllowListStore.addPage(pageId, pageTitle, yjsDoc);
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
		const yjsDoc = createYjsDocument(entityId);
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
