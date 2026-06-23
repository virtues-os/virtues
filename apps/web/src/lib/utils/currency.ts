/**
 * Money in Virtues is stored in **micros** (millionths of a dollar):
 * 1_000_000 micros = $1.00. These helpers render micros as USD for the UI.
 */

/** Format micros as a USD string, e.g. 12_340_000 → "$12.34". */
export function formatMicrosUSD(micros: number): string {
	return new Intl.NumberFormat('en-US', {
		style: 'currency',
		currency: 'USD',
		minimumFractionDigits: 2,
		maximumFractionDigits: 2,
	}).format((micros ?? 0) / 1_000_000);
}

/**
 * Format a per-call micros amount, which can be tiny (sub-cent). Shows more
 * precision for small values so a $0.0003 charge isn't rendered as "$0.00".
 */
export function formatMicrosPrecise(micros: number): string {
	const dollars = (micros ?? 0) / 1_000_000;
	const abs = Math.abs(dollars);
	const digits = abs > 0 && abs < 0.01 ? 4 : 2;
	return new Intl.NumberFormat('en-US', {
		style: 'currency',
		currency: 'USD',
		minimumFractionDigits: 2,
		maximumFractionDigits: digits,
	}).format(dollars);
}
