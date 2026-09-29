/**
 * Setup's first half, before this device has a server: the account, finding
 * the server and opening it with its four words, its Wi-Fi, and pairing.
 * (agents/build/onboarding.md, "Setup".)
 *
 * WHERE IT RUNS. Only where the app itself carries this code and has a
 * radio. The iPhone's shell opens `/setup` from its own baked copy of the app
 * when it has no server, and carries on there after pairing (one origin, the
 * loopback). The Mac's shell does the same from the copy it now bakes
 * (`tauri.macos.conf.json`), but its server's app lives at another origin,
 * so after pairing the window hands over to the server's copy at `/setup`
 * (`handOff`), the same jump the connect page always made. A browser is
 * always past this half: nothing reaches one without a server.
 * In dev, `?radio=fake` walks it in a browser against a scripted server and
 * a scripted account, and the real dev server takes over after "pairing".
 *
 * SIGN-IN COMES FIRST (2026-09-27). The account grant has to cross the
 * Bluetooth session BEFORE pairing, so the account is settled before the
 * server is even looked for. Signing in is identity, not billing: atlas
 * makes the account for any email (`ensure_account`, "linking is identity,
 * not billing"), and paying is the Subscription step, after pairing, which
 * passes itself over when the account already pays.
 *
 * NOTHING HERE OUTLIVES THE TAB except the account session, which is kept
 * for this tab only: a reload mid-way drops the Bluetooth link anyway, and
 * the session is a bearer token.
 */
import { boxRadio, BoxRadioError, type NearbyBox, type SetupLink } from '$lib/tauri/boxRadio';
import { markInApp } from './inApp';

const ATLAS = 'https://atlas.virtues.com';
const SESSION_KEY = 'virtues-setup-account';
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

/**
 * The Mac (or any desktop shell) running from its own baked copy, not the
 * server's: its shell opens that copy only while it has no server. The shell
 * tells every page where its server's app lives (`__VIRTUES_BOX_URL__`), so
 * any other origin is the baked copy: `tauri://localhost` in a release,
 * tauri's own dev server under `tauri dev`. (Keying on `tauri:` alone missed
 * the dev server, and Setup went looking for a server that wasn't there.)
 */
function bakedDesktop(): boolean {
	if (typeof window === 'undefined') return false;
	const shell = window as unknown as Shell & { __TAURI_INTERNALS__?: unknown };
	if (!shell.__TAURI_INTERNALS__ || shell.__VIRTUES_MOBILE__ || !shell.__VIRTUES_BOX_URL__) return false;
	try {
		return new URL(shell.__VIRTUES_BOX_URL__).origin !== window.location.origin;
	} catch {
		return false;
	}
}

