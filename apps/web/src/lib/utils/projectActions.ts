/**
 * The project verbs every surface shares: make one, open the list, file a
 * thing into one. The Home panel, ⌘K, the menus and drag-and-drop all reach
 * for these, so a door behaves the same whichever wall it is in.
 */

import { ApiError, type Project, type ProjectSummary } from '$lib/api/client';
import type { ContextMenuItem } from '$lib/stores/contextMenu.svelte';
import { GETTING_STARTED_CHAT_ID } from '$lib/components/chat/getting-started/getting-started';
import { INTERVIEW_CHAT_ID } from '$lib/components/chat/interview/interview';
import { PROJECT_ICON } from '$lib/utils/iconHelpers';
import { chatSessions } from '$lib/stores/chatSessions.svelte';
import { projectStore } from '$lib/stores/project.svelte';
import { windowShellStore } from '$lib/stores/window-shell.svelte';
import { promptText } from '$lib/stores/dialog.svelte';
import { toast } from 'svelte-sonner';
import { pendingPrompt } from '$lib/stores/pendingPrompt.svelte';
import { pinMenuItem } from '$lib/pins/pinAction';
import { notifyArchived, notifyTrashed, routeIfOpen } from '$lib/utils/toasts';

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
 * What to say when filing fails, in the order a person needs it: the server
 * could not be reached (nothing to fix here, check the server), the server
 * said no and why (its own sentence), or neither is known.
 *
 * Every failure used to read "Your server couldn't add that", which is also
 * what a stopped server looked like, so a server that was simply off read as
 * a bug in projects.
 */
