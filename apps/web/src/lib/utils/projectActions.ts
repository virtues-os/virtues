/**
 * The project verbs every surface shares: make one, open the list, file a
 * thing into one. The Home panel, ⌘K, the menus and drag-and-drop all reach
 * for these, so a door behaves the same whichever wall it is in.
 */

import type { Project, ProjectSummary } from '$lib/api/client';
import { chatSessions } from '$lib/stores/chatSessions.svelte';
import { projectStore } from '$lib/stores/project.svelte';
import { windowShellStore } from '$lib/stores/window-shell.svelte';
import { promptText } from '$lib/stores/dialog.svelte';
import { toast } from 'svelte-sonner';

export async function newProject(): Promise<void> {
	const name = (
		await promptText({
			title: 'New project',
			placeholder: 'Name your project',
			confirmLabel: 'Create',
		})
	)?.trim();
	if (!name) return;
	try {
		const project = await projectStore.create(name);
		windowShellStore.openTabFromRoute(`/project/${project.id}`, { label: project.name });
	} catch (e) {
		console.error('[projectActions] Failed to create project:', e);
		toast.error("Your server couldn't create that project", {
			description: 'Nothing changed. Try again',
		});
	}
}

export function openProjects(): void {
	windowShellStore.openTabFromRoute('/projects', { label: 'Projects', focusExisting: true });
}

/** A project's own url, in either spelling. You can't put a project in a project. */
export function isProjectUrl(url: string): boolean {
	return /^\/(?:project|notebook)\//.test(url);
}

/**
 * The member url for a route, or null when the route is not one thing that
 * can be filed: a list (`/pages`), a new chat that has no id yet (`/`), a
 * project. A link out is filed as itself.
 */
export function projectMemberUrl(route: string): string | null {
	if (/^https?:\/\//.test(route)) return route;
	const path = route.split(/[?#]/)[0];
	if (isProjectUrl(path)) return null;
	return /^\/[a-z-]+\/[^/]+$/.test(path) ? path : null;
}

/** The project a chat lives in, if the session list knows it. */
export function projectOfChat(url: string): ProjectSummary | undefined {
	if (!url.startsWith('/chat/')) return undefined;
	const chatId = url.slice('/chat/'.length);
	const pid = chatSessions.sessions.find((s) => s.conversation_id === chatId)?.project_id;
	return pid ? projectStore.byId(pid) : undefined;
}

/**
 * File a thing into a project and say what happened. A chat lives in one
 * project, so filing one that is elsewhere moves it; filing it where it
 * already is does nothing and says so.
 */
export async function fileIntoProject(
	project: Pick<Project, 'id' | 'name' | 'archived_at'>,
	url: string,
): Promise<void> {
	const home = projectOfChat(url);
	if (home?.id === project.id) {
		toast(`Already in ${project.name}`);
		return;
	}
	try {
		await projectStore.addItem(project.id, url);
		toast(home ? `Moved to ${project.name}` : `Added to ${project.name}`);
	} catch (e) {
		console.error('[projectActions] Failed to add to project:', e);
		toast.error(`Your server couldn't add this to ${project.name}`, {
			description: project.archived_at
				? 'You archived this project. Unarchive it to add more'
				: 'Nothing changed. Try again',
		});
	}
}

/**
 * Sidebar rows drag as this type, carrying the member url. A custom type
 * rather than `text/uri-list` so the file drop on a project page, and every
 * browser default, can tell a row from a file or a link out of another app.
 */
export const REF_DRAG_TYPE = 'application/x-virtues-ref';

export function startRefDrag(e: DragEvent, url: string, label: string): void {
	if (!e.dataTransfer) return;
	e.dataTransfer.setData(REF_DRAG_TYPE, url);
	e.dataTransfer.setData('text/plain', label);
	e.dataTransfer.effectAllowed = 'link';
}

/** Whether a drag in progress carries a row, readable during dragover. */
export function isRefDrag(e: DragEvent): boolean {
	return !!e.dataTransfer?.types.includes(REF_DRAG_TYPE);
}

/** The member url a dropped row carries, or null for anything else. */
export function droppedRefUrl(e: DragEvent): string | null {
	const url = e.dataTransfer?.getData(REF_DRAG_TYPE);
	return url ? projectMemberUrl(url) : null;
}
