import { redirect } from '@sveltejs/kit';
import type { LayoutLoad } from './$types';

export const load: LayoutLoad = async ({ fetch, url }) => {
	// Check authentication via Rust auth API
	try {
		const sessionResponse = await fetch('/auth/session');

		// Treat any non-OK response as unauthenticated
		if (!sessionResponse.ok) {
			throw redirect(303, '/pair');
		}

		const sessionData = await sessionResponse.json();

		// Redirect to login if not authenticated
		if (!sessionData.user) {
			throw redirect(303, '/pair');
		}

		// Skip profile check for OAuth callback (it handles its own redirect)
		if (url.pathname.startsWith('/oauth/')) {
			return { session: sessionData };
		}

		// Setup gate: an authenticated device on a box whose REQUIRED core
		// (account → name → network) isn't finished belongs in the /setup
		// wizard, not the app shell. /setup + /get-started live in the
		// (onboarding) route group with its own layout, so this can't loop.
		// Without this, a freshly-reinstalled box drops the user straight into
		// chat with account/naming undone.
		try {
			const setupRes = await fetch('/api/setup/state');
			if (setupRes.ok) {
				const setup = await setupRes.json();
				if (setup.setup_complete === false) {
					throw redirect(303, '/setup');
				}
			}
		} catch (e) {
			// Re-throw the redirect; swallow only network/parse errors so a
			// transient box blip never traps the user out of their app.
			if (e && typeof e === 'object' && 'status' in e) throw e;
		}

		// Fetch profile for user preferences and server status
		const profileResponse = await fetch('/api/profile');

		if (profileResponse.ok) {
			const profile = await profileResponse.json();

			// Note: Onboarding wizard redirect removed.
			// Users now see "Getting Started" in chat and "ServerProvisioning" overlay
			// if server_status is not 'ready'.

			return {
				session: sessionData,
				preferredName: profile.preferred_name || null,
				serverStatus: profile.server_status || 'ready',
				sessionExpires: sessionData.expires || null,
				profileTimezone: profile.timezone || null,
				onboardingStatus: profile.onboarding_status || 'active'
			};
		}

		return {
			session: sessionData,
			preferredName: null,
			serverStatus: 'ready', // Assume ready if profile fetch fails
			sessionExpires: sessionData.expires || null,
			profileTimezone: null,
			onboardingStatus: 'active'
		};
	} catch (error) {
		// Re-throw redirects
		if (error && typeof error === 'object' && 'status' in error) {
			throw error;
		}
		// Network errors or JSON parse errors - redirect to login
		console.error('[Layout] Auth check failed:', error);
		throw redirect(303, '/pair');
	}
};
