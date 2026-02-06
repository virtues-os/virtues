/**
 * Pending Edits Store
 *
 * Tracks AI-proposed page edits that are awaiting user accept/reject.
 * Used to coordinate between EditDiffCard (in chat) and the page editor.
 */

export interface PendingEdit {
	editId: string;
	pageId: string;
	/** Original text that was replaced (for reject/undo and diff display) */
	find: string;
	/** New text that replaced it */
	replace: string;
	/** When the edit was made */
	timestamp: number;
}

/**
 * Store of pending edits per page
 * Key: pageId, Value: array of pending edits for that page
 */
let editsMap = $state<Map<string, PendingEdit[]>>(new Map());

/**
 * Add a pending edit
 */
export function addPendingEdit(edit: PendingEdit) {
	const pageEdits = editsMap.get(edit.pageId) ?? [];
	editsMap.set(edit.pageId, [...pageEdits, edit]);
}

/**
 * Accept an edit - removes it from pending (edit stays in document)
 */
export function acceptEdit(pageId: string, editId: string) {
	const pageEdits = editsMap.get(pageId);
	if (pageEdits) {
		editsMap.set(
			pageId,
			pageEdits.filter((e) => e.editId !== editId)
		);
	}
}

/**
 * Reject an edit - removes from pending
 * Note: The actual document revert is handled by the caller
 */
export function rejectEdit(pageId: string, editId: string) {
	const pageEdits = editsMap.get(pageId);
	if (pageEdits) {
		editsMap.set(
			pageId,
			pageEdits.filter((e) => e.editId !== editId)
		);
	}
}

/**
 * Get a specific pending edit by ID
 */
export function getPendingEdit(pageId: string, editId: string): PendingEdit | undefined {
	return editsMap.get(pageId)?.find((e) => e.editId === editId);
}

/**
 * Get all pending edits for a page
 */
export function getPendingEditsForPage(pageId: string): PendingEdit[] {
	return editsMap.get(pageId) ?? [];
}

/**
 * Get total count of pending edits across all pages
 */
export function getPendingEditCount(): number {
	let count = 0;
	for (const edits of editsMap.values()) {
		count += edits.length;
	}
	return count;
}

/**
 * Clear all pending edits for a page
 */
export function clearPendingEdits(pageId: string) {
	editsMap.delete(pageId);
}

/**
 * Check if an edit is pending
 */
export function isEditPending(pageId: string, editId: string): boolean {
	return editsMap.get(pageId)?.some((e) => e.editId === editId) ?? false;
}

/**
 * Reactive getter for pending edits map (for components that need reactivity)
 */
export function getPendingEditsMap(): Map<string, PendingEdit[]> {
	return editsMap;
}
