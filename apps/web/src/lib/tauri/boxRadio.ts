/**
 * The box's radio: talking to a server over Bluetooth, from inside the app.
 *
 * ONE door for every screen that puts a server on a network. Two moments need
 * it, and they are the same screen with a different key:
 *
 *   - **Setup.** A new, unclaimed server. You prove you are standing at it with
 *     the four words on its panel (`openSetup`), then pick its wifi, link it,
 *     and pair — all over one Bluetooth session.
 *   - **Moved.** A server you already own woke up somewhere its wifi is not (the
 *     office, a new flat, a changed password). It has no internet, so nothing
 *     can reach it except the radio. It advertises again, and THIS device
 *     proves it is one of its paired devices by signing a challenge with the
 *     key the server already trusts (`openOwner`). No words, no panel.
 *
 * The Wi-Fi step is written against `BoxWifiLink` and never learns which of
 * the two it is in. That is the whole contract: a step receives a link, and
 * whoever opened it decided how it was authorized.
 *
 * Under this sits the reach plugin (`plugins/reach`), whose commands have two
 * implementations — Swift CoreBluetooth on iOS, `virtues_improv::client`
 * (btleplug) on desktop — behind one command surface. Nothing outside this
 * file should `invoke('plugin:reach|improv_*')` directly.
 *
 * In a plain browser there is no radio. `boxRadio.available` is false and every
 * call throws `unavailable` — unless the dev fake is switched on with
 * `?radio=fake` (sticky for the tab), which walks a scripted server through
 * the same states so the flow can be built at localhost.
 */

import { isIOS, isLinux, isMacOS, isTauri, isWindows } from '$lib/utils/platform';
import { shellSupports } from './bridge';

// ─── the shape a screen sees ────────────────────────────────────────────────

/**
 * What a server's advertisement says about it, read WITHOUT connecting.
 *
 * - `needs-wifi`  — unclaimed, offline: the setup case.
 * - `online`      — unclaimed, already online (ethernet, or wifi already set).
 * - `needs-owner` — claimed, and offline long enough to ask for help: the
 *                   moved case. Only one of its paired devices can open it.
 * - `unknown`     — an advertisement we could not read; treat as `needs-wifi`.
 */
export type BoxRadioState = 'needs-wifi' | 'online' | 'needs-owner' | 'unknown';

export interface NearbyBox {
	/** Opaque handle, valid until the next `discover`. Never parse it. */
	id: string;
	/** The server's radio name, e.g. `Virtues-4812` (older ones: `Virtues-Quaint-Tern`). */
	name: string;
	/** The same name for people: `Virtues 4812`, or `Your server`. Show this. */
	label: string;
	/** Signal strength; closest first is how a person breaks a tie. */
	rssi: number;
	state: BoxRadioState;
}

/** One network from the SERVER's own scan — what it can see, not the phone. */
export interface WifiNetwork {
	ssid: string;
	signal: number;
	secured: boolean;
	/** 802.1X: needs a username as well, so the step shows two fields. */
	enterprise: boolean;
}

export interface WifiCredentials {
	ssid: string;
	/** Empty for an open network. */
	password: string;
	/** 802.1X username. Present routes the join over the enterprise command. */
	identity?: string;
}

/** Live progress of a join, in order. The resolved promise is the truth. */
export type JoinStage = 'sent' | 'joining' | 'joined';

export type BoxRadioErrorCode =
	/** No radio here: a browser, Android, Bluetooth off or not permitted. */
	| 'unavailable'
	/** This app is older than the command (the shell predates it). */
	| 'unsupported'
	/** No server answering nearby, or the one picked has gone out of range. */
	| 'not-found'
	/** The server refused: wrong words, or this device is not one of its own. */
	| 'refused'
	/** The server tried the network and could not join (usually the password). */
	| 'join-failed'
	/** The server stopped answering mid-conversation. */
	| 'timeout'
	| 'failed';

export class BoxRadioError extends Error {
	constructor(
		readonly code: BoxRadioErrorCode,
		message: string
	) {
		super(message);
		this.name = 'BoxRadioError';
	}
}

/**
 * An open Bluetooth conversation with one server, authorized to change its
 * network. The Wi-Fi step takes one of these as a prop and nothing else.
 */
