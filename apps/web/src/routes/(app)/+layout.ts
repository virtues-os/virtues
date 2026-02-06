import { redirect } from '@sveltejs/kit';
import type { LayoutLoad } from './$types';

export const load: LayoutLoad = async ({ fetch, url }) => {
	// Check authentication via Rust auth API
	try {
		const sessionResponse = await fetch('/auth/session');

		// Treat any non-OK response as unauthenticated
		if (!sessionResponse.ok) {
			throw redirect(303, '/login');
		}

		const sessionData = await sessionResponse.json();

		// Redirect to login if not authenticated
		if (!sessionData.user) {
			throw redirect(303, '/login');
		}

		// Skip profile check for OAuth callback (it handles its own redirect)
		if (url.pathname.startsWith('/oauth/')) {
			return { session: sessionData };
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
				sessionExpires: sessionData.expires || null
			};
		}

		return {
			session: sessionData,
			preferredName: null,
			serverStatus: 'ready', // Assume ready if profile fetch fails
			sessionExpires: sessionData.expires || null
		};
	} catch (error) {
		// Re-throw redirects
		if (error && typeof error === 'object' && 'status' in error) {
			throw error;
		}
		// Network errors or JSON parse errors - redirect to login
		console.error('[Layout] Auth check failed:', error);
		throw redirect(303, '/login');
	}
};
