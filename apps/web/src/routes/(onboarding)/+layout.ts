import { redirect } from '@sveltejs/kit';
import type { LayoutLoad } from './$types';

// Session gate for the (onboarding) group. Mirrors (app)/+layout.ts but
// skips the profile / server-status fetches since the wizard runs BEFORE
// the user has set any preferences.
export const load: LayoutLoad = async ({ fetch }) => {
	try {
		const sessionResponse = await fetch('/auth/session');
		if (!sessionResponse.ok) {
			throw redirect(303, '/pair');
		}
		const sessionData = await sessionResponse.json();
		if (!sessionData.user) {
			throw redirect(303, '/pair');
		}
		return { session: sessionData };
	} catch (error) {
		// SvelteKit's redirect() throws a Redirect object (has `status`), NOT a
		// Response — the old `instanceof Response` check never matched, so the
		// intentional /pair redirects above fell through to the catch-all below.
		// Re-throw genuine redirects untouched (mirrors (app)/+layout.ts).
		if (error && typeof error === 'object' && 'status' in error) throw error;
		// Anything else (network, parse error) — punt to login so the user
		// can retry instead of getting stuck on a half-rendered wizard.
		throw redirect(303, '/pair');
	}
};
