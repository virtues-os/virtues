/**
 * What an applet is to the person who meets it, and whose it is.
 *
 * The app shows three words: a site, a task and a skill. A task is either
 * the assistant's or your server's, so there are four kinds. The server says
 * which (`kind`, from `kind_of` in virtues-core/src/scheduler/applets.rs). A
 * server from before that field does not, so the same rule is applied here
 * to the parts the applet has. Keep the two in step.
 *
 * Whose it is (`origin`) is a separate fact. A site can come with the
 * server, and you can set up a task.
 */
import type { Applet, AppletKind } from '$lib/api/client';
import { homeRoute, optionsRoute } from './routes';

type Parts = Pick<Applet, 'schedule' | 'triggers'> & Partial<Pick<Applet, 'kind' | 'has_face' | 'command' | 'watch'>>;

/** Whether something other than a person starts it: a time, new data, or a post from outside. */
export function startsItself(a: Pick<Applet, 'schedule' | 'triggers'> & { watch?: Applet['watch'] }): boolean {
	return !!a.schedule?.trim() || !!a.watch || !!a.triggers?.includes('webhook');
}

/** The rule, read in order: a view is a site, a program is the server's task, anything else that starts itself is the assistant's task, and what is left is a skill. */
export function kindOf(a: Parts): AppletKind {
	if (a.kind) return a.kind;
	if (a.has_face) return 'site';
	if (a.command?.length) return 'server_task';
	return startsItself(a) ? 'assistant_task' : 'skill';
}

/** The word a person sees for each kind. */
export const KIND_WORD: Record<AppletKind, 'Site' | 'Task' | 'Skill'> = {
	site: 'Site',
	server_task: 'Task',
	assistant_task: 'Task',
	skill: 'Skill',
};

/** Whether you set it up, or the assistant did for you, and it did not come with the server or an account. */
export const isYours = (a: Pick<Applet, 'origin'>) => a.origin === 'user' || a.origin === 'ai';

/**
 * Whether the Sites door lists it: a site you set up. A skill you made is
 * under Skills in Settings, and a task of the assistant's is in Tasks once
 * it has its row there. One without a row, on a server from before those
 * rows, stays in the door so that it is still somewhere.
 */
export function inSitesDoor(a: Parts & Pick<Applet, 'origin' | 'archived_at'> & { task_id?: string | null }): boolean {
	if (!isYours(a) || a.archived_at) return false;
	const kind = kindOf(a);
	return kind === 'site' || (kind === 'assistant_task' && !a.task_id);
}

type Addressed = Parts & Pick<Applet, 'id'> & { task_id?: string | null };

/** An applet's own page: its view or what it made, with its options on the other side. */
export const pageOf = (a: Addressed) => homeRoute(a.id, kindOf(a));

/**
 * Where an applet opens. A task of the assistant's that repeats opens as
 * its task, in Tasks. Anything else opens on its own page.
 */
export const homeOf = (a: Addressed) => (kindOf(a) === 'assistant_task' && a.task_id ? `/task/${a.task_id}` : pageOf(a));

/** The options side of that page, with Run history or Technical details open when asked. */
export const optionsOf = (a: Addressed, open?: 'history' | 'details') => optionsRoute(a.id, open, kindOf(a));
