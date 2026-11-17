/**
 * Setup's first half, before this device has a server: find the server on
 * the network, or take its address, and pair with the code `virtues pair`
 * prints. (agents/plan/onboarding-diy-plan.md.)
 *
 * WHERE IT RUNS. Only where the app itself carries this code. The iPhone's
 * and the Mac's shells open `/onboarding` from their own copy of the app when
 * they have no server, and carry on there after pairing: the server is data
 * to them, reached through this device's loopback. A browser is always past
 * this half: nothing reaches one without a server.
 * In dev, `?radio=fake` walks it in a browser, and the real dev server takes
 * over after "pairing". `&nearby=diy` puts one server on the fake network;
 * `&nearby=none` finds nothing.
 *
 * No account is asked for here. Pairing and collecting need none; the
 * subscription step signs in, after this device's own step.
 *
 * NOTHING HERE OUTLIVES THE TAB except the markers below.
 */
import { serversOnNetwork, type NetworkServer } from '$lib/tauri/bridge';
import { isTauri } from '$lib/utils/platform';

const FAKE_KEY = 'virtues-radio';
const FAKE_NEARBY_KEY = 'virtues-fake-nearby';
const FAKE_PAIRED_KEY = 'virtues-fake-paired';
/** The app's own record that this launch just paired, which the next launch's
 *  baked flag supersedes (`__VIRTUES_PAIRED__`). */
const JUST_PAIRED_KEY = 'virtues-just-paired';
/** This device forgot its server during this launch (`startOver`). The
 *  shell's `__VIRTUES_PAIRED__` was baked at launch and still says true, so
 *  without this the flow would go looking for a server that is no longer
 *  there. Per launch: the next one bakes the truth. */
const FORGOT_KEY = 'virtues-forgot-server';

type Shell = { __VIRTUES_MOBILE__?: boolean; __VIRTUES_PAIRED__?: boolean; __VIRTUES_BOX_URL__?: string };

/** The dev fake, for this tab: `?radio=fake` on, `?radio=off` off. */
function isFake(): boolean {
	if (typeof window === 'undefined' || !import.meta.env.DEV) return false;
	try {
		const q = new URLSearchParams(window.location.search);
		if (q.get('radio') === 'fake') sessionStorage.setItem(FAKE_KEY, 'fake');
		if (q.get('radio') === 'off') sessionStorage.removeItem(FAKE_KEY);
		const nearby = q.get('nearby');
		if (nearby === 'diy' || nearby === 'none') sessionStorage.setItem(FAKE_NEARBY_KEY, nearby);
		return sessionStorage.getItem(FAKE_KEY) === 'fake';
	} catch {
		return false;
	}
}

/** Whether this page can find and pair a server: the app's own copy, or the dev fake. */
function canPairHere(): boolean {
	return isTauri || isFake();
}

/**
 * Windows and Linux still show the server's copy of the app, through the
 * loopback, and pair on the connect page; their pages are never unpaired.
 * TODO(2026-11-15): drop this when they carry their own copy
 * (onboarding-diy-plan.md, slice 3).
 */
function showsServersCopy(): boolean {
	if (typeof window === 'undefined') return false;
	const shell = window as unknown as Shell;
	if (shell.__VIRTUES_MOBILE__ || !shell.__VIRTUES_BOX_URL__) return false;
	try {
		return new URL(shell.__VIRTUES_BOX_URL__).origin === window.location.origin;
	} catch {
		return false;
	}
}

function forgotThisLaunch(): boolean {
	try {
		return sessionStorage.getItem(FORGOT_KEY) === '1';
	} catch {
		return false;
	}
}

/** Whether this device still has to find and pair a server here. */
function unpaired(): boolean {
	if (typeof window === 'undefined' || !canPairHere()) return false;
	if (forgotThisLaunch()) return true;
	if (isFake()) {
		try {
			return sessionStorage.getItem(FAKE_PAIRED_KEY) !== '1';
		} catch {
			return true;
		}
	}
	if (showsServersCopy()) return false;
	const shell = window as unknown as Shell;
	if (shell.__VIRTUES_PAIRED__ === true) {
		// The baked flag has caught up with the pairing the marker bridged.
		// Dropping it here means a later unpair, from anywhere, can't be
		// outvoted by a marker from a launch long gone.
		try {
			localStorage.removeItem(JUST_PAIRED_KEY);
		} catch {
			/* nothing to drop */
		}
		return false;
	}
	try {
		return localStorage.getItem(JUST_PAIRED_KEY) !== 'true';
	} catch {
		return true;
	}
}

class PrePair {
	/** This device has no server yet, and finds and pairs one here. */
	active = $state(unpaired());

	/**
	 * A phone joining a server someone already uses, by scanning the code a
	 * paired computer shows, or the one the server printed. That scanner is
	 * the connect page's, which the app ships beside this copy; `#existing`
	 * opens it on the scan. Not in the dev fake, which has no connect page.
	 */
	get canJoinExisting(): boolean {
		return this.active && !isFake();
	}
	joinExisting(): void {
		window.location.href = '/connect.html#existing';
	}

	/** Servers on this device's network. The dev fake has one with `&nearby=diy`. */
	async serversOnNetwork(): Promise<NetworkServer[]> {
		if (!isFake()) return serversOnNetwork();
		await new Promise((r) => setTimeout(r, 1200));
		let nearby: string | null = null;
		try {
			nearby = sessionStorage.getItem(FAKE_NEARBY_KEY);
		} catch {
			/* nothing nearby */
		}
		return nearby === 'diy' ? [{ name: 'virtues', origin: 'http://192.168.1.20:8000', claimed: false }] : [];
	}

	/**
	 * Pair with a server by its address and the code `virtues pair` printed,
	 * here in Setup, then leave the first half. Throws what the reach plugin
	 * threw; `pairError` (pairing.ts) puts it in words.
	 */
	async pairByAddress(address: string, code: string): Promise<void> {
		if (isFake()) {
			await new Promise((r) => setTimeout(r, 900));
			sessionStorage.setItem(FAKE_PAIRED_KEY, '1');
		} else {
			const { invoke } = await import('@tauri-apps/api/core');
			const status = await invoke<{ paired?: boolean }>('plugin:reach|pair', {
				payload: { server: address, code: code.replace(/\s+/g, '') },
			});
			if (!status?.paired) throw new Error('Pairing did not complete.');
			const shell = window as unknown as Shell;
			if (!shell.__VIRTUES_MOBILE__) {
				// Pairing starts this computer's connection to the server; this
				// makes sure of it before Setup reads from the server.
				await invoke('install_helpers', { server: '' }).catch(() => {});
			}
			try {
				localStorage.setItem(JUST_PAIRED_KEY, 'true');
				sessionStorage.removeItem(FORGOT_KEY);
			} catch {
				/* the plugin holds the pairing; this only spares a relaunch */
			}
		}
		this.active = false;
	}

	/**
	 * This device just forgot its server (the recovery screen's "Pair again"
	 * or "Forget this server"): the first half runs again from here, on this
	 * launch, whatever the shell baked when it started.
	 */
	startOver(): void {
		try {
			sessionStorage.setItem(FORGOT_KEY, '1');
			localStorage.removeItem(JUST_PAIRED_KEY);
		} catch {
			/* `active` below carries this page; a reload would ask again */
		}
		this.active = canPairHere();
	}
}

export const prePair = new PrePair();
