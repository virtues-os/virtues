/**
 * "Can't reach your server", as a place you can do something about it.
 *
 * A paired device lands here when its server doesn't answer: at launch (the
 * Mac's shell and the phone's connect page send it), or from the app's own
 * "Can't reach your server" banner. It runs from the app's OWN copy (the
 * phone's baked bundle, the Mac's baked copy), because by definition the
 * server's copy is the thing that isn't answering.
 *
 * WHAT CAN BE WRONG, and what this can tell apart:
 *
 *   back       the server answers and knows this device: open the app
 *   rejected   it answers and refuses this device (reset, or removed from
 *              its list): pair again
 *   asking     a server nearby is advertising for its owner over Bluetooth
 *              (claimed, and offline for 90s; virtues-core `ble_provision`
 *              owner mode): the moved-server case, fixable from right here
 *   offline    THIS device has no network: nothing else can be judged
 *   silent     none of the above: off, unplugged, restarting, or somewhere
 *              this device can't reach. Say so, and keep listening.
 *
 * THE MOVED SERVER, end to end:
 *   1. `boxRadio.openOwner()` proves over Bluetooth that this device is one
 *      of the server's own (a signature with its pairing key), which opens a
 *      wifi-only session. A stranger's server refuses, and the next one is
 *      tried.
 *   2. The Wi-Fi step (Setup's own, taking the link) puts it on a network.
 *   3. `reach_rehome(url)` points this pairing at the address the server
 *      reported. A server with a relay needs none of it (iroh finds it by
 *      its id); one set up without an account only has LAN addresses from
 *      where it lived, so without this it could never be found again.
 *   4. Wait for the server to answer, then open the app.
 *
 * Bluetooth is never touched while a link is open, and opening one waits for
 * any scan in flight: `discover` resets the native side's list of what it has
 * heard, which is what a connect looks the server up in. On the iPhone a scan
 * that overlapped a connect made it fail as "out of range".
 */

import { boxRadio, BoxRadioError, type NearbyBox } from '$lib/tauri/boxRadio';
import { prePair } from '$lib/components/setup/prepair.svelte';
import { COMEBACK_MS, judge, pickReach, shell, type Phase, type ReachPort, type Verdict } from './reconnect';

export { LISTEN_EVERY_MS, thisDevice } from './reconnect';

export class Reconnect {
	verdict = $state<Verdict>('checking');
	phase = $state<Phase>({ kind: 'diagnosis' });
	/** Why Bluetooth can't look, when it can't (off, not allowed). */
	radioNote = $state<string | null>(null);
	/** The last thing that went wrong, in words, beside what failed. */
	error = $state<string | null>(null);
	/** A check is running. */
	busy = $state(false);

	/** Servers nearby asking for their owner, closest first. */
	private asking: NearbyBox[] = [];
	/** Servers that refused this device: someone else's, never offered again
	 *  on this screen. Without this, a stranger's server in a shared office
	 *  kept the offer up, and every tap ended the same way. */
	private notMine = new Set<string>();
	/** The check in flight, so opening a server can wait for its scan. */
	private looking: Promise<void> | null = null;
	/** Bumped to abandon an in-flight check or wait. */
	private gen = 0;

	constructor(private reach: ReachPort = pickReach()) {}

	get canUseRadio(): boolean {
		return boxRadio.available;
	}

	/**
	 * One look: the server over the network, and anything nearby over
	 * Bluetooth, side by side. Safe to call again at any time; an older look
	 * that finishes late is ignored.
	 */
	check(): Promise<void> {
		if (this.phase.kind !== 'diagnosis') return Promise.resolve();
		if (this.looking) return this.looking;
		this.looking = this.look().finally(() => (this.looking = null));
		return this.looking;
	}

	private async look(): Promise<void> {
		const my = ++this.gen;
		this.busy = true;
		const [status, asking] = await Promise.all([
			this.reach.status().catch(() => null),
			this.listen()
		]);
		this.busy = false;
		if (my !== this.gen || this.phase.kind !== 'diagnosis') return;
		if (status && !status.paired) {
			// Nothing to reach: this device was unpaired somewhere else.
			this.pairAgain(false);
			return;
		}
		this.verdict = judge(status, asking, this.reach.online());
		if (this.verdict !== 'asking' && this.verdict !== 'silent') this.error = null;
		if (this.verdict === 'back') this.openApp();
	}

