/**
 * Setup State Store
 *
 * Polls GET /api/setup/state to track the post-setup "next wins" onboarding
 * checklist (first_source, first_device, remote_access, first_sync) and to
 * detect the remote-access verdict flipping to reachable (the flip toast).
 *
 * Pauses polling when the browser tab is hidden, and stops permanently once
 * setup is complete and every onboarding step is done — nothing left to flip.
 */

import { getSetupState, type DegradedCollector, type SetupStep } from '$lib/api/client';

const POLL_INTERVAL = 60_000; // 60 seconds

class SetupStateStore {
	onboarding = $state<SetupStep[]>([]);
	/** The setup half too — Home's getting-started needs the account step. */
	setup = $state<SetupStep[]>([]);
	/** null = not loaded yet (distinct from a real false from the server). */
	setupComplete = $state<boolean | null>(null);
	loaded = $state(false);
	/** Set when remote_access flips false → true mid-session; consumer resets it. */
	remoteAccessFlipped = $state(false);
	/** Collectors running with a denied permission — surfaced, never swallowed. */
	degraded = $state<DegradedCollector[]>([]);

	/**
	 * Last observed remote_access.done. Starts null so the very first
	 * successful fetch can never register a false→true transition — the
	 * first-load toast is suppressed by construction, not by a special case.
	 */
	private prevRemoteDone: boolean | null = null;

	/**
	 * Snapshot of "everything done" from the last check — when true we stop the
	 * polling interval (nothing left to flip). NOT a permanent latch: `check()`
	 * recomputes it every fetch, and `start()` always does one fresh check, so
	 * if state later regresses (a new device pairs, a step un-completes) polling
	 * revives on the next start()/visibility change instead of being frozen for
	 * the rest of the session.
	 */
	private finished = false;

	private intervalId: ReturnType<typeof setInterval> | null = null;
	private visibilityHandler: (() => void) | null = null;

	/** Every onboarding win collected (false until the list has loaded). */
	get allDone(): boolean {
		return this.onboarding.length > 0 && this.onboarding.every((s) => s.done);
	}

	/** The remote_access step, if the server reports one. */
	get remoteAccess(): SetupStep | undefined {
		return this.onboarding.find((s) => s.id === 'remote_access');
	}

	get doneCount(): number {
		return this.onboarding.filter((s) => s.done).length;
	}

	/** One onboarding win, by id. False while nothing has loaded. */
	done(id: string): boolean {
		return this.onboarding.find((s) => s.id === id)?.done ?? false;
	}

	/**
	 * May this box call the models? Account linked, or setup complete without
	 * one — the second arm is the DIY exemption (`compute_setup_state` requires
	 * an account only on appliances), mirrored from the old onboarding route.
	 */
	get accountSatisfied(): boolean {
		const linked = this.setup.find((s) => s.id === 'account')?.done ?? false;
		return linked || this.setupComplete === true;
	}

	/** Anything actually flowing — the signal that retires "connect your world".
	 *  `device_collecting` (data has landed), never `first_device` (merely
	 *  paired): the device someone paired to ENTER the app is an app_device
	 *  row, so counting pairings made this step born-done on every box and
	 *  fired the "tomorrow morning" promise over an empty record. */
	get worldEnough(): boolean {
		return (
			this.done('first_source') ||
			this.done('living_source') ||
			this.done('first_phone') ||
			this.done('chat_imported') ||
			this.done('device_collecting')
		);
	}

	/** Start polling /api/setup/state */
	start() {
		if (this.intervalId) return; // already running

		// Always do one fresh check on (re)start — this re-evaluates `finished`,
		// so a remount after state regressed revives polling rather than being
		// blocked by a stale latch.
		this.check();
		if (this.finished) return; // done — one check, no interval/handler

		this.intervalId = setInterval(() => this.check(), POLL_INTERVAL);

		this.visibilityHandler = () => {
			if (document.hidden) {
				this.pause();
			} else {
				this.pause();
				this.check();
				this.intervalId = setInterval(() => this.check(), POLL_INTERVAL);
			}
		};
		document.addEventListener('visibilitychange', this.visibilityHandler);
	}

	/** Stop polling entirely */
	stop() {
		this.pause();
		if (this.visibilityHandler) {
			document.removeEventListener('visibilitychange', this.visibilityHandler);
			this.visibilityHandler = null;
		}
	}

	private pause() {
		if (this.intervalId) {
			clearInterval(this.intervalId);
			this.intervalId = null;
		}
	}

	/** Fetch setup state (also callable externally to force refresh) */
	async check() {
		try {
			const data = await getSetupState();
			this.onboarding = data.onboarding ?? [];
			this.setup = data.setup ?? [];
			this.setupComplete = data.setup_complete ?? null;
			this.degraded = data.degraded ?? [];
			this.loaded = true;

			// Flip detection: only a mid-session false → true transition counts.
			const nowDone = this.remoteAccess?.done ?? null;
			if (this.prevRemoteDone === false && nowDone === true) {
				this.remoteAccessFlipped = true;
			}
			if (nowDone !== null) {
				this.prevRemoteDone = nowDone;
			}

			// Recompute "done" every fetch (revivable, not a permanent latch).
			// When done, stop the interval; if state later regresses this flips
			// back to false and the next start()/visibility change resumes.
			this.finished = this.setupComplete === true && this.allDone;
			if (this.finished) {
				this.stop();
			}
		} catch {
			// Network error — keep last state, retry next interval.
		}
	}
}

export const setupStateStore = new SetupStateStore();