export interface BoxWifiLink {
	readonly box: NearbyBox;
	/** How the link was opened. For copy only — a step must not branch on it. */
	readonly purpose: 'setup' | 'owner';
	/** Networks the server can see. Throws rather than returning `[]` on failure. */
	scan(): Promise<WifiNetwork[]>;
	/**
	 * Put the server on a network and watch it happen. Resolves once it has
	 * joined, with the address it now answers on. Rejects `join-failed` on a
	 * wrong password — the link stays open, so the step can simply ask again.
	 */
	join(creds: WifiCredentials, onStage?: (stage: JoinStage) => void): Promise<{ url: string }>;
	/** Drop the connection. Always safe; call it when the step unmounts. */
	close(): Promise<void>;
}

/** The setup link can do the two things only a brand-new server allows. */
export interface SetupLink extends BoxWifiLink {
	readonly purpose: 'setup';
	/**
	 * False when the server's firmware predates the four-word gate and asked
	 * for nothing — there are then no words for the owner to save.
	 */
	readonly gated: boolean;
	/** Hand over a pre-approved account grant; the server links itself later. */
	grant(grant: string): Promise<void>;
	/** Pair THIS device through the server's radio. Resolves with reach status. */
	pair(): Promise<unknown>;
}

export interface BoxRadio {
	/** Whether this app has a radio to use at all. Check before offering it. */
	readonly available: boolean;
	/** Listen for servers for a few seconds; closest first. */
	discover(opts?: { seconds?: number }): Promise<NearbyBox[]>;
	/**
	 * Open a new server WITHOUT words, if it will let you: firmware older than
	 * the four-word gate asks for nothing. Resolves `null` for every modern
	 * server — then ask for the words and call `openSetup`. Costs a modern
	 * server none of its ten attempts. Call once, before showing the words.
	 */
	tryOpenWithoutWords(box: NearbyBox): Promise<SetupLink | null>;
	/** Open a new server with the four words on its panel. */
	openSetup(box: NearbyBox, phrase: string): Promise<SetupLink>;
	/**
	 * Open a server this device is already paired with. With no `box`, listens
	 * and tries each `needs-owner` server in range, closest first, and opens
	 * the first one that accepts this device — a stranger's server refuses, so
	 * a crowded office still lands on the right one.
	 */
	openOwner(box?: NearbyBox): Promise<BoxWifiLink>;
}

// ─── the command surface this needs ─────────────────────────────────────────

/**
 * `COMMAND_SURFACE_VERSION` that added `improv_owner_claim`. Keep in step with
 * the table in `src-tauri/src/lib.rs`.
 */
export const OWNER_CLAIM_SURFACE = 5;

// ─── shared helpers ─────────────────────────────────────────────────────────

/**
 * `Virtues-4812` → `Virtues 4812`, the way the server's own screen says it.
 * Idempotent. A server named by its number keeps the brand, since "4812"
 * alone reads as nothing (2026-09-28, when the number replaced the codename);
 * an older server's codename drops it (`Virtues-Quaint-Tern` → `Quaint
 * Tern`). Strips stray punctuation some Bluetooth stacks hand back (a leading
 * `[` was seen live, 2026-08-13) and the client's own `Virtues box` fallback,
 * which would otherwise read as "Setting up box".
 */
export function boxLabel(name: string | null | undefined): string {
	const rest = String(name || '')
		.replace(/^[^A-Za-z0-9]+|[^A-Za-z0-9]+$/g, '')
		.replace(/^Virtues[-_ ]?/i, '')
		.replace(/[-_]+/g, ' ')
		.replace(/^box$/i, '')
		.trim();
	if (/^\d+$/.test(rest)) return `Virtues ${rest}`;
	return rest || 'Your server';
}

/**
 * How long each call may take before the screen gets its answer back. Every
 * Bluetooth call needs a floor: a stale peripheral can hang past every timeout
 * the native side has, and a screen awaiting it has no way out. Each is set
 * ABOVE the native client's own bound, so the native answer (with its real
 * reason) wins whenever there is one.
 */
export const DEADLINE_MS = {
	claim: 30_000,
	scan: 35_000,
	/** A join is bounded by the server's `nmcli` plus the client's 45s. */
	join: 60_000,
	grant: 30_000,
	/** The server may take 45s + 15s; the clients wait 70s. */
	pair: 80_000,
	owner: 45_000
} as const;

