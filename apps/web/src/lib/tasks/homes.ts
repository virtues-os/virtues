/**
 * A task's home and what else it is about.
 *
 * A home is one of five fields on a task (`thread_id`, `person_id`,
 * `place_id`, `org_id`, `thing_id`), at most one set. "Also about" is a list
 * of the same five kinds. Both are shown and picked as links, `/person/<id>`
 * and so on, the same routes the rest of the app opens.
 */
import type { Task, TaskLink, TaskPatch } from '$lib/api/client';
import { identityOf } from '$lib/refs/identity.svelte';
import { projectStore } from '$lib/stores/project.svelte';
import { thingsStore } from '$lib/stores/things.svelte';
import { parseRef } from '$lib/utils/refRoutes';
import { THREAD_ICON } from '$lib/utils/iconHelpers';

export type SubjectKind = TaskLink['kind'];

export const SUBJECT_KINDS: SubjectKind[] = ['thread', 'person', 'place', 'org', 'thing'];

const FIELD = {
	thread: 'thread_id',
	person: 'person_id',
	place: 'place_id',
	org: 'org_id',
	thing: 'thing_id',
} as const;

/** The task's home, or null when it has none yet. */
export function homeOf(t: Pick<Task, 'thread_id' | 'thing_id' | 'person_id' | 'place_id' | 'org_id'>): TaskLink | null {
	for (const kind of SUBJECT_KINDS) {
		const id = t[FIELD[kind]];
		if (id) return { kind, id };
	}
	return null;
}

/** The edit that gives a task `home`, clearing the other four. */
export function homePatch(home: TaskLink | null): TaskPatch {
	const patch: TaskPatch = {};
	for (const kind of SUBJECT_KINDS) patch[FIELD[kind]] = home?.kind === kind ? home.id : null;
	return patch;
}

export function subjectUrl(link: TaskLink): string {
	return `/${link.kind}/${link.id}`;
}

/** A link from a route, when the route names a thread or a subject. */
export function linkFromUrl(url: string): TaskLink | null {
	const ref = parseRef(url);
	if (!ref || !SUBJECT_KINDS.includes(ref.kind as SubjectKind)) return null;
	return { kind: ref.kind as SubjectKind, id: ref.id };
}

export function sameLink(a: TaskLink | null, b: TaskLink | null): boolean {
	return !!a && !!b && a.kind === b.kind && a.id === b.id;
}

export function subjectIcon(kind: SubjectKind): string {
	switch (kind) {
		case 'thread':
			return THREAD_ICON;
		case 'person':
			return 'atlas:people';
		case 'place':
			return 'atlas:places';
		case 'org':
			return 'atlas:organizations';
		case 'thing':
			return 'atlas:things';
	}
}

/** What a thread or subject is called, from this client's stores or the server. */
export function subjectName(link: TaskLink): string {
	if (link.kind === 'thread') {
		const p = [...projectStore.projects, ...projectStore.archived].find((x) => x.id === link.id);
		if (p) return p.name || 'Untitled thread';
	}
	if (link.kind === 'thing') {
		const thing = thingsStore.byId(link.id);
		if (thing) return thing.title;
	}
	const title = identityOf(subjectUrl(link)).title;
	return title === subjectUrl(link) ? 'Loading…' : title;
}
