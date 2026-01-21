import { redirect } from '@sveltejs/kit';
import type { LayoutLoad } from './$types';

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

		// Check onboarding status via profile API
		try {
			const profileResponse = await fetch('/api/profile');
			if (profileResponse.ok) {
				const profile = await profileResponse.json();

				// Redirect to home if user has already completed onboarding
				if (profile.onboarding_status === 'complete') {
					throw redirect(303, '/');
				}
			} else {
				console.warn('[Onboarding Layout] Profile API returned:', profileResponse.status);
			}
		} catch (profileError) {
			// Re-throw redirects
			if (profileError && typeof profileError === 'object' && 'status' in profileError) {
				throw profileError;
			}
			// Log but don't block onboarding if profile check fails
			console.warn('[Onboarding Layout] Profile check failed:', profileError);
		}

		return {
			session: sessionData
		};
	} catch (error) {
		// Re-throw redirects
		if (error && typeof error === 'object' && 'status' in error) {
			throw error;
		}
		// Network errors or JSON parse errors - redirect to login
		console.error('[Onboarding Layout] Auth check failed:', error);
		throw redirect(303, '/login');
	}
};