function withDeadline<T>(p: Promise<T>, ms: number, onTimeout?: () => void): Promise<T> {
	let timer: ReturnType<typeof setTimeout>;
	const deadline = new Promise<never>((_, reject) => {
		timer = setTimeout(() => {
			onTimeout?.();
			reject(new BoxRadioError('timeout', "Your server didn't answer. Move closer to it and try again."));
		}, ms);
	});
	return Promise.race([p, deadline]).finally(() => clearTimeout(timer));
}

// ─── the real radio (Tauri) ─────────────────────────────────────────────────

type Invoke = <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;

async function tauriInvoke(): Promise<Invoke> {
	const { invoke } = await import('@tauri-apps/api/core');
	return invoke as Invoke;
}

function stateFromByte(b: number): BoxRadioState {
	// Improv's state byte, as `virtues-improv::protocol::State` defines it.
	switch (b) {
		case 0x01:
			return 'needs-owner';
		case 0x02:
			return 'needs-wifi';
		case 0x04:
			return 'online';
		default:
			return 'unknown';
	}
}

/**
 * A command that rejected outright. Tauri's own refusals for a command the
 * installed app does not have ("Command x not found", "not allowed by ACL")
 * mean a stale app — matched narrowly, because a plain "not found" in some
 * other error must not read as "update the app".
 */
function fromThrown(e: unknown): BoxRadioError {
	if (e instanceof BoxRadioError) return e;
	const msg = e instanceof Error ? e.message : String(e);
	if (/command \S+ not found|not allowed by acl/i.test(msg)) {
		return new BoxRadioError('unsupported', 'Update the app to use this.');
	}
	if (/android/i.test(msg)) return new BoxRadioError('unavailable', msg);
	return new BoxRadioError('failed', msg);
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
	const invoke = await tauriInvoke();
	try {
		return await invoke<T>(`plugin:reach|${cmd}`, args);
	} catch (e) {
		throw fromThrown(e);
	}
}

type Outcome = { ok: boolean; error?: string; code?: string };

const CODES: readonly BoxRadioErrorCode[] = [
	'unavailable',
	'unsupported',
	'not-found',
	'refused',
	'join-failed',
	'timeout',
	'failed'
];

/**
 * Commands answer `{ok:false, code, error}` rather than rejecting; lift that.
 * `code` comes from the native side (Rust `FailureKind`, Swift
 * `ImprovFailure`) and is trusted as-is. An app built before codes existed
 * sends none, and then the answer is `failed` — never a guess at `refused`,
 * which a screen would show as "wrong words" for a Bluetooth hiccup.
 */
function check<T extends Outcome>(r: T): T {
	if (r.ok) return r;
	const code = CODES.includes(r.code as BoxRadioErrorCode) ? (r.code as BoxRadioErrorCode) : 'failed';
	throw new BoxRadioError(code, r.error ?? "Your server couldn't do that.");
}

/** Drop the connection without caring whether that works. */
function hangUp(): void {
	void call('improv_disconnect').catch(() => {});
}

function tauriLink(box: NearbyBox, purpose: 'setup' | 'owner'): BoxWifiLink {
	return {
		box,
		purpose,
		async scan() {
			const r = await withDeadline(
				call<{ networks: WifiNetwork[]; code?: string; error?: string }>('improv_wifi_scan', {
					id: box.id
				}),
				DEADLINE_MS.scan,
				hangUp
			);
			// An error is a failed scan; an empty list WITHOUT one is an answer
			// ("no networks here"), and the step says something different for it.
			if (r.error) check({ ok: false, code: r.code ?? 'timeout', error: r.error });
			// iOS omits `enterprise` on older builds; default it rather than
			// letting `undefined` leak into the step.
			return r.networks.map((n) => ({ ...n, enterprise: Boolean(n.enterprise) }));
		},
		async join(creds, onStage) {
			let unlisten: (() => void) | undefined;
			if (onStage) {
				try {
					const { addPluginListener } = await import('@tauri-apps/api/core');
					const l = await addPluginListener<{ stage: JoinStage }>(
						'reach',
						'improv-progress',
						(p) => p?.stage && onStage(p.stage)
					);
					unlisten = () => void l.unregister();
				} catch {
					// Progress is cosmetic; the resolution below is the truth.
				}
			}
			try {
				const r = await withDeadline(
					call<Outcome & { url?: string }>('improv_provision', {
						id: box.id,
						ssid: creds.ssid,
						password: creds.password,
						identity: creds.identity || null
					}),
					DEADLINE_MS.join
				);
				check(r);
				return { url: r.url ?? '' };
			} finally {
				unlisten?.();
			}
		},
		async close() {
			await call('improv_disconnect').catch(() => {});
		}
	};
}

