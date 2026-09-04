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

		// TWO GATES, because there are two different kinds of "not ready".
		//
		// SETUP is the box coming up — claimed, and (on an appliance) linked to
		// an account. A box that hasn't done that can't run the app at all.
		//
		// ONBOARDING is the box becoming worth having — something connected, so
		// it has a life to keep a record of. It used to gate nothing: the state
		// endpoint has modelled these steps all along, but `setup_complete`
		// flips true the instant you pair and link, so the shell swallowed
		// everyone the moment setup finished and the chapters after it were
		// written, reachable only by typing the URL, and never seen (2026-08-13).
		//
		// Skipping is honoured and remembered. "Prescribe, never enforce" means
		// the second gate must be a door, not a wall — and a door that asks
		// again every launch is a wall with extra steps.
		//
		// /founders-letter lives in the (onboarding) route group with its own
		// layout, so neither redirect can loop.
		// RETRY, because "couldn't ask" is not "nothing to do".
		//
		// This runs the instant the desktop app hands over after pairing, when
		// the box has just finished writing a device row and the loopback proxy
		// is seconds old. A 502 or a thrown fetch used to fall straight through
		// to the shell — the same silent pass as a satisfied gate — so a
		// freshly-paired owner landed in an empty chat instead of onboarding,
		// and only a manual reload revealed it (seen live 2026-08-13).
		//
		// Three quick tries, then give up and continue. Onboarding is worth a
		// second of patience; it is never worth locking someone out of their
		// own app over a box that blipped.
		try {
			let setup: {
				setup_complete?: boolean;
				onboarding_complete?: boolean;
				onboarding_status?: string;
			} | null = null;
			for (let i = 0; i < 3 && !setup; i++) {
				if (i > 0) await new Promise((r) => setTimeout(r, 400));
				try {
					const setupRes = await fetch('/api/setup/state');
					if (setupRes.ok) setup = await setupRes.json();
				} catch {
					// keep trying
				}
			}
			if (setup) {
				// Gate on onboarding_status ONLY — never on setup_complete. On an
				// appliance, setup_complete also requires the linked account, and
				// the airlock's account step is skippable: gating the shell on it
				// bounced "Enter Virtues" straight back to the letter forever,
				// with AccountGate (the remedy) stranded behind the wall it was
				// meant to open. The account is Home's getting-started business.
				// `active` covers both finished and dismissed, which is the whole
				// reason it replaced a separate skipped flag.
				if (setup.onboarding_complete === false && setup.onboarding_status !== 'active') {
					throw redirect(303, '/founders-letter');
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

			// (Onboarding redirects live above, off /api/setup/state. This fetch
			// only feeds preferences and the ServerProvisioning overlay, which
			// +layout.svelte shows while server_status is not 'ready'.)
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
