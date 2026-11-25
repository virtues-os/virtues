/**
 * How Billing's balance is reached: what the plan and top-ups put in, less
 * what AI and web search used since the plan last renewed. The rows are
 * whole cents and always add up to the balance in cents.
 *
 * The plan's part is spent first and resets at each renewal; top-up credit
 * is spent after it and stays (services/virtues-api/src/entitlement.rs).
 */

/** What `/api/billing/usage` sends about the wallet. */
export type WalletUsage = {
	balance_micros: number;
	month_to_date_micros: number;
	/** The part of the balance that is top-up credit. Absent from older servers. */
	topup_micros?: number;
	/** When the plan's credit was last set. Null or absent without a renewal,
	 *  or from a Virtues service that predates it. */
	period_start?: string | null;
	/** What has come off the balance since `period_start`. */
	period_used_micros?: number | null;
};

export type BalanceRow = { label: string; micros: number };

const toCents = (micros: number) => Math.round(micros / 10_000) * 10_000;

/**
 * The rows above the total, or none when the service can't say what was used
 * since the renewal. `since` is the renewal date as the page writes it.
 */
export function balanceRows(usage: WalletUsage, since: string): BalanceRow[] {
	const used = usage.period_used_micros;
	if (!usage.period_start || used === null || used === undefined) return [];
	const topup = usage.topup_micros ?? 0;
	const planLeft = usage.balance_micros - topup;
	// Top-up credit is untouched while plan credit is left, so each part is known.
	const credit: BalanceRow[] = [];
	if (planLeft > 0) {
		credit.push({ label: 'Included with your plan', micros: toCents(planLeft + used) });
		if (topup > 0) credit.push({ label: 'Top-up credit', micros: toCents(topup) });
	} else {
		// The plan's part is used up and top-up credit is being spent. How the
		// two shared the usage isn't recorded, so they are one row.
		credit.push({ label: 'Plan and top-up credit', micros: toCents(usage.balance_micros + used) });
	}
	// The last row takes the rounding, so the cents on screen add up.
	const put = credit.reduce((sum, r) => sum + r.micros, 0);
	return [...credit, { label: `Used on AI and web search since ${since}`, micros: toCents(usage.balance_micros) - put }];
}
