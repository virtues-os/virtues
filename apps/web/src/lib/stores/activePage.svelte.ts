/**
 * Active Page Store
 *
 * Manages the active page state for AI-assisted page editing.
 * Tracks which page is bound for editing.
 */

import { type YjsDocument } from '$lib/yjs';

interface ActivePageState {
	/** Currently bound page ID */
	boundPageId: string | null;
	/** Title of the bound page */
	boundPageTitle: string | null;
	/** Active Yjs document for the bound page */
	yjsDoc: YjsDocument | null;
}

function createActivePageStore() {
	let state = $state<ActivePageState>({
		boundPageId: null,
		boundPageTitle: null,
		yjsDoc: null
	});

	return {
		/** Get current state (reactive) */
		get boundPageId() {
			return state.boundPageId;
		},
		get boundPageTitle() {
			return state.boundPageTitle;
		},
		get yjsDoc() {
			return state.yjsDoc;
		},
		get isBound() {
			return state.boundPageId !== null;
		},

		/**
		 * Bind to a page for editing
		 * @param pageId - The page ID to bind
		 * @param pageTitle - The page title for display
		 * @param yjsDoc - The Yjs document instance
		 */
		bind(pageId: string, pageTitle: string, yjsDoc: YjsDocument) {
			// Clean up previous binding if any
			if (state.yjsDoc && state.yjsDoc !== yjsDoc) {
				state.yjsDoc.destroy();
			}

			state = {
				boundPageId: pageId,
				boundPageTitle: pageTitle,
				yjsDoc: yjsDoc
			};
		},

		/**
		 * Unbind from current page (stop editing)
		 */
		unbind() {
			// Clean up Yjs document
			if (state.yjsDoc) {
				state.yjsDoc.destroy();
			}

			state = {
				boundPageId: null,
				boundPageTitle: null,
				yjsDoc: null
			};
		},

		/**
		 * Get bound page ID (for tool context)
		 * Non-reactive getter for passing to API calls
		 */
		getBoundPageId(): string | null {
			return state.boundPageId;
		},

		/**
		 * Get bound page title
		 * Non-reactive getter
		 */
		getBoundPageTitle(): string | null {
			return state.boundPageTitle;
		},

		/**
		 * Get Yjs document (for AI edits)
		 * Non-reactive getter
		 */
		getYjsDoc(): YjsDocument | null {
			return state.yjsDoc;
		}
	};
}

export const activePageStore = createActivePageStore();
