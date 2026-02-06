/**
 * Version Store
 *
 * Two-level version checking:
 *
 * 1. Frontend drift: Polls /health to detect when the backend has been updated
 *    to a newer version than what was baked into this frontend at build time.
 *    When detected, shows a persistent banner prompting the user to refresh.
 *
 * 2. System update: Polls /api/system/update-available to detect when Atlas
 *    has a newer image available than what's currently running. When detected,
 *    shows a persistent banner with "Update" action that triggers a rolling deploy.
 *
 * Skips polling in dev mode (GIT_COMMIT not injected).
 * Pauses polling when the browser tab is hidden.
 */

import { checkSystemUpdate, triggerSystemUpdate } from '$lib/api/client';

const BUILD_COMMIT: string = __BUILD_COMMIT__;
const POLL_INTERVAL = 60_000; // 60 seconds (frontend drift)
const SYSTEM_UPDATE_POLL_INTERVAL = 300_000; // 5 minutes (system updates)

class VersionStore {
	/** Frontend code is out of sync with backend (needs page refresh) */
	updateAvailable = $state(false);
	serverCommit = $state<string | null>(null);

	/** A newer system image is available from Atlas (needs container update) */
	systemUpdateAvailable = $state(false);
	latestVersion = $state<string | null>(null);

	/** Update is currently in progress */
	updating = $state(false);

	private intervalId: ReturnType<typeof setInterval> | null = null;
	private systemIntervalId: ReturnType<typeof setInterval> | null = null;
	private visibilityHandler: (() => void) | null = null;

	/** Start polling for version changes */
	start() {
		if (this.intervalId) return;
		if (BUILD_COMMIT === 'dev') return; // No version tracking in dev

		this.resume();

		this.visibilityHandler = () => {
			if (document.hidden) {
				this.pause();
			} else {
				this.resume();
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

	/** Trigger a system update (container restart via Atlas) */
	async triggerUpdate() {
		if (this.updating) return;
		this.updating = true;

		try {
			await triggerSystemUpdate();
			// Update triggered — the container will restart.
			// The UpdateOverlay component handles waiting for /health to respond.
		} catch (e) {
			console.error('Failed to trigger update:', e);
			this.updating = false;
			throw e;
		}
	}

	/** Resume polling, respecting current state (only restarts what's still needed) */
	private resume() {
		this.pause(); // Clear any stale intervals first

		// Only restart drift polling if we haven't detected an update yet
		if (!this.updateAvailable) {
			this.checkFrontendDrift();
			this.intervalId = setInterval(() => this.checkFrontendDrift(), POLL_INTERVAL);
		}

		// Only restart system polling if no update detected and not mid-update
		if (!this.systemUpdateAvailable && !this.updating) {
			this.checkSystemUpdate();
			this.systemIntervalId = setInterval(() => this.checkSystemUpdate(), SYSTEM_UPDATE_POLL_INTERVAL);
		}
	}

	/** Pause all polling (clears intervals, does not affect state) */
	private pause() {
		if (this.intervalId) {
			clearInterval(this.intervalId);
			this.intervalId = null;
		}
		if (this.systemIntervalId) {
			clearInterval(this.systemIntervalId);
			this.systemIntervalId = null;
		}
	}

	private async checkFrontendDrift() {
		try {
			const res = await fetch('/health');
			if (!res.ok) return;
			const data = await res.json();

			if (data.commit) {
				this.serverCommit = data.commit;
				if (data.commit !== BUILD_COMMIT) {
					this.updateAvailable = true;
					// Stop frontend drift polling once detected
					if (this.intervalId) {
						clearInterval(this.intervalId);
						this.intervalId = null;
					}
				}
			}
		} catch {
			// Network error - ignore, will retry next interval
		}
	}

	private async checkSystemUpdate() {
		try {
			const status = await checkSystemUpdate();
			this.systemUpdateAvailable = status.available;
			this.latestVersion = status.latest;

			if (status.available) {
				// Stop system update polling once detected
				if (this.systemIntervalId) {
					clearInterval(this.systemIntervalId);
					this.systemIntervalId = null;
				}
			}
		} catch {
			// Network error or Tollbooth not available — ignore, will retry
		}
	}
}

export const versionStore = new VersionStore();
