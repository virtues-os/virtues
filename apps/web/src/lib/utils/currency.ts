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
 * Format a micros amount that can be tiny, such as one AI call. Ten cents and
 * up, and zero, show cents ("$0.46"). Under ten cents always shows four
 * places, so a column of calls lines up ("$0.0300" above "$0.0025"). A charge
 * too small for four places reads "<$0.0001", short enough for that column,
 * so it never looks free.
 */
export function formatMicrosPrecise(micros: number): string {
	const dollars = (micros ?? 0) / 1_000_000;
	const abs = Math.abs(dollars);
	if (abs > 0 && abs < 0.00005) return '<$0.0001';
	const places = abs === 0 || abs >= 0.1 ? 2 : 4;
	return new Intl.NumberFormat('en-US', {
		style: 'currency',
		currency: 'USD',
		minimumFractionDigits: places,
		maximumFractionDigits: places,
	}).format(dollars);
}
