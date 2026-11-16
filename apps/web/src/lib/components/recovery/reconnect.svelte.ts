/**
 * "Can't reach your server", as a place you can do something about it.
 *
 * A paired device lands here when its server doesn't answer: at launch (the
 * Mac's shell and the phone's connect page send it), or from the app's own
 * "Can't reach your server" banner or sidebar chip. It runs from the app's
 * OWN copy (the phone's baked bundle, the Mac's baked copy), because by
 * definition the server's copy is the thing that isn't answering.
 *
 * WHAT CAN BE WRONG, and what this can tell apart:
 *
 *   back       the server answers and knows this device: open the app
 *   rejected   it answers and refuses this device (reset, or removed from
 *              its list): pair again
 *   offline    THIS device has no network: nothing else can be judged
 *   silent     none of the above: off, unplugged, restarting, or somewhere
 *              this device can't reach. Say so, and keep listening.
 *
 * Before any of those, this device's own pairing can be the trouble: it has
 * one but cannot use it (`problem`). On the Mac the Keychain refused to give
 * the device key back, or no longer has it; on any device the pairing file
 * can fail to read. The plugin's sentence is shown as it is, and no check of
 * the server can help. Only a missing key is fixed by pairing again
 * (`problem.pairAgain`). After any other problem the key may still be on
 * this device, and quitting and reopening Virtues can bring it back. Pairing
 * again would add a second device to the server's list while the first key
 * still works.
 */

import { prePair } from '$lib/components/setup/prepair.svelte';
import { isIOS, isLinux, isMacOS, isTauri, isWindows } from '$lib/utils/platform';
import {
	judge,
	pairingProblem,
	pickReach,
	shell,
	type PairingProblem,
	type ReachPort,
	type Verdict
} from './reconnect';

export { LISTEN_EVERY_MS, thisDevice } from './reconnect';

export class Reconnect {
	verdict = $state<Verdict>('checking');
	/** The last thing that went wrong, in words, beside what failed. */
	error = $state<string | null>(null);
	/** A check is running. */
	busy = $state(false);
	/** A pairing this device has but cannot use. While set, `error` holds
	 *  its sentence and `verdict` is `silent`. */
	problem = $state<PairingProblem | null>(null);

	/** The check in flight. */
	private looking: Promise<void> | null = null;
	/** Bumped to abandon an in-flight check. */
	private gen = 0;

	constructor(private reach: ReachPort = pickReach()) {}

	/** One look at the server. Safe to call again at any time; an older look
	 *  that finishes late is ignored. */
	check(): Promise<void> {
		if (this.looking) return this.looking;
		this.looking = this.look().finally(() => (this.looking = null));
		return this.looking;
	}

	private async look(): Promise<void> {
		const my = ++this.gen;
		this.busy = true;
		const status = await this.reach.status().catch(() => null);
		this.busy = false;
		if (my !== this.gen) return;
		if (status && !status.paired) {
			// Nothing to reach: this device was unpaired somewhere else.
			this.pairAgain(false);
			return;
		}
		const problem = pairingProblem(status);
		if (problem) {
			// This device's own record is the trouble.
			this.problem = problem;
			this.verdict = 'silent';
			this.error = problem.message;
			return;
		}
		if (this.problem) {
			this.problem = null;
			this.error = null;
		}
		this.verdict = judge(status, this.reach.online());
		if (this.verdict !== 'silent') this.error = null;
		if (this.verdict === 'back') this.openApp();
	}

	/** Stop anything in flight (the page is going away). */
	dispose(): void {
		this.gen++;
	}

	/** Into the app, which is this same copy at `/` on the phone and the Mac. */
	openApp(): void {
		this.gen++;
		window.location.replace('/');
	}

	/**
	 * Can the app open without its server? Its own copy can (the phone, and the
	 * Mac since 2026-09-29): it says "Can't reach your server" in the phone's
	 * banner and the desktop's sidebar footer.
	 */
	get canOpenAnyway(): boolean {
		const s = shell();
		return !!(s.__VIRTUES_BACKEND_ORIGIN__ || s.__VIRTUES_MOBILE__);
	}

	/**
	 * Forget this server on this device and set up again. Nothing on the
	 * server changes. `forget` false when there is nothing left to forget.
	 */
	async pairAgain(forget = true): Promise<void> {
		this.dispose();
		if (forget) await this.reach.forget().catch(() => {});
		if (!(isTauri && (isIOS || isMacOS || isWindows || isLinux))) {
			// Android pairs on its connect page.
			window.location.replace('/connect.html');
			return;
		}
		prePair.startOver();
		window.location.replace('/setup');
	}
}