function setupLink(box: NearbyBox, gated: boolean): SetupLink {
	return {
		...tauriLink(box, 'setup'),
		purpose: 'setup',
		gated,
		async grant(grant) {
			check(
				await withDeadline(call<Outcome>('improv_grant', { id: box.id, grant }), DEADLINE_MS.grant, hangUp)
			);
		},
		async pair() {
			const p = check(
				await withDeadline(
					call<Outcome & { status?: unknown }>('improv_pair', { id: box.id }),
					DEADLINE_MS.pair,
					hangUp
				)
			);
			return p.status;
		}
	};
}

const tauriRadio: BoxRadio = {
	available: isTauri && (isIOS || isMacOS || isWindows || isLinux),

	async discover(opts) {
		const r = await call<{ boxes: Array<Omit<NearbyBox, 'state'> & { improvState: number }>; error?: string }>(
			'improv_discover',
			{ seconds: opts?.seconds ?? 4 }
		);
		// A scan that FAILED is not a scan that found nothing — say which.
		if (r.boxes.length === 0 && r.error) throw new BoxRadioError('unavailable', r.error);
		return r.boxes.map(({ improvState, ...b }) => ({
			...b,
			label: boxLabel(b.name),
			state: stateFromByte(improvState)
		}));
	},

	async tryOpenWithoutWords(box) {
		// "-" is the probe, not "": the wire rejects an empty phrase outright,
		// while a gated server normalizes "-" to nothing and refuses it BEFORE
		// spending any of its ten attempts per 15 minutes.
		try {
			const r = await withDeadline(
				call<Outcome & { gated?: boolean }>('improv_claim', { id: box.id, phrase: '-' }),
				DEADLINE_MS.claim,
				hangUp
			);
			return r.ok && r.gated === false ? setupLink(box, false) : null;
		} catch {
			// The words screen reports any real problem when they are sent.
			return null;
		}
	},

	async openSetup(box, phrase) {
		const r = check(
			await withDeadline(
				call<Outcome & { gated?: boolean }>('improv_claim', { id: box.id, phrase }),
				DEADLINE_MS.claim,
				hangUp
			)
		);
		return setupLink(box, r.gated !== false);
	},

	async openOwner(box) {
		if (!(await shellSupports(OWNER_CLAIM_SURFACE))) {
			throw new BoxRadioError('unsupported', 'Update the app to reconnect your server.');
		}
		const candidates = box
			? [box]
			: (await tauriRadio.discover({ seconds: 5 })).filter((b) => b.state === 'needs-owner');
		if (candidates.length === 0) {
			throw new BoxRadioError('not-found', 'No server nearby is asking for its owner.');
		}
		let last: BoxRadioError | undefined;
		for (const c of candidates) {
			try {
				check(
					await withDeadline(
						call<Outcome>('improv_owner_claim', { id: c.id }),
						DEADLINE_MS.owner,
						hangUp
					)
				);
				return tauriLink(c, 'owner');
			} catch (e) {
				last = fromThrown(e);
				// A refusal means "not yours": try the next one. So does a
				// server that went quiet or out of range. Anything else (no
				// pairing on this device, a stale app) will not change.
				if (!['refused', 'timeout', 'not-found'].includes(last.code)) throw last;
				hangUp();
			}
		}
		throw last ?? new BoxRadioError('not-found', 'No server nearby is asking for its owner.');
	}
};

// ─── the dev fake ───────────────────────────────────────────────────────────

export interface FakeRadioOptions {
	/** Which server is nearby. Default: one new server needing wifi. */
	boxes?: NearbyBox[];
	/** The phrase `openSetup` accepts. Default `mango-burly-skull-dough`. */
	phrase?: string;
	/**
	 * A password that fails the join, to exercise the retry. Default
	 * `wrongpassword`: 8 or more characters, because the Wi-Fi step never
	 * sends a shorter one to a WPA network (it was `wrong`, which no screen
	 * could send).
	 */
	badPassword?: string;
	/** Scale every delay; 0 for tests. Default 1. */
	speed?: number;
}

