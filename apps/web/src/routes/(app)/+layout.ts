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

		// Skip onboarding redirect for OAuth callback (it handles its own redirect)
		if (url.pathname.startsWith('/oauth/')) {
			return { session: sessionData };
		}

		// Check onboarding status via Rust profile API
		const profileResponse = await fetch('/api/profile');

		if (profileResponse.ok) {
			const profile = await profileResponse.json();

			// Redirect to onboarding if status is not 'complete'
			if (profile.onboarding_status !== 'complete') {
				// Allow access to onboarding pages
				if (!url.pathname.startsWith('/onboarding')) {
					throw redirect(303, '/onboarding/welcome');
				}
			}

			return {
				session: sessionData,
				preferredName: profile.preferred_name || null,
				sessionExpires: sessionData.expires || null
			};
		}

		return {
			session: sessionData,
			preferredName: null,
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
