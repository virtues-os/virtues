/**
 * Edit Allow List Store
 *
 * Manages the list of resources the AI is allowed to edit.
 * Syncs with the backend for permission persistence across clients.
 *
 * Permission model:
 * - Read: Implicit (AI can read any resource in the space)
 * - Edit: Explicit (AI can only edit resources on this allow list)
 *
 * The list is per-chat and cleared when the chat is deleted (via CASCADE).
 */

import { type YjsDocument } from '$lib/yjs';

/**
 * Resource types that can be edited
 */
export type EditableResourceType = 'page' | 'folder' | 'wiki_entry';

/**
 * An item on the edit allow list
 */
export interface EditAllowListItem {
	/** Resource type */
	type: EditableResourceType;
	/** Unique identifier (e.g., page_id) */
	id: string;
	/** Display title */
	title: string;
	/** Optional icon identifier */
	icon?: string;
	/** For pages: the Yjs document for real-time sync */
	yjsDoc?: YjsDocument;
}

/**
 * Context sent to the backend for a single editable resource
 * Used in chat API requests
 */
export interface EditableResourceContext {
	type: EditableResourceType;
	id: string;
	title?: string;
	/** Current content from Yjs document (for pages) */
	content?: string;
}

/**
 * Permission from backend API
 */
interface BackendPermission {
	id: string;
	chat_id: string;
	entity_id: string;
	entity_type: string;
	entity_title: string | null;
	granted_at: string;
}

interface EditAllowListState {
	/** Current chat ID (for backend sync) */
	chatId: string | null;
	/** Resources the AI can edit */
	items: EditAllowListItem[];
	/** Whether we're syncing with backend */
	loading: boolean;
	/** Whether the chat exists in the backend (for deferred sync) */
	chatExistsInBackend: boolean;
}

