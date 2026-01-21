// Bookmarks store using Svelte 5 runes

export interface Bookmark {
	id: string;
	bookmark_type: 'tab' | 'entity';
	route: string | null;
	tab_type: string | null;
	label: string;
	icon: string | null;
	entity_type: string | null;
	entity_id: string | null;
	entity_slug: string | null;
	sort_order: number;
	created_at: string;
	updated_at: string;
}

export interface CreateTabBookmarkRequest {
	route: string;
	tab_type: string;
	label: string;
	icon?: string;
}

export interface CreateEntityBookmarkRequest {
	entity_type: string;
	entity_id: string;
	entity_slug: string;
	label: string;
	icon?: string;
}

export interface ToggleBookmarkResponse {
	bookmarked: boolean;
	bookmark: Bookmark | null;
}

class BookmarksStore {
	bookmarks = $state<Bookmark[]>([]);
	loading = $state(false);
	error = $state<string | null>(null);

	// Computed sets for quick lookups
	private get routeSet(): Set<string> {
		return new Set(
			this.bookmarks.filter((b) => b.bookmark_type === 'tab' && b.route).map((b) => b.route!)
		);
	}

	private get entitySet(): Set<string> {
		return new Set(
			this.bookmarks
				.filter((b) => b.bookmark_type === 'entity' && b.entity_id)
				.map((b) => b.entity_id!)
		);
	}

	/**
	 * Load all bookmarks from the server
	 */
	async load(): Promise<void> {
		this.loading = true;
		this.error = null;

		try {
			const response = await fetch('/api/bookmarks');
			if (!response.ok) {
				throw new Error('Failed to fetch bookmarks');
			}
			this.bookmarks = await response.json();
		} catch (e) {
			this.error = e instanceof Error ? e.message : 'Unknown error';
			console.error('[Bookmarks] Failed to load:', e);
		} finally {
			this.loading = false;
		}
	}

	/**
	 * Check if a route is bookmarked
	 */
	isRouteBookmarked(route: string): boolean {
		return this.routeSet.has(route);
	}

	/**
	 * Check if an entity is bookmarked
	 */
	isEntityBookmarked(entityId: string): boolean {
		return this.entitySet.has(entityId);
	}

	/**
	 * Get the bookmark for a specific route
	 */
	getBookmarkForRoute(route: string): Bookmark | undefined {
		return this.bookmarks.find((b) => b.bookmark_type === 'tab' && b.route === route);
	}

	/**
	 * Get the bookmark for a specific entity
	 */
	getBookmarkForEntity(entityId: string): Bookmark | undefined {
		return this.bookmarks.find((b) => b.bookmark_type === 'entity' && b.entity_id === entityId);
	}

	/**
	 * Toggle bookmark for a route (create or delete)
	 */
	async toggleRouteBookmark(params: CreateTabBookmarkRequest): Promise<boolean> {
		try {
			const response = await fetch('/api/bookmarks/toggle/tab', {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify(params)
			});

			if (!response.ok) {
				const error = await response.json();
				throw new Error(error.error || 'Failed to toggle bookmark');
			}

			const result: ToggleBookmarkResponse = await response.json();
			await this.load(); // Refresh list
			return result.bookmarked;
		} catch (e) {
			console.error('[Bookmarks] Failed to toggle route bookmark:', e);
			throw e;
		}
	}

	/**
	 * Toggle bookmark for an entity (create or delete)
	 */
	async toggleEntityBookmark(params: CreateEntityBookmarkRequest): Promise<boolean> {
		try {
			const response = await fetch('/api/bookmarks/toggle/entity', {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify(params)
			});

			if (!response.ok) {
				const error = await response.json();
				throw new Error(error.error || 'Failed to toggle bookmark');
			}

			const result: ToggleBookmarkResponse = await response.json();
			await this.load(); // Refresh list
			return result.bookmarked;
		} catch (e) {
			console.error('[Bookmarks] Failed to toggle entity bookmark:', e);
			throw e;
		}
	}

	/**
	 * Add a tab bookmark
	 */
	async addTabBookmark(params: CreateTabBookmarkRequest): Promise<Bookmark> {
		try {
			const response = await fetch('/api/bookmarks/tab', {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify(params)
			});

			if (!response.ok) {
				const error = await response.json();
				throw new Error(error.error || 'Failed to add bookmark');
			}

			const bookmark: Bookmark = await response.json();
			await this.load(); // Refresh list
			return bookmark;
		} catch (e) {
			console.error('[Bookmarks] Failed to add tab bookmark:', e);
			throw e;
		}
	}

	/**
	 * Add an entity bookmark
	 */
	async addEntityBookmark(params: CreateEntityBookmarkRequest): Promise<Bookmark> {
		try {
			const response = await fetch('/api/bookmarks/entity', {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify(params)
			});

			if (!response.ok) {
				const error = await response.json();
				throw new Error(error.error || 'Failed to add bookmark');
			}

			const bookmark: Bookmark = await response.json();
			await this.load(); // Refresh list
			return bookmark;
		} catch (e) {
			console.error('[Bookmarks] Failed to add entity bookmark:', e);
			throw e;
		}
	}

	/**
	 * Remove a bookmark by ID
	 */
	async removeBookmark(id: string): Promise<void> {
		try {
			const response = await fetch(`/api/bookmarks/${id}`, {
				method: 'DELETE'
			});

			if (!response.ok) {
				const error = await response.json();
				throw new Error(error.error || 'Failed to remove bookmark');
			}

			await this.load(); // Refresh list
		} catch (e) {
			console.error('[Bookmarks] Failed to remove bookmark:', e);
			throw e;
		}
	}
}

export const bookmarks = new BookmarksStore();
