/**
 * Projects Store
 *
 * Projects are curated, reusable sets of references (pages, chats, people,
 * places, files) that a user applies as a context lens in chat. A project
 * is a flat list of URLs with a name, icon, and optional description —
 * not a workspace. Users @-mention projects in chat to inline their
 * members as salience hints for the agent.
 */

import {
	listProjects,
	getProject,
	createProject,
	updateProject,
	deleteProject,
	addProjectItem as apiAddProjectItem,
	removeProjectItem as apiRemoveProjectItem,
	reorderProjectItems as apiReorderProjectItems,
	type Project,
	type ProjectSummary,
	type ProjectDetail,
	type ProjectItem,
} from '$lib/api/client';

class ProjectsStore {
	projects = $state<ProjectSummary[]>([]);
	loading = $state(false);
	error = $state<string | null>(null);

	// Cache of full project details (id → ProjectDetail)
	private detailCache = $state<Map<string, ProjectDetail>>(new Map());

	async load(): Promise<void> {
		this.loading = true;
		this.error = null;
		try {
			const res = await listProjects();
			this.projects = res.projects;
		} catch (e) {
			console.error('[ProjectsStore] Failed to load projects:', e);
			this.error = e instanceof Error ? e.message : 'Failed to load projects';
			this.projects = [];
		} finally {
			this.loading = false;
		}
	}

	async loadDetail(id: string, force = false): Promise<ProjectDetail> {
		if (!force) {
			const cached = this.detailCache.get(id);
			if (cached) return cached;
		}
		const detail = await getProject(id);
		const next = new Map(this.detailCache);
		next.set(id, detail);
		this.detailCache = next;
		return detail;
	}

	getCachedDetail(id: string): ProjectDetail | undefined {
		return this.detailCache.get(id);
	}

	async create(
		name: string,
		options?: { icon?: string | null; description?: string | null },
	): Promise<Project> {
		const project = await createProject(name, options);
		this.projects = [
			...this.projects,
			{ ...project, item_count: 0 } as ProjectSummary,
		];
		return project;
	}

	async update(
		id: string,
		updates: {
			name?: string;
			icon?: string | null;
			description?: string | null;
			sort_order?: number;
		},
	): Promise<Project> {
		const updated = await updateProject(id, updates);
		this.projects = this.projects.map((p) =>
			p.id === id ? { ...p, ...updated } : p,
		);
		const cached = this.detailCache.get(id);
		if (cached) {
			const next = new Map(this.detailCache);
			next.set(id, { ...cached, ...updated });
			this.detailCache = next;
		}
		return updated;
	}

	async remove(id: string): Promise<void> {
		await deleteProject(id);
		this.projects = this.projects.filter((p) => p.id !== id);
		if (this.detailCache.has(id)) {
			const next = new Map(this.detailCache);
			next.delete(id);
			this.detailCache = next;
		}
	}

	async addItem(
		projectId: string,
		url: string,
		options?: { name?: string | null; description?: string | null },
	): Promise<ProjectItem> {
		const item = await apiAddProjectItem(projectId, url, options);
		// Invalidate detail cache — next read will repopulate
		if (this.detailCache.has(projectId)) {
			const next = new Map(this.detailCache);
			next.delete(projectId);
			this.detailCache = next;
		}
		// Bump item_count optimistically
		this.projects = this.projects.map((p) =>
			p.id === projectId ? { ...p, item_count: p.item_count + 1 } : p,
		);
		return item;
	}

	async removeItem(projectId: string, url: string): Promise<void> {
		await apiRemoveProjectItem(projectId, url);
		if (this.detailCache.has(projectId)) {
			const next = new Map(this.detailCache);
			next.delete(projectId);
			this.detailCache = next;
		}
		this.projects = this.projects.map((p) =>
			p.id === projectId ? { ...p, item_count: Math.max(0, p.item_count - 1) } : p,
		);
	}

	async reorderItems(projectId: string, urls: string[]): Promise<void> {
		await apiReorderProjectItems(projectId, urls);
		if (this.detailCache.has(projectId)) {
			const next = new Map(this.detailCache);
			next.delete(projectId);
			this.detailCache = next;
		}
	}
}

export const projectsStore = new ProjectsStore();
