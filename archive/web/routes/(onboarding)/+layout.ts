import { redirect } from '@sveltejs/kit';
import type { LayoutLoad } from './$types';

/**
 * DEPRECATED: Onboarding Wizard Layout
 * 
 * This layout is preserved for reference but the onboarding wizard has been
 * replaced with:
 * - Provisioning-time data collection (billing, email, name)
 * - Tollbooth profile hydration on container boot
 * - "Getting Started" checklist in the chat UI
 * 
 * If a user somehow lands here, redirect them to the main app.
 */
export const load: LayoutLoad = async ({ fetch }) => {
	// Check auth via Rust API
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

		// DEPRECATED: Onboarding wizard is no longer used.
		// Always redirect to home - users will see "Getting Started" in chat.
		throw redirect(303, '/');
	} catch (error) {
		// Re-throw redirects
		if (error && typeof error === 'object' && 'status' in error) {
			throw error;
		}
		// Log but don't block onboarding if profile check fails
		console.warn('[Onboarding Layout] Auth check failed:', error);
		throw redirect(303, '/login');
	}
};
