import { redirect } from '@sveltejs/kit';
import type { LayoutLoad } from './$types';
import { boxBelowFloor } from '$lib/boxApi';
import type { SetupState } from '$lib/api/client';
import { setupStateStore } from '$lib/stores/setupState.svelte';
import { sidebarDoors } from '$lib/stores/sidebarDoors.svelte';
import { units } from '$lib/stores/units.svelte';
import { markStartup } from '$lib/perf/startup';

/** Degraded shell data for a transient box blip — keeps the app mounted. */
const OFFLINE_DATA = {
	session: null,
	preferredName: null,
	sessionExpires: null,
	homeTimezone: null,
	onboardingStatus: 'active'
};

/** The shell's data, timed for the startup report (lib/perf/startup.ts). */
export const load: LayoutLoad = async (event) => {
	markStartup('load-start');
	try {
		return await loadShell(event);
	} finally {
		markStartup('load-end');
	}
};

const loadShell: LayoutLoad = async ({ fetch, url }) => {
	// Check authentication via Rust auth API
	try {
		// PARALLEL, with the session. Setup state, the version floor and the
		// profile don't depend on each other or on the session's answer, so all
		// four start at once and are read in gate order below. Waiting for the
		// session first cost a round trip before the shell, and each one awaited
		// in turn cost another; on the relay those are most of a cold start. A
		// gate that redirects leaves the rest unread, and an unpaired device's
		// 401s are discarded with them. Setup state and the profile are asked
		// again after the session when the early answer came back empty: right
		// after the phone resumes, the first attempts can die with the session's
		// while the parked endpoint rebuilds (the session retries below).
		const askSetup = async () => {
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
			return setup;
		};
		const oauth = url.pathname.startsWith('/oauth/');
		const setupP = oauth ? Promise.resolve(null) : askSetup();
		const floorP = oauth ? Promise.resolve(false) : boxBelowFloor(fetch);
		const profileP: Promise<Response | null> = oauth ? Promise.resolve(null) : fetch('/api/profile').catch(() => null);
		// Mark it handled now: a redirect below returns before it is awaited.
		floorP.catch(() => {});

		// One retry after a beat: on the mobile shell this fetch rides the iroh
		// loopback, and right after an app resume the parked endpoint may still
		// be rebuilding (~1-3s) — the first attempt can die in that window.
		let sessionResponse: Response;
		let sessionRetried = false;
		try {
			sessionResponse = await fetch('/auth/session');
		} catch {
			sessionRetried = true;
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
		if (oauth) {
			return { session: sessionData };
		}

		// TWO GATES, because there are two different kinds of "not ready".
		//
		// SETUP is the server coming up: claimed by its first device. A server
		// that hasn't been claimed can't run the app at all.
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
		// /setup lives in the (onboarding) route group with its own layout,
		// so neither redirect can loop.
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
			// Asked again only when the session itself had to retry: then the
			// early attempts may have died with its first try. A setup state
			// that fails on its own has had its three tries already.
			const setup = (await setupP) ?? (sessionRetried ? await askSetup() : null);
			// The shell's setup store and sidebar would ask again on mount;
			// hand them these answers instead (see `seed` on each).
			if (setup) setupStateStore.seed(setup as SetupState);
			if (setup) {
				// Gate on onboarding_status ONLY, never on setup_complete. Setup
				// is the claim; whether the person has finished or dismissed the
				// rest of Setup is onboarding_status.
				// Connecting AI is the getting-started room's first step now.
				// `active` covers both finished and dismissed, which is the whole
				// reason it replaced a separate skipped flag.
				// A status of `onboarding` opens Setup on its own, whatever the
				// record already holds — it is what `skip-onboarding
				// {skipped:false}` sets, and the only way to walk Setup again on
				// a box that has a source. Setup's close turns it `active`, so
				// this cannot trap anyone.
				if (
					setup.onboarding_status === 'onboarding' ||
					(setup.onboarding_complete === false && setup.onboarding_status !== 'active')
				) {
					throw redirect(303, '/onboarding');
				}
			}
		} catch (e) {
			// Re-throw the redirect; swallow only network/parse errors so a
			// transient box blip never traps the user out of their app.
			if (e && typeof e === 'object' && 'status' in e) throw e;
		}

		// The server is too old for this copy of the app: one clear screen
		// rather than a half-working app. See $lib/boxApi.ts.
		if (await floorP) throw redirect(303, '/server-update');

		// Fetch profile for user preferences
		let profileResponse = await profileP;
		if (!profileResponse?.ok && sessionRetried) profileResponse = await fetch('/api/profile');

		if (profileResponse?.ok) {
			const profile = await profileResponse.json();
			sidebarDoors.seed(profile);
			units.seed(profile);

			// (Onboarding redirects live above, off /api/setup/state. This fetch
			// only feeds preferences.)
			return {
				session: sessionData,
				preferredName: profile.preferred_name || null,
				sessionExpires: sessionData.expires || null,
				homeTimezone: profile.home_timezone || null,
				onboardingStatus: profile.onboarding_status || 'active'
			};
		}

		return {
			session: sessionData,
			preferredName: null,
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
