/**
 * Subscription Store
 *
 * Polls /api/subscription to track subscription status and trial countdown.
 * Used by the app layout to show trial toasts and handle expired subscriptions.
 *
 * Pauses polling when the browser tab is hidden.
 */

import { getSubscription } from '$lib/api/client';

const POLL_INTERVAL = 60_000; // 60 seconds

class SubscriptionStore {
	/** `none` | `linked` | `active` | `unknown`. */
	status = $state<string>('unknown');
	trialExpiresAt = $state<string | null>(null);
	daysRemaining = $state<number | null>(null);
	isActive = $state(false);
	/** Does the box hold an api_key. Says nothing about payment since 0017. */
	linked = $state(false);
	/** Is there an active subscription behind that key. The one that matters. */
	subscribed = $state(false);
	/**
	 * False when atlas could not be reached and the box has no cached answer.
	 * Callers must render a third state, not fall back to "unsubscribed" — an
	 * outage telling a paying owner their subscription is gone is the failure
	 * this flag exists to prevent.
	 */
	entitlementKnown = $state(false);

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
			const data = await getSubscription<{
				status?: string;
				trial_expires_at?: string | null;
				days_remaining?: number | null;
				is_active?: boolean;
				linked?: boolean;
				subscribed?: boolean;
				entitlement_known?: boolean;
			}>();

			this.status = data.status ?? 'unknown';
			this.trialExpiresAt = data.trial_expires_at ?? null;
			this.daysRemaining = data.days_remaining ?? null;
			this.isActive = data.is_active ?? false;
			this.linked = data.linked ?? false;
			this.subscribed = data.subscribed ?? data.is_active ?? false;
			this.entitlementKnown = data.entitlement_known ?? false;
		} catch {
			// Non-2xx (incl. 402/401) or network error - ignore, will retry next interval
		}
	}
}

export const subscriptionStore = new SubscriptionStore();
