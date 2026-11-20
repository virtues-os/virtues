/**
 * The addresses of sites and jobs.
 *
 * A site is something you set up, and a job is something your server runs
 * for itself. Both are rows of the same kind on the server, and the id says
 * which: `applet_user__water_log` is yours, `applet_embedding_index` is the
 * server's. An address shows only the name: `/site/water_log`,
 * `/job/embedding_index`, with `/options` for the other side of the page.
 *
 * The addresses used before these (`/applet/<id>`, `/applet/<id>/view`,
 * `?panel=info`, and `/action/…` before that) are in pins, saved tabs and
 * links the assistant wrote, so `readAddress` still reads them.
 */

/** The Sites door. */
export const SITES_ROUTE = '/sites';
/** The Jobs page in Settings. */
export const JOBS_ROUTE = '/virtues/jobs';

const YOURS = 'applet_user__';
const ANY = 'applet_';

/** Whether an id is something you set up. Anything else is the server's own. */
export const isSiteId = (id: string) => id.startsWith(YOURS);

/** A site's or a job's page. */
export function homeRoute(id: string): string {
	return isSiteId(id) ? `/site/${id.slice(YOURS.length)}` : `/job/${id.slice(ANY.length)}`;
}

/** The options side of that page, with Run history or Technical details open when asked. */
export function optionsRoute(id: string, open?: 'history' | 'details'): string {
	return `${homeRoute(id)}/options${open ? `?panel=${open}` : ''}`;
}

export interface Address {
	/** The server's id. */
	id: string;
	side: 'home' | 'options';
	/** The part of Options to open, when the address names one. */
	open: 'history' | 'details' | null;
	/** The address as it is written today. */
	route: string;
}

/** Read a site's or a job's address, in today's form or an earlier one. Null for any other route. */
export function readAddress(route: string): Address | null {
	const url = new URL(route, 'http://localhost');
	const panel = url.searchParams.get('panel');
	const open = panel === 'history' || panel === 'details' ? panel : null;

	let id: string;
	let side: Address['side'];
	const now = url.pathname.match(/^\/(site|job)\/([^/]+)(\/options)?$/);
	if (now) {
		id = (now[1] === 'site' ? YOURS : ANY) + now[2];
		side = now[3] ? 'options' : 'home';
	} else {
		const before = url.pathname.match(/^\/(?:applet|action)\/(applet_[^/]+)(\/view)?$/);
		if (!before) return null;
		id = before[1];
		side = !before[2] && (panel === 'info' || open) ? 'options' : 'home';
	}
	return { id, side, open, route: side === 'options' ? optionsRoute(id, open ?? undefined) : homeRoute(id) };
}
