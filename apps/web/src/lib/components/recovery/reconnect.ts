/**
 * The recovery screen's plain parts: the verdict rules and the two ways of
 * reaching the native side (the real plugin, and a fake for `?radio=fake`
 * and tests). The screen's state lives in `reconnect.svelte.ts`; see its
 * header for what each verdict means.
 */

import { shellSupports } from '$lib/tauri/bridge';
import type { BoxWifiLink } from '$lib/tauri/boxRadio';

/** `COMMAND_SURFACE_VERSION` that added `reach_rehome` (src-tauri/src/lib.rs). */
export const REHOME_SURFACE = 6;

/** How long a server that just joined a network gets to answer. A relay
 *  reconnect, a DHCP lease and a first iroh dial fit well inside this. */
export const COMEBACK_MS = 120_000;
/** Between checks while nothing is wrong enough to stop checking. */
export const LISTEN_EVERY_MS = 15_000;

export type Verdict = 'checking' | 'back' | 'rejected' | 'asking' | 'offline' | 'silent';
export type Phase =
	| { kind: 'diagnosis' }
	| { kind: 'opening' }
	| { kind: 'wifi'; link: BoxWifiLink }
	| { kind: 'returning'; since: number }
	| { kind: 'stuck' };

/** The plugin's `ReachStatus`, as much of it as this screen reads. */
export interface ReachStatus {
	paired: boolean;
	session: 'authed' | 'rejected' | 'unknown' | 'unpaired';
	reachable?: boolean;
	path?: string;
}

/** What the screen needs from the native side; a fake stands in for tests and `?radio=fake`. */
export interface ReachPort {
	status(): Promise<ReachStatus>;
	/** Returns false when this app is too old to have the command. */
	rehome(url: string): Promise<boolean>;
	forget(): Promise<void>;
	online(): boolean;
}

type Shell = {
	__VIRTUES_MOBILE__?: boolean;
	__VIRTUES_BOX_URL__?: string;
	__VIRTUES_BACKEND_ORIGIN__?: string;
	__TAURI_INTERNALS__?: unknown;
};

export function shell(): Shell {
	return (typeof window === 'undefined' ? {} : window) as Shell;
}

/** Phone or computer, for the copy. */
export function thisDevice(): 'this phone' | 'this computer' {
	return shell().__VIRTUES_MOBILE__ ? 'this phone' : 'this computer';
}

const STATUS_DEADLINE_MS = 12_000;

function deadline<T>(p: Promise<T>, ms: number, fallback: T): Promise<T> {
	return Promise.race([p, new Promise<T>((r) => setTimeout(() => r(fallback), ms))]);
}

export const tauriReach: ReachPort = {
	async status() {
		const { invoke } = await import('@tauri-apps/api/core');
		// The native probe is bounded at 6s; this only guards a shell that
		// never answers at all.
		return deadline(
			invoke<ReachStatus>('plugin:reach|reach_status'),
			STATUS_DEADLINE_MS,
			{ paired: true, session: 'unknown' } as ReachStatus
		);
	},
	async rehome(url) {
		if (!(await shellSupports(REHOME_SURFACE))) return false;
		const { invoke } = await import('@tauri-apps/api/core');
		await invoke('plugin:reach|reach_rehome', { url });
		return true;
	},
	async forget() {
		const { invoke } = await import('@tauri-apps/api/core');
		await invoke('plugin:reach|forget');
	},
	online: () => (typeof navigator === 'undefined' ? true : navigator.onLine)
};

/**
 * `?radio=fake` in dev: a paired server that moved. Unreachable until its
 * Wi-Fi is set over the fake radio, then back a few seconds later.
 */
export function fakeReach(speed = 1): ReachPort {
	let rejoinedAt = 0;
	const wait = (ms: number) => new Promise((r) => setTimeout(r, ms * speed));
	return {
		async status() {
			await wait(900);
			const back = rejoinedAt > 0 && Date.now() - rejoinedAt > 4000 * speed;
			return { paired: true, session: back ? 'authed' : 'unknown', reachable: back };
		},
		async rehome() {
			rejoinedAt = Date.now();
			return true;
		},
		async forget() {},
		online: () => true
	};
}

function fakeRequested(): boolean {
	try {
		return import.meta.env.DEV && sessionStorage.getItem('virtues-radio') === 'fake';
	} catch {
		return false;
	}
}

export function pickReach(): ReachPort {
	return fakeRequested() ? fakeReach() : tauriReach;
}

/** The pure part: what a status and a Bluetooth scan add up to. */
export function judge(
	status: ReachStatus | null,
	asking: boolean,
	online: boolean
): Exclude<Verdict, 'checking'> {
	if (status?.session === 'authed') return 'back';
	if (status?.session === 'rejected') return 'rejected';
	// A server asking for its owner is worth offering even when this device
	// is offline: Bluetooth needs no network, and the fix happens over it.
	if (asking) return 'asking';
	if (!online) return 'offline';
	return 'silent';
}

