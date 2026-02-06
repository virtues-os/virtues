/**
 * Subscription Store
 *
 * Polls /api/subscription to track subscription status and trial countdown.
 * Used by the app layout to show trial toasts and handle expired subscriptions.
 *
 * Pauses polling when the browser tab is hidden.
 */

const POLL_INTERVAL = 60_000; // 60 seconds

class SubscriptionStore {
	status = $state<string>('active');
	trialExpiresAt = $state<string | null>(null);
	daysRemaining = $state<number | null>(null);
	isActive = $state(true);

	private intervalId: ReturnType<typeof setInterval> | null = null;
	private visibilityHandler: (() => void) | null = null;

	/** Start polling /api/subscription */
	start() {
		if (this.intervalId) return;

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

	/** Fetch subscription status (also callable externally to force refresh) */
	async check() {
		try {
			const res = await fetch('/api/subscription');
			if (!res.ok) return;
			const data = await res.json();

			this.status = data.status ?? 'active';
			this.trialExpiresAt = data.trial_expires_at ?? null;
			this.daysRemaining = data.days_remaining ?? null;
			this.isActive = data.is_active ?? true;
		} catch {
			// Network error - ignore, will retry next interval
		}
	}
}

export const subscriptionStore = new SubscriptionStore();
