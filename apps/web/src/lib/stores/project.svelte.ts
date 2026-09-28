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
	archiveProject,
	unarchiveProject,
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
import { chatSessions } from '$lib/stores/chatSessions.svelte';

/**
 * A chat is filed by its own `project_id`, which is what the project page and
 * the chat read, not by a membership row. Any change to a chat's membership
 * has to re-read the session list, or the add looks like it did nothing
 * (VIR-359) and the chat still believes it lives where it used to.
 */
const isChatUrl = (url: string) => url.startsWith('/chat/');

export class ProjectStore {
	/** Every project that is not in the trash, archived ones included. */
	private all = $state<ProjectSummary[]>([]);
	loading = $state(false);
	error = $state<string | null>(null);

	private details = $state<Map<string, ProjectDetail>>(new Map());

	/**
	 * The working set — what the Home panel, ⌘K and "Add to project" list.
	 * Archived projects are kept out here rather than at each reader, so a
	 * closed project cannot leak into a menu that forgot to filter.
	 */
	get projects(): ProjectSummary[] {
		return this.all.filter((p) => !p.archived_at);
	}

	/** The closed ones, for the projects page's Archived fold. */
	get archived(): ProjectSummary[] {
		return this.all.filter((p) => !!p.archived_at);
	}

	/** GET /api/projects?include_archived=true — refresh the summary list. */
	async load(): Promise<void> {
		this.loading = true;
		this.error = null;
		try {
			const res = await listProjects({ includeArchived: true });
			this.all = res.projects;
		} catch (e) {
			console.error('[ProjectStore] Failed to load projects:', e);
			this.error = e instanceof Error ? e.message : 'Failed to load projects';
			this.all = [];
		} finally {
			this.loading = false;
		}
	}

	/** POST /api/projects/:id/archive — close it, then refresh list and detail. */
	async archive(id: string): Promise<void> {
		await archiveProject(id);
		await this.afterArchiveChange(id);
	}

	/** POST /api/projects/:id/unarchive — reopen it. */
	async unarchive(id: string): Promise<void> {
		await unarchiveProject(id);
		await this.afterArchiveChange(id);
	}

	/** Refetch rather than evict: an open project page reads the cached
	 *  detail, and evicting it showed "not found" until the refetch landed. */
	private async afterArchiveChange(id: string): Promise<void> {
		await Promise.all([this.load(), this.details.has(id) ? this.get(id, { force: true }) : null]);
	}

	byId(id: string): ProjectSummary | undefined {
		return this.all.find((s) => s.id === id);
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

	/**
	 * POST /api/projects/:id/items — add a member URL and update the cached
	 * detail. A chat lives in one project, so filing one also takes it out of
	 * the project it was in; every cached detail is corrected, not only this one.
	 */
	async addItem(id: string, url: string): Promise<void> {
		const item = await addProjectItem(id, url);
		if (isChatUrl(url)) this.dropFromOtherDetails(id, url);
		const cached = this.details.get(id);
		if (cached) {
			const exists = cached.items.some((i) => i.url === item.url);
			// Membership is idempotent server-side; don't duplicate an existing member.
			this.setDetail({
				...cached,
				items: exists
					? cached.items.map((i) => (i.url === item.url ? item : i))
					: [...cached.items, item]
			});
		}
		await this.afterMembershipChange(url);
	}

	/** DELETE /api/projects/:id/items — remove a member URL and update the cached detail. */
	async removeItem(id: string, url: string): Promise<void> {
		await removeProjectItem(id, url);
		const cached = this.details.get(id);
		if (cached) {
			this.setDetail({ ...cached, items: cached.items.filter((i) => i.url !== url) });
		}
		await this.afterMembershipChange(url);
	}

	/**
	 * The counts are the server's: a chat is counted by its `project_id` and
	 * everything else by its row, and guessing either here drifted. A chat
	 * change also re-reads the session list, which is what the project page
	 * lists chats from and what an open chat reads its project from.
	 */
	private async afterMembershipChange(url: string): Promise<void> {
		await Promise.all([this.load(), isChatUrl(url) ? chatSessions.refresh() : null]);
	}

	private dropFromOtherDetails(keepId: string, url: string): void {
		let changed = false;
		const next = new Map(this.details);
		for (const [pid, d] of next) {
			if (pid !== keepId && d.items.some((i) => i.url === url)) {
				next.set(pid, { ...d, items: d.items.filter((i) => i.url !== url) });
				changed = true;
			}
		}
		if (changed) this.details = next;
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
		const url = `/chat/${chatId}`;
		await updateChat(chatId, { projectId });
		this.dropFromOtherDetails(projectId ?? '', url);
		await this.afterMembershipChange(url);
	}

	private setDetail(detail: ProjectDetail): void {
		const next = new Map(this.details);
		next.set(detail.id, detail);
		this.details = next;
	}
}

export const projectStore = new ProjectStore();
