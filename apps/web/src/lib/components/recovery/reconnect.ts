/**
 * The recovery screen's plain parts: the verdict rules and the two ways of
 * reaching the native side (the real plugin, and a fake for `?radio=fake`
 * and tests). The screen's state lives in `reconnect.svelte.ts`; see its
 * header for what each verdict means.
 */

/** Between checks while nothing is wrong enough to stop checking. */
export const LISTEN_EVERY_MS = 15_000;

export type Verdict = 'checking' | 'back' | 'rejected' | 'offline' | 'silent';

/** The plugin's `ReachStatus`, as much of it as this screen reads. */
export interface ReachStatus {
	paired: boolean;
	session: 'authed' | 'rejected' | 'unknown' | 'unpaired';
	reachable?: boolean;
	path?: string;
	/** Why this device cannot use the pairing it has, in words for the
	 *  owner. Absent from an app older than surface 15. */
	problem?: string | null;
	/** The device key is gone, so pairing again is the only fix. */
	keyMissing?: boolean;
}

/** A pairing this device has but cannot use. */
export interface PairingProblem {
	/** The plugin's sentence, shown as it is. */
	message: string;
	/** Pairing again fixes it. Only when the device key is gone. After any
	 *  other problem the key may still be on this device, and quitting and
	 *  reopening Virtues can bring it back. Pairing again would add a second
	 *  device to the server's list while the first key still works. */
	pairAgain: boolean;
}

/** What is wrong with this device's own pairing, if anything. */
export function pairingProblem(status: ReachStatus | null): PairingProblem | null {
	if (!status?.paired || !status.problem) return null;
	return { message: status.problem, pairAgain: status.keyMissing === true };
}

/** What the screen needs from the native side; a fake stands in for tests and `?radio=fake`. */
export interface ReachPort {
	status(): Promise<ReachStatus>;
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
	async forget() {
		const { invoke } = await import('@tauri-apps/api/core');
		await invoke('plugin:reach|forget');
	},
	online: () => (typeof navigator === 'undefined' ? true : navigator.onLine)
};

/**
 * `?radio=fake` in dev: a paired server that doesn't answer for the first
 * twenty seconds, then comes back.
 */
export function fakeReach(speed = 1): ReachPort {
	const since = Date.now();
	const wait = (ms: number) => new Promise((r) => setTimeout(r, ms * speed));
	return {
		async status() {
			await wait(900);
			const back = Date.now() - since > 20_000 * speed;
			return { paired: true, session: back ? 'authed' : 'unknown', reachable: back };
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

/** The pure part: what a status adds up to. */
export function judge(status: ReachStatus | null, online: boolean): Exclude<Verdict, 'checking'> {
	if (status?.session === 'authed') return 'back';
	if (status?.session === 'rejected') return 'rejected';
	if (!online) return 'offline';
	return 'silent';
}