function createEditAllowListStore() {
	let state = $state<EditAllowListState>({
		chatId: null,
		items: [],
		loading: false,
		chatExistsInBackend: false
	});

	/**
	 * Fetch permissions from backend
	 */
	async function fetchFromBackend(chatId: string): Promise<EditAllowListItem[]> {
		try {
			const response = await fetch(`/api/chats/${chatId}/permissions`);
			if (!response.ok) {
				console.warn('Failed to fetch permissions:', response.statusText);
				return [];
			}
			const data = await response.json();
			const permissions: BackendPermission[] = data.permissions || [];

			return permissions.map((p) => ({
				type: p.entity_type as EditableResourceType,
				id: p.entity_id,
				title: p.entity_title || 'Untitled'
			}));
		} catch (error) {
			console.warn('Error fetching permissions:', error);
			return [];
		}
	}

	/**
	 * Add permission to backend
	 */
	async function addToBackend(
		chatId: string,
		item: EditAllowListItem
	): Promise<boolean> {
		try {
			const response = await fetch(`/api/chats/${chatId}/permissions`, {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({
					entity_id: item.id,
					entity_type: item.type,
					entity_title: item.title
				})
			});
			return response.ok;
		} catch (error) {
			console.warn('Error adding permission:', error);
			return false;
		}
	}

	/**
	 * Remove permission from backend
	 */
	async function removeFromBackend(chatId: string, entityId: string): Promise<boolean> {
		try {
			const response = await fetch(`/api/chats/${chatId}/permissions/${entityId}`, {
				method: 'DELETE'
			});
			return response.ok;
		} catch (error) {
			console.warn('Error removing permission:', error);
			return false;
		}
	}

	return {
		/** Get all items (reactive) */
		get items() {
			return state.items;
		},

		/** Get current chat ID */
		get chatId() {
			return state.chatId;
		},

		/** Check if currently loading */
		get loading() {
			return state.loading;
		},

		/** Check if any resources are allowed */
		get hasItems() {
			return state.items.length > 0;
		},

		/** Get count of allowed resources */
		get count() {
			return state.items.length;
		},

		/**
		 * Initialize the store for a specific chat
		 * Fetches existing permissions from backend
		 */
		async init(chatId: string) {
			// Skip if already initialized for this chat
			if (state.chatId === chatId) {
				return;
			}

			// Clear previous state
			this.clear();

			state.chatId = chatId;
			state.loading = true;
			state.chatExistsInBackend = true; // Chat exists if we're fetching from it

			// Fetch permissions from backend
			const items = await fetchFromBackend(chatId);
			state.items = items;
			state.loading = false;
		},

		/**
		 * Set the chat ID without fetching from backend
		 * Used for new chats that don't have permissions yet
		 */
		setChatId(chatId: string) {
			if (state.chatId !== chatId) {
				this.clear();
				state.chatId = chatId;
				state.chatExistsInBackend = false; // New chat doesn't exist in backend yet
			}
		},

		/**
		 * Mark the chat as created in the backend and sync pending permissions
		 * Call this after the first message is sent
		 */
		async markChatCreated() {
			if (state.chatExistsInBackend) return; // Already marked

			state.chatExistsInBackend = true;

			// Sync all local items to backend
			if (state.chatId && state.items.length > 0) {
				for (const item of state.items) {
					await addToBackend(state.chatId, item);
				}
			}
		},

		/**
		 * Add a resource to the allow list
		 * Syncs with backend if chat exists, otherwise stores locally for later sync
		 */
		async add(item: EditAllowListItem) {
			// Don't add duplicates
			if (state.items.some((i) => i.type === item.type && i.id === item.id)) {
				return;
			}

			// Add to local state
			state.items = [...state.items, item];

			// Only sync with backend if chat exists there
			if (state.chatId && state.chatExistsInBackend) {
				const success = await addToBackend(state.chatId, item);
				if (!success) {
					// Rollback on failure
					state.items = state.items.filter((i) => !(i.type === item.type && i.id === item.id));
					console.warn('Failed to sync permission with backend');
				}
			}
			// If chat doesn't exist yet, item stays in local state
			// and will be synced when markChatCreated() is called
		},

		/**
		 * Add a page to the allow list (convenience method)
		 */
		async addPage(pageId: string, title: string, yjsDoc?: YjsDocument) {
			await this.add({
				type: 'page',
				id: pageId,
				title,
				yjsDoc
			});
		},

		/**
		 * Remove a resource from the allow list
		 * Syncs with backend if chat exists
		 */
		async remove(type: EditableResourceType, id: string) {
			const item = state.items.find((i) => i.type === type && i.id === id);

			// Clean up Yjs document if it's a page
			if (item?.yjsDoc) {
				item.yjsDoc.destroy();
			}

			// Remove from local state
			state.items = state.items.filter((i) => !(i.type === type && i.id === id));

			// Only sync with backend if chat exists there
			if (state.chatId && state.chatExistsInBackend && item) {
				const success = await removeFromBackend(state.chatId, id);
				if (!success) {
					// Rollback on failure (re-add without Yjs doc)
					state.items = [...state.items, { ...item, yjsDoc: undefined }];
					console.warn('Failed to sync permission removal with backend');
				}
			}
			// If chat doesn't exist yet, item is just removed from local state
		},

		/**
		 * Check if a resource is on the allow list
		 */
		isAllowed(type: EditableResourceType, id: string): boolean {
			return state.items.some((i) => i.type === type && i.id === id);
		},

		/**
		 * Get a specific item from the list
		 */
		get(type: EditableResourceType, id: string): EditAllowListItem | undefined {
			return state.items.find((i) => i.type === type && i.id === id);
		},

		/**
		 * Get all page IDs on the allow list
		 * Non-reactive getter for passing to API calls
		 */
		getAllowedPageIds(): string[] {
			return state.items.filter((i) => i.type === 'page').map((i) => i.id);
		},

		/**
		 * Get context for all allowed resources (for API requests)
		 * Includes current content from Yjs documents where available
		 */
		getContextForApi(): EditableResourceContext[] {
			return state.items.map((item) => {
				const context: EditableResourceContext = {
					type: item.type,
					id: item.id,
					title: item.title
				};

				// For pages with Yjs, include current content
				if (item.type === 'page' && item.yjsDoc) {
					context.content = item.yjsDoc.ytext.toString();
				}

				return context;
			});
		},

		/**
		 * Clear all resources from the allow list
		 * Does NOT sync with backend (use for local cleanup only)
		 */
		clear() {
			// Clean up all Yjs documents
			for (const item of state.items) {
				if (item.yjsDoc) {
					item.yjsDoc.destroy();
				}
			}
			state.items = [];
			state.chatId = null;
			state.chatExistsInBackend = false;
		},

		/**
		 * Update the Yjs document for a page
		 * Used when a page is opened/synced
		 */
		updateYjsDoc(pageId: string, yjsDoc: YjsDocument) {
			const item = state.items.find((i) => i.type === 'page' && i.id === pageId);
			if (item) {
				// Clean up old doc if different
				if (item.yjsDoc && item.yjsDoc !== yjsDoc) {
					item.yjsDoc.destroy();
				}
				item.yjsDoc = yjsDoc;
			}
		}
	};
}

export const editAllowListStore = createEditAllowListStore();
