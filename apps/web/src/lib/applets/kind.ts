/**
 * What an applet is to the person who meets it, and whose it is.
 *
 * The app shows three words: a site, a task and a skill. A task is either
 * the assistant's or your server's, so there are four kinds. The server says
 * which, as `kind` (`derived_kind` in virtues-core/src/scheduler/applets.rs),
 * and the app reads it: the rule is not repeated here. The server ships this
 * app, so there is no older server that leaves the field out.
 *
 * Whose it is (`origin`) is a separate fact. A site can come with the
 * server, and you can set up a task.
 */
import type { Applet, AppletKind } from '$lib/api/client';
import { homeRoute, optionsRoute } from './routes';

type Parts = Pick<Applet, 'kind'>;

/**
 * Whether something other than a person starts it: a time, new data, or a
 * post from outside. Server tasks are grouped by it, and a site that starts
 * itself has a refresh to turn on and off.
 */
export function startsItself(a: Pick<Applet, 'schedule' | 'triggers'> & { watch?: Applet['watch'] }): boolean {
	return !!a.schedule?.trim() || !!a.watch || !!a.triggers?.includes('webhook');
}

/**
 * A site that only shows something: no refresh, no prompt and no program.
 * It has nothing to run, so nothing to turn on or off and no options worth a
 * side of its page. A chart of your sleep is one; a water log you add a
 * glass to from a chat is not, since it has a prompt.
 */
export function isPlainSite(a: Pick<Applet, 'kind' | 'schedule' | 'triggers' | 'agent' | 'command'> & { watch?: Applet['watch'] }): boolean {
	return a.kind === 'site' && !startsItself(a) && !a.agent?.trim() && !a.command?.length;
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
	return a.kind === 'site' || (a.kind === 'assistant_task' && !a.task_id);
}

type Addressed = Parts & Pick<Applet, 'id'> & { task_id?: string | null };

/** An applet's own page: its view or what it made, with its options on the other side. */
export const pageOf = (a: Addressed) => homeRoute(a.id, a.kind);

/**
 * Where an applet opens. A task of the assistant's that repeats opens as
 * its task, in Tasks. Anything else opens on its own page.
 */
export const homeOf = (a: Addressed) => (a.kind === 'assistant_task' && a.task_id ? `/task/${a.task_id}` : pageOf(a));

/** The options side of that page, with Run history or Technical details open when asked. */
export const optionsOf = (a: Addressed, open?: 'history' | 'details') => optionsRoute(a.id, open, a.kind);
