/**
 * The sidebar's Recent: chats, pages, projects, tasks and applets, newest
 * first, as one list.
 *
 * A thing is as recent as the later of two moments: when it last changed
 * (a chat's last message, a page's or project's or task's last edit) and
 * when you last opened it (the visits log). An applet has no edit time of
 * its own that is yours, so only opening it counts.
 *
 * Pinned things are left out: Pinned already shows them, and the same row
 * twice costs two of Recent's ten places. Archived projects and applets,
 * and closed tasks, are left out because they are put away.
 */
import { chatSessions, type ChatSession } from '$lib/stores/chatSessions.svelte';
import { pagesStore } from '$lib/stores/pages.svelte';
import { projectStore } from '$lib/stores/project.svelte';
import { tasksStore } from '$lib/stores/tasks.svelte';
import { appletsStore } from '$lib/stores/applets.svelte';
import { pinsStore } from '$lib/stores/pins.svelte';
import { visits } from '$lib/stores/visits.svelte';
import { isUntitled, untitled } from '$lib/refs/identity.svelte';
import type { Applet, PageSummary, ProjectSummary, Task } from '$lib/api/client';

export type RecentKind = 'chat' | 'page' | 'project' | 'task' | 'applet';

interface RecentBase {
	/** The thing's route, which is also its key. */
	url: string;
	title: string;
	/** The title is a stand-in ("Untitled"), drawn muted. */
	untitled: boolean;
	ts: number;
}

export type RecentRow =
	| (RecentBase & { kind: 'chat'; session: ChatSession })
	| (RecentBase & { kind: 'page'; page: PageSummary })
	| (RecentBase & { kind: 'project'; project: ProjectSummary })
	| (RecentBase & { kind: 'task'; task: Task })
	| (RecentBase & { kind: 'applet'; applet: Applet });

/** The noun for each kind, for the filter and the row's tooltip. */
export const RECENT_KIND_LABEL: Record<RecentKind, string> = {
	chat: 'Chat',
	page: 'Page',
	project: 'Project',
	task: 'Task',
	applet: 'Applet',
};

function stamp(iso: string | null | undefined): number {
	if (!iso) return 0;
	const t = Date.parse(iso);
	return Number.isNaN(t) ? 0 : t;
}

function named(title: string | null | undefined, kind: RecentKind) {
	const t = title?.trim() ?? '';
	return { title: t || untitled(kind), untitled: isUntitled(t, kind) };
}

/** A task opens in the list it is shown in: the assistant's go under Assistant. */
export function taskRoute(t: Task): string {
	const base = t.assignee === 'assistant' ? '/tasks/assistant' : '/tasks';
	return `${base}?task=${encodeURIComponent(t.id)}`;
}

let loading = false;

/** Load what Recent reads that no other mounted view loads for it. Once. */
export function loadRecentSources() {
	if (loading) return;
	loading = true;
	void visits.loadRecent();
	if (!tasksStore.open.length && !tasksStore.loading) void tasksStore.load();
	if (!appletsStore.loaded) void appletsStore.load();
}

/** Every recent thing, newest first, pinned ones left out. Not capped. */
export function recentRows(): RecentRow[] {
	const rows: RecentRow[] = [];

	for (const session of chatSessions.sessions) {
		const id = session.conversation_id;
		rows.push({
			kind: 'chat',
			url: `/chat/${id}`,
			...named(session.title, 'chat'),
			// The last message, not the row's last touch: filing or renaming an
			// old chat must not move it to the top.
			ts: Math.max(stamp(session.last_message_at || session.last_updated), visits.lastVisit('chat', id)),
			session,
		});
	}

	for (const page of pagesStore.pages) {
		rows.push({
			kind: 'page',
			url: `/page/${page.id}`,
			...named(page.title, 'page'),
			ts: Math.max(stamp(page.updated_at), visits.lastVisit('page', page.id)),
			page,
		});
	}

	for (const project of projectStore.projects) {
		if (project.archived_at) continue;
		rows.push({
			kind: 'project',
			url: `/project/${project.id}`,
			...named(project.name, 'project'),
			ts: Math.max(stamp(project.updated_at), visits.lastVisit('project', project.id)),
			project,
		});
	}

	for (const task of tasksStore.open) {
		rows.push({
			kind: 'task',
			url: taskRoute(task),
			...named(task.title, 'task'),
			ts: stamp(task.updated_at),
			task,
		});
	}

	for (const applet of appletsStore.mine) {
		const ts = visits.lastVisit('applet', applet.id);
		if (!ts) continue;
		rows.push({
			kind: 'applet',
			url: `/applet/${applet.id}`,
			...named(applet.name, 'applet'),
			ts,
			applet,
		});
	}

	return rows.filter((r) => r.ts > 0 && !pinsStore.isPinned(r.url)).sort((a, b) => b.ts - a.ts);
}
