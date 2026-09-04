/**
 * Subscription Store
 *
 * Polls /api/subscription for the box's billing standing: linked (has an
 * account key) and subscribed (a subscription stands behind it) — two facts
 * since 0017, not one. There is no trial and never was; the countdown fields
 * this store once carried were vestigial from a plan that did not ship.
 *
 * Pauses polling when the browser tab is hidden.
 */

import { getSubscription } from '$lib/api/client';

const POLL_INTERVAL = 60_000; // 60 seconds

class SubscriptionStore {
	/** `none` | `linked` | `active` | `unknown`. */
	status = $state<string>('unknown');
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

	/**
	 * Fetch subscription status (also callable externally to force refresh).
	 * `fresh` makes the box re-ask atlas instead of serving its cache — for
	 * the minutes after a checkout is opened, when the answer is about to
	 * change.
	 */
	async check(fresh = false) {
		try {
			const data = await getSubscription<{
				status?: string;
				is_active?: boolean;
				linked?: boolean;
				subscribed?: boolean;
				entitlement_known?: boolean;
			}>(fresh);

			this.status = data.status ?? 'unknown';
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