	/** Is a server nearby asking for its owner, and not one that already refused us? */
	private async listen(): Promise<boolean> {
		if (!boxRadio.available) return false;
		try {
			const found = await boxRadio.discover({ seconds: 4 });
			this.radioNote = null;
			this.asking = found.filter((b) => b.state === 'needs-owner' && !this.notMine.has(b.id));
		} catch (e) {
			this.radioNote = e instanceof BoxRadioError ? e.message : null;
			this.asking = [];
		}
		return this.asking.length > 0;
	}

	/**
	 * "Choose its Wi-Fi": prove ownership over Bluetooth and open the Wi-Fi
	 * step. Tries each server asking, closest first; one that refuses is
	 * someone else's.
	 */
	async openOwner(): Promise<void> {
		this.error = null;
		this.phase = { kind: 'opening' };
		// A scan still running would pull the server out from under the connect.
		await this.looking?.catch(() => {});
		const my = ++this.gen;
		let last: BoxRadioError | null = null;
		outer: for (const box of this.asking) {
			// Two tries per server. A refusal is usually "not yours", but the
			// server answers a stale or evicted challenge the same way, so one
			// fresh challenge comes before calling a server someone else's.
			for (let attempt = 0; attempt < 2; attempt++) {
				try {
					const link = await boxRadio.openOwner(box);
					if (my !== this.gen) {
						// The screen moved on (left, or a new attempt) while
						// this one was connecting: close what it opened.
						await link.close().catch(() => {});
						return;
					}
					this.phase = { kind: 'wifi', link };
					return;
				} catch (e) {
					last = e instanceof BoxRadioError ? e : new BoxRadioError('failed', "Couldn't open your server over Bluetooth. Try again.");
					if (my !== this.gen) return;
					// Make sure the connection is gone before the next connect.
					await boxRadio.disconnect();
					if (last.code !== 'refused') break outer; // out of range, silent, an old app: another try won't help
				}
			}
			this.notMine.add(box.id);
		}
		this.phase = { kind: 'diagnosis' };
		this.asking = this.asking.filter((b) => !this.notMine.has(b.id));
		this.verdict = this.asking.length ? 'asking' : 'silent';
		this.error = !last
			? // The list emptied between the last scan and the tap.
				'Your server stopped showing up nearby. Move closer to it and look again.'
			: last.code === 'refused'
				? "The server nearby isn't paired with this device."
				: last.message;
	}

	/** The Wi-Fi step lost its link (out of range, or the server went quiet). */
	async lost(): Promise<void> {
		// Closed before the next scan starts: Bluetooth is never scanned while
		// a link is open or closing.
		await this.closeLink();
		this.phase = { kind: 'diagnosis' };
		this.error = 'Lost the Bluetooth connection to your server. Move closer to it and try again.';
		void this.check();
	}

	/** The server joined a network and reported the address it now answers on. */
	async joined(url: string): Promise<void> {
		await this.closeLink();
		const my = ++this.gen;
		const since = Date.now();
		this.phase = { kind: 'returning', since };
		try {
			await this.reach.rehome(url);
		} catch {
			// A failed re-home still leaves the relay path; keep waiting.
		}
		while (my === this.gen && Date.now() - since < COMEBACK_MS) {
			const s = await this.reach.status().catch(() => null);
			if (my !== this.gen) return;
			if (s?.session === 'authed') {
				this.openApp();
				return;
			}
			if (s?.session === 'rejected') {
				this.phase = { kind: 'diagnosis' };
				this.verdict = 'rejected';
				return;
			}
			await new Promise((r) => setTimeout(r, 3000));
		}
		if (my === this.gen) this.phase = { kind: 'stuck' };
	}

	/** From `stuck`: look again from the top. */
	retry(): void {
		this.error = null;
		this.phase = { kind: 'diagnosis' };
		this.verdict = 'checking';
		void this.check();
	}

	/** Stop anything in flight (the page is going away). */
	dispose(): void {
		this.gen++;
		void this.closeLink();
	}

	private async closeLink(): Promise<void> {
		const p = this.phase;
		if (p.kind === 'wifi') await p.link.close().catch(() => {});
	}

	/** Into the app, which is this same copy at `/` on the phone and the Mac. */
	openApp(): void {
		this.gen++;
		window.location.replace('/');
	}

	/**
	 * Can the app open without its server? Its own copy can (the phone, and the
	 * Mac since 2026-09-29): it shows the "Can't reach your server" banner.
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
		if (!boxRadio.available) {
			// No radio in this copy (Android): its connect page still pairs.
			window.location.replace('/connect.html');
			return;
		}
		prePair.startOver();
		window.location.replace('/setup');
	}
}