function isFake(): boolean {
	try {
		return import.meta.env.DEV && sessionStorage.getItem('virtues-radio') === 'fake';
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
	if (typeof window === 'undefined' || !boxRadio.available) return false;
	if (forgotThisLaunch()) return true;
	if (isFake()) {
		try {
			return sessionStorage.getItem(FAKE_PAIRED_KEY) !== '1';
		} catch {
			return true;
		}
	}
	const shell = window as unknown as Shell;
	// A desktop shell that still points pages at the box's copy (Windows and
	// Linux; they inject `__VIRTUES_BOX_URL__`) is unpaired only while it runs
	// its own copy. The Mac stopped injecting that on 2026-09-29: it always
	// runs its own copy, like the phone, and answers from the paired flag
	// below, like the phone. See agents/plan/local-ui-plan.md.
	if (!shell.__VIRTUES_MOBILE__ && shell.__VIRTUES_BOX_URL__) return bakedDesktop();
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

// ─── the account, at atlas ──────────────────────────────────────────────────

export class AccountError extends Error {
	constructor(
		readonly code: string,
		message: string
	) {
		super(message);
	}
}

async function atlas<T>(path: string, body: unknown, token?: string): Promise<T> {
	if (isFake()) return fakeAtlas<T>(path, body);
	const ctrl = new AbortController();
	const t = setTimeout(() => ctrl.abort(), 15000);
	let res: Response;
	try {
		res = await fetch(ATLAS + path, {
			method: 'POST',
			headers: {
				'content-type': 'application/json',
				...(token ? { authorization: `Bearer ${token}` } : {})
			},
			body: JSON.stringify(body ?? {}),
			signal: ctrl.signal
		});
	} catch {
		throw new AccountError(
			'transport',
			"Couldn't reach Virtues. Check this device's internet connection, then try again."
		);
	} finally {
		clearTimeout(t);
	}
	const data = await res.json().catch(() => ({}));
	if (!res.ok) {
		const e = (data as { error?: { code?: string; message?: string } }).error ?? {};
		throw new AccountError(e.code ?? String(res.status), sentence(e.message) || 'Something went wrong. Try again.');
	}
	return data as T;
}

function sentence(s?: string): string {
	const t = (s ?? '').trim();
	if (!t) return '';
	const c = t[0].toUpperCase() + t.slice(1);
	return /[.?!]$/.test(c) ? c : `${c}.`;
}

/** In a browser with `?radio=fake`: any email, code 123456. Never sends mail. */
async function fakeAtlas<T>(path: string, body: unknown): Promise<T> {
	await new Promise((r) => setTimeout(r, 600));
	const b = (body ?? {}) as { email?: string; code?: string };
	if (path === '/account/login') return { sent: true } as T;
	if (path === '/account/login/verify') {
		if (b.code !== '123456') throw new AccountError('invalid_code', "That code didn't match. Check the email and try again.");
		return { token: 'fake-session', entitled: false } as T;
	}
	if (path === '/init/grant') return { grant: 'fake-grant', expires_in: 600 } as T;
	throw new AccountError('not_found', 'Not in the fake.');
}

// ─── the state ──────────────────────────────────────────────────────────────

interface Account {
	email: string;
	token: string;
	entitled: boolean;
}

function readAccount(): Account | null {
	try {
		const raw = sessionStorage.getItem(SESSION_KEY);
		return raw ? (JSON.parse(raw) as Account) : null;
	} catch {
		return null;
	}
}

class PrePair {
	/** This device has no server yet, and finds and pairs one here. */
	active = $state(unpaired());
	account = $state<Account | null>(typeof window === 'undefined' ? null : readAccount());
	/** They chose to go on without an account (their own AI, or later). */
	withoutAccount = $state(false);
	box = $state<NearbyBox | null>(null);
	link = $state<SetupLink | null>(null);
	/** The words they typed, shown back to be saved. */
	words = $state('');
	/** The words are saved, or there were none to save. */
	wordsKept = $state(false);
	/** On a network: joined over Wi-Fi, or it was online already (ethernet). */
	online = $state(false);
	/** Paired, and Setup is moving on to the server's steps. The pairing
	 *  screen holds until it has; the Wi-Fi receipt flashed in between. */
	handingOver = $state(false);

	get accountSettled(): boolean {
		return !!this.account || this.withoutAccount;
	}
	get serverOpen(): boolean {
		return !!this.link && this.wordsKept;
	}

	async sendCode(email: string): Promise<void> {
		await atlas('/account/login', { email });
	}

	async verify(email: string, code: string): Promise<void> {
		const r = await atlas<{ token: string; entitled: boolean }>('/account/login/verify', { email, code });
		this.account = { email, token: r.token, entitled: !!r.entitled };
		this.withoutAccount = false;
		try {
			sessionStorage.setItem(SESSION_KEY, JSON.stringify(this.account));
		} catch {
			/* kept in memory for this page */
		}
	}

	/**
	 * A device joining a server someone already set up. Setup's first half
	 * only sets up NEW servers; joining (the QR from the other device on a
	 * phone, a code on a computer) is still the connect page's, which the
	 * app ships beside this copy; `#existing` opens it on the join itself.
	 * Not in the dev fake, which has no connect page.
	 */
	get canJoinExisting(): boolean {
		return this.active && !isFake();
	}
	joinExisting(): void {
		window.location.href = '/connect.html#existing';
	}

	goWithout(): void {
		this.withoutAccount = true;
	}

	/**
	 * This device just forgot its server (the recovery screen's "Pair again"
	 * or "Forget this server"): the first half runs again from here, on this
	 * launch, whatever the shell baked when it started.
	 */
	startOver(): void {
		markInApp(false);
		try {
			sessionStorage.setItem(FORGOT_KEY, '1');
			localStorage.removeItem(JUST_PAIRED_KEY);
		} catch {
			/* `active` below carries this page; a reload would ask again */
		}
		this.link = null;
		this.box = null;
		this.words = '';
		this.wordsKept = false;
		this.online = false;
		this.active = boxRadio.available;
	}

	/** A link the server opened, kept until pairing ends it. */
	opened(box: NearbyBox, link: SetupLink, words: string): void {
		this.box = box;
		this.link = link;
		this.words = words;
		this.wordsKept = !link.gated;
		this.online = box.state === 'online';
	}

	/** Back to looking: the link is dropped and nothing on the server changed. */
	async forget(): Promise<void> {
		const l = this.link;
		this.link = null;
		this.box = null;
		this.words = '';
		this.wordsKept = false;
		this.online = false;
		await l?.close().catch(() => {});
	}

	/**
	 * The account's grant, over the open link, so the server links itself once
	 * it is online. No account, no grant. Throws `AccountError` (atlas) or
	 * `BoxRadioError` (the server).
	 */
	async grant(): Promise<void> {
		if (!this.account || !this.link) return;
		const g = await atlas<{ grant: string }>('/init/grant', {}, this.account.token);
		await this.link.grant(g.grant);
	}

	/**
	 * On a computer, after pairing: wait for the server to answer on this
	 * machine's loopback, then open its own copy of the app at Setup.
	 * `carry` rides along in the address, for what this copy knows and the
	 * server's copy can't read (another origin, its own storage): how far
	 * through Welcome and the letter they are, and the theme they chose.
	 * 'away' once it has navigated, 'silent' when the server never answered
	 * (opening it anyway only showed the webview's own error page), and
	 * 'here' where there is nothing to hand to (the phone: one origin).
	 */
	async handOff(carry: Record<string, string> = {}): Promise<'away' | 'silent' | 'here'> {
		if (!bakedDesktop()) return 'here';
		const box = (window as unknown as Shell).__VIRTUES_BOX_URL__ ?? 'http://localhost:7117';
		try {
			const { invoke } = await import('@tauri-apps/api/core');
			// Pairing already starts the loopback; this makes sure of it.
			await invoke('install_helpers', { server: '' });
		} catch {
			/* an older shell: pairing started it */
		}
		const until = Date.now() + 60000;
		let answered = false;
		while (!answered && Date.now() < until) {
			try {
				answered = (await fetch(`${box}/api/box/health`, { cache: 'no-store' })).ok;
			} catch {
				/* not answering yet */
			}
			if (!answered) await new Promise((r) => setTimeout(r, 1500));
		}
		if (!answered) return 'silent';
		const q = new URLSearchParams(carry).toString();
		window.location.href = `${box}/setup${q ? `?${q}` : ''}`;
		return 'away';
	}

	/** Pair this device through the server's radio, then leave the first half. */
	async pair(): Promise<void> {
		if (!this.link) throw new BoxRadioError('not-found', 'Your server is no longer connected. Start again.');
		await this.link.pair();
		try {
			if (isFake()) sessionStorage.setItem(FAKE_PAIRED_KEY, '1');
			else localStorage.setItem(JUST_PAIRED_KEY, 'true');
			sessionStorage.removeItem(FORGOT_KEY);
		} catch {
			/* the plugin holds the pairing; this only spares a relaunch */
		}
		await this.link.close().catch(() => {});
		// A computer stays on this half until it hands over (`handOff`): the
		// server's app is at another origin, and this copy can't read it.
		if (!bakedDesktop()) {
			this.handingOver = true;
			this.link = null;
			this.active = false;
		}
	}
}

export const prePair = new PrePair();
