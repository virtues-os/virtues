import { redirect } from '@sveltejs/kit';
import type { LayoutLoad } from './$types';

// Session gate for the (onboarding) group. The same rule as (app)/+layout.ts
// but without the profile / server-status fetches, since Setup runs before
// there are preferences to load.
//
// ONLY A REAL REJECTION MEANS UNPAIRED. This gate used to send every failure
// to /pair — a thrown fetch, a 502 while the loopback proxy came up, a parse
// error — so a paired person on a blip mid-Setup landed on the pairing screen,
// and Setup's own "couldn't reach your server" screen never showed. A 401/403
// or a session with no user is unpaired; anything else is the server being
// unreachable for a moment, which Setup says itself (`session: null`).
export const load: LayoutLoad = async ({ fetch }) => {
	try {
		// One retry after a beat: on the phone this rides the iroh loopback,
		// which can still be rebuilding right after the app resumes.
		let res: Response;
		try {
			res = await fetch('/auth/session');
		} catch {
			await new Promise((r) => setTimeout(r, 1500));
			res = await fetch('/auth/session');
		}
		if (!res.ok) {
			if (res.status === 401 || res.status === 403) throw redirect(303, '/pair');
			return { session: null };
		}
		const sessionData = await res.json();
		if (!sessionData.user) throw redirect(303, '/pair');
		return { session: sessionData };
	} catch (error) {
		// SvelteKit's redirect() throws a Redirect object (has `status`).
		if (error && typeof error === 'object' && 'status' in error) throw error;
		return { session: null };
	}
};
