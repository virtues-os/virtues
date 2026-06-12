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

import { getSetupState, type SetupStep } from '$lib/api/client';

const POLL_INTERVAL = 60_000; // 60 seconds

class SetupStateStore {
	onboarding = $state<SetupStep[]>([]);
	/** null = not loaded yet (distinct from a real false from the server). */
	setupComplete = $state<boolean | null>(null);
	loaded = $state(false);
	/** Set when remote_access flips false → true mid-session; consumer resets it. */
	remoteAccessFlipped = $state(false);

	/**
	 * Last observed remote_access.done. Starts null so the very first
	 * successful fetch can never register a false→true transition — the
	 * first-load toast is suppressed by construction, not by a special case.
	 */
	private prevRemoteDone: boolean | null = null;

	/** Once everything is done there is nothing left to flip — stay stopped. */
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

	/** Start polling /api/setup/state */
	start() {
		if (this.intervalId || this.finished) return;

		this.check();
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
			this.setupComplete = data.setup_complete ?? null;
			this.loaded = true;

			// Flip detection: only a mid-session false → true transition counts.
			const nowDone = this.remoteAccess?.done ?? null;
			if (this.prevRemoteDone === false && nowDone === true) {
				this.remoteAccessFlipped = true;
			}
			if (nowDone !== null) {
				this.prevRemoteDone = nowDone;
			}

			// Setup complete and every win collected — stop polling permanently.
			if (this.setupComplete === true && this.allDone) {
				this.finished = true;
				this.stop();
			}
		} catch {
			// Network error — keep last state, retry next interval.
		}
	}
}

export const setupStateStore = new SetupStateStore();