const FAKE_NETWORKS: WifiNetwork[] = [
	{ ssid: 'Home', signal: -41, secured: true, enterprise: false },
	{ ssid: 'Home 5G', signal: -48, secured: true, enterprise: false },
	{ ssid: 'Office', signal: -62, secured: true, enterprise: true },
	{ ssid: 'Cafe Guest', signal: -75, secured: false, enterprise: false }
];

/**
 * A scripted radio for building and testing screens without hardware. Same
 * states, same errors, same timing shape as a real server.
 */
export function fakeBoxRadio(opts: FakeRadioOptions = {}): BoxRadio {
	const speed = opts.speed ?? 1;
	const wait = (ms: number) => new Promise((r) => setTimeout(r, ms * speed));
	const boxes = opts.boxes ?? [
		{ id: 'fake-1', name: 'Virtues-4812', label: 'Virtues 4812', rssi: -52, state: 'needs-wifi' as const }
	];
	const phrase = opts.phrase ?? 'mango-burly-skull-dough';
	const bad = opts.badPassword ?? 'wrongpassword';

	const link = (box: NearbyBox, purpose: 'setup' | 'owner'): BoxWifiLink => ({
		box,
		purpose,
		async scan() {
			await wait(1400);
			return FAKE_NETWORKS;
		},
		async join(creds, onStage) {
			onStage?.('sent');
			await wait(600);
			onStage?.('joining');
			await wait(2400);
			if (creds.password === bad) {
				throw new BoxRadioError('join-failed', "Your server couldn't join that network. Check the password.");
			}
			onStage?.('joined');
			await wait(400);
			return { url: 'http://192.168.1.50:8000' };
		},
		async close() {}
	});

	return {
		available: true,
		async discover() {
			await wait(1800);
			return boxes;
		},
		async tryOpenWithoutWords() {
			await wait(500);
			return null; // every fake server has the four-word gate
		},
		async openSetup(box, words) {
			await wait(700);
			if (words.trim().toLowerCase().replace(/\s+/g, '-') !== phrase) {
				throw new BoxRadioError('refused', "Those words don't match the ones on your server.");
			}
			return {
				...link(box, 'setup'),
				purpose: 'setup',
				gated: true,
				async grant() {
					await wait(500);
				},
				async pair() {
					await wait(1500);
					return { paired: true, reachable: true };
				}
			};
		},
		async openOwner(box) {
			const target = box ?? boxes.find((b) => b.state === 'needs-owner');
			if (!target) throw new BoxRadioError('not-found', 'No server nearby is asking for its owner.');
			await wait(900);
			return link(target, 'owner');
		}
	};
}

// ─── which one this page gets ───────────────────────────────────────────────

const unavailable: BoxRadio = {
	available: false,
	async discover() {
		throw new BoxRadioError('unavailable', 'Bluetooth needs the Virtues app.');
	},
	async tryOpenWithoutWords() {
		throw new BoxRadioError('unavailable', 'Bluetooth needs the Virtues app.');
	},
	async openSetup() {
		throw new BoxRadioError('unavailable', 'Bluetooth needs the Virtues app.');
	},
	async openOwner() {
		throw new BoxRadioError('unavailable', 'Bluetooth needs the Virtues app.');
	}
};

/** `?radio=fake` switches the fake on for this tab; `?radio=off` switches it off. */
function fakeRequested(): boolean {
	if (typeof window === 'undefined') return false;
	try {
		const q = new URLSearchParams(window.location.search).get('radio');
		if (q === 'fake') sessionStorage.setItem('virtues-radio', 'fake');
		if (q === 'off') sessionStorage.removeItem('virtues-radio');
		return sessionStorage.getItem('virtues-radio') === 'fake';
	} catch {
		return false;
	}
}

function pick(): BoxRadio {
	if (tauriRadio.available) return tauriRadio;
	if (import.meta.env.DEV && fakeRequested()) {
		// A server that moved, too, so the owner path can be walked in a browser.
		return fakeBoxRadio({
			boxes: [
				{ id: 'fake-1', name: 'Virtues-4812', label: 'Virtues 4812', rssi: -52, state: 'needs-wifi' },
				{ id: 'fake-2', name: 'Virtues-0371', label: 'Virtues 0371', rssi: -60, state: 'needs-owner' }
			]
		});
	}
	return unavailable;
}

/** The radio for this page. Decided once; the platform cannot change under it. */
export const boxRadio: BoxRadio = pick();
