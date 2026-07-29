import { redirect } from '@sveltejs/kit';
import type { LayoutLoad } from './$types';

/** Degraded shell data for a transient box blip — keeps the app mounted. */
const OFFLINE_DATA = {
	session: null,
	preferredName: null,
	serverStatus: 'ready',
	sessionExpires: null,
	homeTimezone: null,
	onboardingStatus: 'active'
};

export const load: LayoutLoad = async ({ fetch, url }) => {
	// Check authentication via Rust auth API
	try {
		// One retry after a beat: on the mobile shell this fetch rides the iroh
		// loopback, and right after an app resume the parked endpoint may still
		// be rebuilding (~1-3s) — the first attempt can die in that window.
		let sessionResponse: Response;
		try {
			sessionResponse = await fetch('/auth/session');
		} catch {
			await new Promise((r) => setTimeout(r, 1500));
			sessionResponse = await fetch('/auth/session');
		}

		// Only a real rejection means unpaired. A 5xx / gateway failure is the
		// box or the transport being momentarily unavailable — keep the shell
		// (same philosophy as the setup-state probe below: a transient blip
		// must never trap the user out of their app).
		if (!sessionResponse.ok) {
			if (sessionResponse.status === 401 || sessionResponse.status === 403) {
				throw redirect(303, '/pair');
			}
			return OFFLINE_DATA;
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
		// (account → name; the rest of onboarding is optional, network is
		// informational) isn't finished belongs in the unified /setup flow, not
		// the app shell. /setup lives in the (onboarding) route group with its
		// own layout, so this can't loop. Without this, a freshly-reinstalled
		// box drops the user straight into chat with account/naming undone.
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
				homeTimezone: profile.home_timezone || null,
				onboardingStatus: profile.onboarding_status || 'active'
			};
		}

		return {
			session: sessionData,
			preferredName: null,
			serverStatus: 'ready', // Assume ready if profile fetch fails
			sessionExpires: sessionData.expires || null,
			homeTimezone: null,
			onboardingStatus: 'active'
		};
	} catch (error) {
		// Re-throw redirects
		if (error && typeof error === 'object' && 'status' in error) {
			throw error;
		}
		// Network / parse errors mean UNREACHABLE, not unpaired — redirecting
		// to /pair here strands a validly-paired device on the pairing screen
		// over a 2s transport blip (seen in the wild: theme switch re-ran this
		// load while the mobile shell's parked endpoint was mid-rebuild).
		console.error('[Layout] Auth check failed (treating as offline):', error);
		return OFFLINE_DATA;
	}
};
