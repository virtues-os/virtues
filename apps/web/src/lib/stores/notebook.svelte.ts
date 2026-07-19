/**
 * Notebook Store
 *
 * A Notebook is the "room" a chat lives in — a manual collection the user returns
 * to (project, pet, hobby, goal, topic). It gathers entities, chats, and pages
 * as URL-native members and carries a single accent tint plus a catch-up memo.
 *
 * This store owns Notebook CRUD, the membership list, and the chat↔Notebook binding.
 * Tab/window/URL concerns live in `window-shell.svelte.ts`, not here.
 */

import {
	listNotebooks,
	getNotebook,
	createNotebook,
	updateNotebook,
	deleteNotebook,
	addNotebookItem,
	removeNotebookItem,
	reorderNotebookItems,
	updateChat,
	type Notebook,
	type NotebookSummary,
	type NotebookDetail
} from '$lib/api/client';

export class NotebookStore {
	notebooks = $state<NotebookSummary[]>([]);
	loading = $state(false);
	error = $state<string | null>(null);

	private details = $state<Map<string, NotebookDetail>>(new Map());

	/** GET /api/notebooks — refresh the summary list. */
	async load(): Promise<void> {
		this.loading = true;
		this.error = null;
		try {
			const res = await listNotebooks();
			this.notebooks = res.notebooks;
		} catch (e) {
			console.error('[NotebookStore] Failed to load notebooks:', e);
			this.error = e instanceof Error ? e.message : 'Failed to load notebooks';
			this.notebooks = [];
		} finally {
			this.loading = false;
		}
	}

	byId(id: string): NotebookSummary | undefined {
		return this.notebooks.find((s) => s.id === id);
	}

	/** Return the cached detail, or fetch + cache it. Pass `{ force: true }` to refetch. */
	async get(id: string, opts?: { force?: boolean }): Promise<NotebookDetail> {
		if (!opts?.force) {
			const cached = this.details.get(id);
			if (cached) return cached;
		}
		const detail = await getNotebook(id);
		this.setDetail(detail);
		return detail;
	}

	getCached(id: string): NotebookDetail | undefined {
		return this.details.get(id);
	}

	/** POST /api/notebooks — create, refresh the list, return the new Notebook. */
	async create(name: string, opts?: { icon?: string | null; accent_color?: string | null }): Promise<Notebook> {
		const notebook = await createNotebook({ name, icon: opts?.icon, accent_color: opts?.accent_color });
		await this.load();
		return notebook;
	}

	/** PUT /api/notebooks/:id — patch a Notebook, then refresh. */
	async update(
		id: string,
		patch: {
			name?: string;
			icon?: string | null;
			accent_color?: string | null;
			current_status?: string | null;
			instructions?: string | null;
			sort_order?: number;
		}
	): Promise<Notebook> {
		const updated = await updateNotebook(id, patch);
		// Merge into any cached detail.
		const cached = this.details.get(id);
		if (cached) this.setDetail({ ...cached, ...updated });
		await this.load();
		return updated;
	}

	/** DELETE /api/notebooks/:id — remove, then refresh. */
	async remove(id: string): Promise<void> {
		await deleteNotebook(id);
		if (this.details.has(id)) {
			const next = new Map(this.details);
			next.delete(id);
			this.details = next;
		}
		await this.load();
	}

	/** POST /api/notebooks/:id/items — add a member URL and update the cached detail. */
	async addItem(id: string, url: string): Promise<void> {
		const item = await addNotebookItem(id, url);
		const cached = this.details.get(id);
		if (cached) {
			const exists = cached.items.some((i) => i.url === item.url);
			// Membership is idempotent server-side; don't duplicate an existing member.
			if (exists) {
				this.setDetail({ ...cached, items: cached.items.map((i) => (i.url === item.url ? item : i)) });
			} else {
				this.setDetail({ ...cached, items: [...cached.items, item] });
				this.bumpItemCount(id, 1);
			}
		} else {
			await this.get(id, { force: true });
		}
	}

	/** DELETE /api/notebooks/:id/items — remove a member URL and update the cached detail. */
	async removeItem(id: string, url: string): Promise<void> {
		await removeNotebookItem(id, url);
		const cached = this.details.get(id);
		if (cached) {
			this.setDetail({ ...cached, items: cached.items.filter((i) => i.url !== url) });
		}
		this.bumpItemCount(id, -1);
	}

	/** PUT /api/notebooks/:id/items/reorder — set the member order and update the cached detail. */
	async reorderItems(id: string, urls: string[]): Promise<void> {
		await reorderNotebookItems(id, urls);
		const cached = this.details.get(id);
		if (cached) {
			const byUrl = new Map(cached.items.map((i) => [i.url, i]));
			const reordered = urls
				.map((url, idx) => {
					const item = byUrl.get(url);
					return item ? { ...item, sort_order: idx } : null;
				})
				.filter((i): i is (typeof cached.items)[number] => i !== null);
			this.setDetail({ ...cached, items: reordered });
		}
	}

	/**
	 * Bind (or detach, with `null`) a chat to a Notebook. Folds the chat into the
	 * Notebook's membership server-side, so we reload the list (chat_count changes).
	 */
	async setChatNotebook(chatId: string, notebookId: string | null): Promise<void> {
		await updateChat(chatId, { notebookId });
		// Reconcile chat_counts in the background — don't block the caller on a
		// second round-trip (the breadcrumb's local state already reflects the pick).
		this.load();
	}

	private setDetail(detail: NotebookDetail): void {
		const next = new Map(this.details);
		next.set(detail.id, detail);
		this.details = next;
	}

	private bumpItemCount(id: string, delta: number): void {
		this.notebooks = this.notebooks.map((s) =>
			s.id === id ? { ...s, item_count: Math.max(0, s.item_count + delta) } : s
		);
	}
}

export const notebookStore = new NotebookStore();
