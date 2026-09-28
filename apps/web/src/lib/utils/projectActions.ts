/**
 * The project verbs every surface shares: make one, open the list, file a
 * thing into one. The Home panel, ⌘K, the menus and drag-and-drop all reach
 * for these, so a door behaves the same whichever wall it is in.
 */

import type { Project, ProjectSummary } from '$lib/api/client';
import type { ContextMenuItem } from '$lib/stores/contextMenu.svelte';
import { GETTING_STARTED_CHAT_ID } from '$lib/components/chat/getting-started/getting-started';
import { INTERVIEW_CHAT_ID } from '$lib/components/chat/interview/interview';
import { PROJECT_ICON } from '$lib/utils/iconHelpers';
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
 * Chats that are never filed. Setup and the interview are rooms of their own,
 * and the interview is the most private text on the box; a project's brief
 * and items have no business in either.
 */
const RESERVED_CHATS = new Set([GETTING_STARTED_CHAT_ID, INTERVIEW_CHAT_ID]);

/**
 * The member url for a route, or null when the route is not one thing that
 * can be filed: a list (`/pages`), a new chat that has no id yet (`/`), a
 * project, a reserved chat. A link out is filed as itself.
 */
export function projectMemberUrl(route: string): string | null {
	if (/^https?:\/\//.test(route)) return route;
	const path = route.split(/[?#]/)[0];
	if (isProjectUrl(path)) return null;
	if (path.startsWith('/chat/') && RESERVED_CHATS.has(path.slice('/chat/'.length))) return null;
	return /^\/[a-z-]+\/[^/]+$/.test(path) ? path : null;
}

/**
 * What can be filed: a thing with a url, or an unsent chat, which has no url
 * yet and is filed through its draft (see `ProjectStore.drafts`).
 */
export type FileTarget = { url: string } | { draftChat: string };

/**
 * The target a tab would file. An unsent chat tab files its draft; a
 * temporary chat, which the box never writes, files nothing.
 */
export function targetForTab(tab: { id: string; route: string }): FileTarget | null {
	const url = projectMemberUrl(tab.route);
	if (url) return { url };
	if (tab.route.includes('temporary=1')) return null;
	const chat = projectStore.unsentChatOf(tab.id);
	return chat ? { draftChat: chat } : null;
}

/** The project a target is in now: a chat's own, or an unsent chat's draft. */
export function projectOfTarget(target: FileTarget): ProjectSummary | undefined {
	if ('draftChat' in target) {
		const pid = projectStore.draftFor(target.draftChat);
		return pid ? projectStore.byId(pid) : undefined;
	}
	return projectOfChat(target.url);
}

/** The project a chat lives in, if the session list knows it. */
export function projectOfChat(url: string): ProjectSummary | undefined {
	if (!url.startsWith('/chat/')) return undefined;
	const pid = chatSessions.projectOf(url.slice('/chat/'.length));
	return pid ? projectStore.byId(pid) : undefined;
}

/**
 * File a thing into a project and say what happened. A chat lives in one
 * project, so filing one that is elsewhere moves it; filing it where it
 * already is does nothing and says so.
 */
export async function fileIntoProject(
	project: Pick<Project, 'id' | 'name' | 'archived_at'>,
	target: FileTarget | string,
): Promise<void> {
	const t: FileTarget = typeof target === 'string' ? { url: target } : target;
	const home = projectOfTarget(t);
	if (home?.id === project.id) {
		toast(`Already in ${project.name}`);
		return;
	}
	if ('draftChat' in t) {
		// Nothing to send yet. The chat carries it to the server with its
		// first message, and shows it at the top until then.
		projectStore.setDraft(t.draftChat, project.id);
		toast(`This chat will be in ${project.name}`);
		return;
	}
	try {
		await projectStore.addItem(project.id, t.url);
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

/** Take a thing out of a project. Its chats, pages and files stay where they are. */
export async function removeFromProject(
	project: Pick<Project, 'id' | 'name'>,
	target: FileTarget | string,
): Promise<void> {
	const t: FileTarget = typeof target === 'string' ? { url: target } : target;
	if ('draftChat' in t) {
		projectStore.setDraft(t.draftChat, null);
		toast(`Removed from ${project.name}`);
		return;
	}
	try {
		await projectStore.removeItem(project.id, t.url);
		toast(`Removed from ${project.name}`);
	} catch (e) {
		console.error('[projectActions] Failed to remove from project:', e);
		toast.error(`Your server couldn't remove this from ${project.name}`, {
			description: "It's still there. Try again",
		});
	}
}

/**
 * The one "Add to project" menu, for every surface that files things: the
 * sidebar, tabs, the chat's own menu, the project page's rows, grids.
 *
 * A chat lives in one project, and the menu says which: a check where it is,
 * "Move to project" as the label, and "Remove from" to take it out. Anything
 * else is filed additively and the menu does not track where.
 */
export function projectMenuItems(target: FileTarget | null): ContextMenuItem[] {
	if (!target) return [];
	const projects = projectStore.projects;
	const home = projectOfTarget(target);

	const submenu: ContextMenuItem[] = projects.map((p) => ({
		id: `project-${p.id}`,
		label: p.name,
		icon: p.icon || PROJECT_ICON,
		checked: p.id === home?.id,
		action: () => fileIntoProject(p, target),
	}));

	submenu.push({
		id: 'new-project-with-item',
		label: 'New project…',
		icon: 'ri:add-line',
		dividerBefore: projects.length > 0,
		action: async () => {
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
				await fileIntoProject(project, target);
			} catch (e) {
				console.error('[projectActions] Failed to create project:', e);
				toast.error("Your server couldn't create that project", {
					description: 'Nothing changed. Try again',
				});
			}
		},
	});

	if (home) {
		submenu.push({
			id: 'remove-from-project',
			label: `Remove from ${home.name}`,
			icon: 'ri:close-line',
			action: () => removeFromProject(home, target),
		});
	}

	return [
		{
			id: 'add-to-project',
			label: home ? 'Move to project' : 'Add to project',
			icon: 'ri:folder-add-line',
			dividerBefore: true,
			submenu,
		},
	];
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