function failure(e: unknown, title: string, fallback: string): [string, { description: string }] {
	const unreachable =
		e instanceof TypeError || (e instanceof ApiError && e.status >= 502 && e.status <= 504);
	if (unreachable) {
		return ["Couldn't reach your server", { description: 'Check that it is on, then try again' }];
	}
	if (e instanceof ApiError && e.status === 400) {
		if (/archived/i.test(e.message)) {
			return [title, { description: 'You archived this project. Unarchive it to add more' }];
		}
		if (e.message) return [title, { description: e.message }];
	}
	return [title, { description: fallback }];
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
	const already =
		home?.id === project.id ||
		('url' in t && projectStore.holding(t.url).some((p) => p.id === project.id));
	if (already) {
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
		toast.error(...failure(e, `Your server couldn't add this to ${project.name}`, 'Nothing changed. Try again'));
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
		toast.error(...failure(e, `Your server couldn't remove this from ${project.name}`, "It's still there. Try again"));
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
	// Things that can be in several projects are ticked in each that holds
	// them, where the view showing them has asked (`loadHolders`).
	const held = new Set(
		'url' in target ? projectStore.holding(target.url).map((p) => p.id) : [],
	);

	const submenu: ContextMenuItem[] = projects.map((p) => ({
		id: `project-${p.id}`,
		label: p.name,
		icon: p.icon || PROJECT_ICON,
		checked: p.id === home?.id || held.has(p.id),
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

/** Open a project in the window you are in. */
export function openProject(project: Pick<Project, 'id' | 'name'>): void {
	windowShellStore.openTabFromRoute(`/project/${project.id}`, {
		label: project.name,
		focusExisting: true,
	});
}

/**
 * The menu on a project chip: the thing you are looking at, seen from the
 * project it is in. Open it, file it somewhere else too (a chat moves, a page
 * joins), or take it out of this one.
 */
export function projectChipMenuItems(
	project: Pick<Project, 'id' | 'name' | 'archived_at'>,
	target: FileTarget,
): ContextMenuItem[] {
	const elsewhere = projectMenuItems(target).map((i) => ({
		...i,
		submenu: i.submenu?.filter((s) => s.id !== 'remove-from-project'),
	}));
	return [
		{ id: 'open-project', label: `Open ${project.name}`, icon: PROJECT_ICON, action: () => openProject(project) },
		...elsewhere,
		{
			id: 'remove-from-this-project',
			label: `Remove from ${project.name}`,
			icon: 'ri:close-line',
			dividerBefore: true,
			action: () => removeFromProject(project, target),
		},
	];
}

// ── A project's own verbs ────────────────────────────────────────────────
// The sidebar row, the projects page and the project's own header all offer
// these; one copy each, so a rename or a delete behaves the same from every
// door.

/**
 * A new chat that already lives in the project: the draft is staged for the
 * next chat to claim, as "Ask this project" does, so its first message files
 * it and grounds retrieval there.
 */
export function newChatInProject(project: Pick<Project, 'id'>): void {
	pendingPrompt.setProject(project.id);
	windowShellStore.openTabFromRoute('/', { label: 'New chat' });
}

export async function renameProject(project: Pick<Project, 'id' | 'name'>): Promise<void> {
	const name = (
		await promptText({ title: 'Rename project', initialValue: project.name, confirmLabel: 'Rename' })
	)?.trim();
	if (!name || name === project.name) return;
	try {
		await projectStore.update(project.id, { name });
	} catch (e) {
		console.error('[projectActions] rename failed:', e);
		toast.error(`Your server couldn't rename "${project.name}"`, {
			description: 'Nothing changed. Try again',
		});
	}
}

/** Reversible, so no confirm: the toast carries the Undo. */
export async function archiveProject(project: Pick<Project, 'id' | 'name'>): Promise<void> {
	try {
		await projectStore.archive(project.id);
		notifyArchived(project.id, project.name);
	} catch (e) {
		console.error('[projectActions] archive failed:', e);
		toast.error(`Your server couldn't archive "${project.name}"`, {
			description: 'Nothing changed. Try again',
		});
	}
}

export async function unarchiveProject(project: Pick<Project, 'id' | 'name'>): Promise<void> {
	try {
		await projectStore.unarchive(project.id);
	} catch (e) {
		console.error('[projectActions] unarchive failed:', e);
		toast.error(`Your server couldn't reopen "${project.name}"`, {
			description: 'Nothing changed. Try again',
		});
	}
}

/**
 * A trip to Recently deleted, with the Undo in the toast. Its chats, pages
 * and files stay where they are; only the project goes. Returns whether it
 * went, for a caller that has somewhere to go next.
 */
export async function deleteProject(project: Pick<Project, 'id' | 'name'>): Promise<boolean> {
	const route = `/project/${project.id}`;
	const reopen = routeIfOpen(route);
	try {
		await projectStore.remove(project.id);
		windowShellStore.closeTabsByRoute(route);
		notifyTrashed({ kind: 'project', id: project.id, name: project.name, reopen });
		return true;
	} catch (e) {
		console.error('[projectActions] delete failed:', e);
		toast.error(`Your server couldn't delete "${project.name}"`, {
			description: "It's still here. Try again",
		});
		return false;
	}
}

/** The menu on a project itself, wherever it is listed. */
export function projectRowMenuItems(
	project: Pick<ProjectSummary, 'id' | 'name' | 'icon' | 'archived_at'>,
): ContextMenuItem[] {
	const url = `/project/${project.id}`;
	if (project.archived_at) {
		return [
			{ id: 'open', label: 'Open', icon: PROJECT_ICON, action: () => openProject(project) },
			{ id: 'unarchive', label: 'Unarchive', icon: 'ri:inbox-unarchive-line', action: () => unarchiveProject(project) },
			{
				id: 'delete',
				label: 'Delete',
				icon: 'ri:delete-bin-line',
				variant: 'destructive',
				dividerBefore: true,
				action: () => void deleteProject(project),
			},
		];
	}
	return [
		{
			id: 'open-beside',
			label: 'Open beside',
			icon: 'ri:layout-column-line',
			action: () => void windowShellStore.openRouteBeside(url, project.name),
		},
		{ id: 'new-chat', label: 'New chat here', icon: 'ri:chat-new-line', action: () => newChatInProject(project) },
		{ id: 'rename', label: 'Rename', icon: 'ri:edit-line', action: () => renameProject(project) },
		// No "Add to project": you can't put a project in a project.
		pinMenuItem({ url, label: project.name, icon: project.icon }),
		{ id: 'archive', label: 'Archive', icon: 'ri:archive-line', action: () => archiveProject(project) },
		{
			id: 'delete',
			label: 'Delete',
			icon: 'ri:delete-bin-line',
			variant: 'destructive',
			dividerBefore: true,
			action: () => void deleteProject(project),
		},
	];
}

/**
 * The glyph for a project member by its url, from the shell's drawn set so a
 * project's contents wear the same marks as the sidebar that lists them. A
 * link out keeps an interface symbol: there is no drawn object for "a page
 * somewhere else".
 */
export function memberIcon(url: string): string {
	if (/^https?:\/\//.test(url)) return 'ri:external-link-line';
	const kind = url.split('/')[1] ?? '';
	const map: Record<string, string> = {
		page: 'atlas:pages',
		person: 'atlas:people',
		place: 'atlas:places',
		org: 'atlas:organizations',
		drive: 'atlas:files',
		day: 'atlas:day',
		year: 'atlas:years',
		chat: 'atlas:chats',
		source: 'atlas:sources'
	};
	return map[kind] ?? 'ri:links-line';
}

/**
 * What to call a member: the name the server resolved, else a link's host,
 * else what kind of thing it is. Never the raw url, which is what a box
 * older than the resolved names would otherwise show.
 */
export function memberName(item: { url: string; title?: string }): string {
	if (item.title) return item.title;
	if (/^https?:\/\//.test(item.url)) {
		try {
			return new URL(item.url).hostname.replace(/^www\./, '');
		} catch {
			return item.url;
		}
	}
	const kind = item.url.split('/')[1] ?? '';
	const names: Record<string, string> = {
		page: 'Page',
		person: 'Person',
		place: 'Place',
		org: 'Organization',
		drive: 'File',
		day: 'Day',
		year: 'Year',
		source: 'Source'
	};
	return names[kind] ?? 'Item';
}
