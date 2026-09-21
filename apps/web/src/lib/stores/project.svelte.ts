/**
 * Project Store
 *
 * A Project is the "room" a chat lives in — a manual collection the user returns
 * to (project, pet, hobby, goal, topic). It gathers entities, chats, and pages
 * as URL-native members and carries a single accent tint plus a catch-up memo.
 *
 * This store owns Project CRUD, the membership list, and the chat↔Project binding.
 * Tab/window/URL concerns live in `window-shell.svelte.ts`, not here.
 */

import {
	listProjects,
	getProject,
	createProject,
	updateProject,
	deleteProject,
	addProjectItem,
	removeProjectItem,
	reorderProjectItems,
	setProjectItemRole,
	type ProjectItemRole,
	updateChat,
	type Project,
	type ProjectSummary,
	type ProjectDetail
} from '$lib/api/client';

export class ProjectStore {
	projects = $state<ProjectSummary[]>([]);
	loading = $state(false);
	error = $state<string | null>(null);

	private details = $state<Map<string, ProjectDetail>>(new Map());

	/** GET /api/projects — refresh the summary list. */
	async load(): Promise<void> {
		this.loading = true;
		this.error = null;
		try {
			const res = await listProjects();
			this.projects = res.projects;
		} catch (e) {
			console.error('[ProjectStore] Failed to load projects:', e);
			this.error = e instanceof Error ? e.message : 'Failed to load projects';
			this.projects = [];
		} finally {
			this.loading = false;
		}
	}

	byId(id: string): ProjectSummary | undefined {
		return this.projects.find((s) => s.id === id);
	}

	/** Return the cached detail, or fetch + cache it. Pass `{ force: true }` to refetch. */
	async get(id: string, opts?: { force?: boolean }): Promise<ProjectDetail> {
		if (!opts?.force) {
			const cached = this.details.get(id);
			if (cached) return cached;
		}
		const detail = await getProject(id);
		this.setDetail(detail);
		return detail;
	}

	getCached(id: string): ProjectDetail | undefined {
		return this.details.get(id);
	}

	/** POST /api/projects — create, refresh the list, return the new Project. */
	async create(name: string, opts?: { icon?: string | null; accent_color?: string | null }): Promise<Project> {
		const project = await createProject({ name, icon: opts?.icon, accent_color: opts?.accent_color });
		await this.load();
		return project;
	}

	/** PUT /api/projects/:id — patch a Project, then refresh. */
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
	): Promise<Project> {
		const updated = await updateProject(id, patch);
		// Merge into any cached detail.
		const cached = this.details.get(id);
		if (cached) this.setDetail({ ...cached, ...updated });
		await this.load();
		return updated;
	}

	/** DELETE /api/projects/:id — remove, then refresh. */
	async remove(id: string): Promise<void> {
		await deleteProject(id);
		if (this.details.has(id)) {
			const next = new Map(this.details);
			next.delete(id);
			this.details = next;
		}
		await this.load();
	}

	/** POST /api/projects/:id/items — add a member URL and update the cached detail. */
	async addItem(id: string, url: string): Promise<void> {
		const item = await addProjectItem(id, url);
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

	/** DELETE /api/projects/:id/items — remove a member URL and update the cached detail. */
	async removeItem(id: string, url: string): Promise<void> {
		await removeProjectItem(id, url);
		const cached = this.details.get(id);
		if (cached) {
			this.setDetail({ ...cached, items: cached.items.filter((i) => i.url !== url) });
		}
		this.bumpItemCount(id, -1);
	}

	/** PUT /api/projects/:id/items/reorder — set the member order and update the cached detail. */
	async reorderItems(id: string, urls: string[]): Promise<void> {
		await reorderProjectItems(id, urls);
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

	/** PUT /api/projects/:id/items/role — change what a member is to the project. */
	async setItemRole(id: string, url: string, role: ProjectItemRole): Promise<void> {
		const updated = await setProjectItemRole(id, url, role);
		const cached = this.details.get(id);
		if (cached) {
			this.setDetail({
				...cached,
				items: cached.items.map((i) => (i.url === url ? updated : i))
			});
		}
	}

	/**
	 * Bind (or detach, with `null`) a chat to a Project. Folds the chat into the
	 * Project's membership server-side, so we reload the list (chat_count changes).
	 */
	async setChatProject(chatId: string, projectId: string | null): Promise<void> {
		await updateChat(chatId, { projectId });
		// Reconcile chat_counts in the background — don't block the caller on a
		// second round-trip (the breadcrumb's local state already reflects the pick).
		this.load();
	}

	private setDetail(detail: ProjectDetail): void {
		const next = new Map(this.details);
		next.set(detail.id, detail);
		this.details = next;
	}

	private bumpItemCount(id: string, delta: number): void {
		this.projects = this.projects.map((s) =>
			s.id === id ? { ...s, item_count: Math.max(0, s.item_count + delta) } : s
		);
	}
}

export const projectStore = new ProjectStore();
