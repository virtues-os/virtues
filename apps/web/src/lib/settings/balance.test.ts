import { describe, expect, it } from 'vitest';
import { balanceRows, type WalletUsage } from './balance';

const SINCE = 'November 14';
const usage = (over: Partial<WalletUsage>): WalletUsage => ({
	balance_micros: 18_000_000,
	month_to_date_micros: 5_000_000,
	topup_micros: 0,
	period_start: '2026-11-14T15:00:00Z',
	period_used_micros: 2_000_000,
	...over,
});
const total = (u: WalletUsage) => balanceRows(u, SINCE).reduce((sum, r) => sum + r.micros, 0);

describe('balanceRows', () => {
	it('shows the plan credit and the usage since the renewal, not since the 1st', () => {
		expect(balanceRows(usage({}), SINCE)).toEqual([
			{ label: 'Included with your plan', micros: 20_000_000 },
			{ label: 'Used on AI and web search since November 14', micros: -2_000_000 },
		]);
	});

	it('shows top-up credit on its own row', () => {
		const u = usage({ balance_micros: 33_000_000, topup_micros: 15_000_000 });
		expect(balanceRows(u, SINCE).map((r) => [r.label, r.micros])).toEqual([
			['Included with your plan', 20_000_000],
			['Top-up credit', 15_000_000],
			['Used on AI and web search since November 14', -2_000_000],
		]);
	});

	it('joins the two credits once the plan part is used up', () => {
		const u = usage({ balance_micros: 4_000_000, topup_micros: 4_000_000, period_used_micros: 26_000_000 });
		expect(balanceRows(u, SINCE)[0]).toEqual({ label: 'Plan and top-up credit', micros: 30_000_000 });
	});

	it('adds up to the balance in every state', () => {
		for (const u of [
			usage({}),
			usage({ balance_micros: 33_000_000, topup_micros: 15_000_000 }),
			usage({ balance_micros: 4_000_000, topup_micros: 4_000_000, period_used_micros: 26_000_000 }),
			usage({ balance_micros: 0, period_used_micros: 20_000_000 }),
			usage({ balance_micros: -40_000, period_used_micros: 20_040_000 }),
		]) {
			expect(total(u)).toBe(u.balance_micros);
		}
	});

	it('adds up in cents when the amounts fall between them', () => {
		const u = usage({ balance_micros: 18_605_000, period_used_micros: 1_395_000 });
		const [plan, used] = balanceRows(u, SINCE);
		expect([plan.micros, used.micros, plan.micros + used.micros]).toEqual([20_000_000, -1_390_000, 18_610_000]);
	});

	it('shows nothing when the service sends no renewal', () => {
		expect(balanceRows(usage({ period_start: undefined, period_used_micros: undefined }), SINCE)).toEqual([]);
		expect(balanceRows(usage({ period_start: null, period_used_micros: null }), SINCE)).toEqual([]);
	});
});
