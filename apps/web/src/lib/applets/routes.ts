/**
 * The addresses of sites, skills and server tasks.
 *
 * All three are rows of one kind on the server. An address starts with the
 * word a person sees and then shows only the name: `/site/water_log`,
 * `/skill/meeting_prep`, `/server-task/embedding_index`, each with
 * `/options` for the other side of the page.
 *
 * The server's id carries one more fact, whether you made it:
 * `applet_user__water_log` is yours and `applet_dot_cloud` came with the
 * server. The address drops that, so reading one back gives a first guess
 * and a second (`otherId`), and the page tries both. A name you give never
 * equals a built-in's: setup treats the same name as the same applet.
 *
 * The addresses used before these (`/applet/<id>`, `/applet/<id>/view`,
 * `?panel=info`, `/job/<name>`, and `/action/…` before those) are in pins,
 * saved tabs and links the assistant wrote, so `readAddress` still reads
 * them.
 */
import type { AppletKind } from '$lib/api/client';

/** The Sites door. */
export const SITES_ROUTE = '/sites';
/** The Server tasks page in Settings. */
export const SERVER_TASKS_ROUTE = '/virtues/server-tasks';

const YOURS = 'applet_user__';
const ANY = 'applet_';

/** The first word of an address. */
export type AddressWord = 'site' | 'skill' | 'server-task';

const isYoursId = (id: string) => id.startsWith(YOURS);
const nameIn = (id: string) => id.slice(isYoursId(id) ? YOURS.length : ANY.length);

/** The word an address starts with, from what the applet is. */
function wordFor(id: string, kind?: AppletKind | null): AddressWord {
	if (kind === 'site') return 'site';
	if (kind === 'skill') return 'skill';
	if (kind === 'server_task') return 'server-task';
	// The assistant's task, or a caller that knows only the id. A task with
	// a row opens at its task's address (`homeOf` in kind.ts); this one is
	// the address of its options, and of a task that has no row.
	return isYoursId(id) ? 'site' : 'server-task';
}

/** An applet's page. Pass its kind when it is known; the page corrects its own address once it has loaded. */
export function homeRoute(id: string, kind?: AppletKind | null): string {
	return `/${wordFor(id, kind)}/${nameIn(id)}`;
}

/** The options side of that page, with Run history or Technical details open when asked. */
export function optionsRoute(id: string, open?: 'history' | 'details' | null, kind?: AppletKind | null): string {
	return `${homeRoute(id, kind)}/options${open ? `?panel=${open}` : ''}`;
}

export interface Address {
	/** The server's id, as a first guess when the address shows only a name. */
	id: string;
	/** The other id that name could be. Null when the address gave the whole id. */
	otherId: string | null;
	word: AddressWord;
	side: 'home' | 'options';
	/** The part of Options to open, when the address names one. */
	open: 'history' | 'details' | null;
	/** The address as it is written today, under the same first word. */
	route: string;
}

/** Read a site's, a skill's or a server task's address, in today's form or an earlier one. Null for any other route. */
export function readAddress(route: string): Address | null {
	const url = new URL(route, 'http://localhost');
	const panel = url.searchParams.get('panel');
	const open = panel === 'history' || panel === 'details' ? panel : null;

	let id: string;
	let otherId: string | null = null;
	let word: AddressWord;
	let side: Address['side'];
	// `/job/<name>` was a server task's address for a day.
	const now = url.pathname.match(/^\/(site|skill|server-task|job)\/([^/]+)(\/options)?$/);
	if (now) {
		word = now[1] === 'job' ? 'server-task' : (now[1] as AddressWord);
		// A site or a skill is most often one you made, and a server task most often is not.
		const yours = word !== 'server-task';
		id = (yours ? YOURS : ANY) + now[2];
		otherId = (yours ? ANY : YOURS) + now[2];
		side = now[3] ? 'options' : 'home';
	} else {
		const before = url.pathname.match(/^\/(?:applet|action)\/(applet_[^/]+)(\/view)?$/);
		if (!before) return null;
		id = before[1];
		word = wordFor(id);
		side = !before[2] && (panel === 'info' || open) ? 'options' : 'home';
	}
	const home = `/${word}/${nameIn(id)}`;
	return { id, otherId, word, side, open, route: side === 'options' ? `${home}/options${open ? `?panel=${open}` : ''}` : home };
}
